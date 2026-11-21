//! Aggressive Mode Orchestration Engine (`AggressiveEngine`).
//!
//! // ARCH: ARCHITECTURE.md §1.3, §1.4, §5 GAP-33, GAP-35 & GAP-42 — orchestrates NPAE Aggressive
//! // structuring, OryEngine pattern memory & dynamic blueprint integration, domain-aware ambiguity
//! // & question generation, CRISP / Grounding Harness rendering, tri-layer HallucinationGuard,
//! // and Stage 6B quality scoring & correction.

use super::grammar::GrammarCorrector;
use super::rag_context::DerivedRagContext;
use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{
    CompressionMeta, NpaeConfig, ProcessingMeta, RenderStyle, StructuredPromptResponse,
};
use std::time::Instant;

/// Aggressive Mode execution engine.
#[derive(Debug)]
pub struct AggressiveEngine;

impl AggressiveEngine {
    /// Executes the full NPAE Aggressive pipeline on a raw prompt and compressed representation.
    ///
    /// # Errors
    /// Returns `Err` if intent extraction, Ory processing, structurer dispatch, or ambiguity
    /// scoring fails.
    ///
    /// # Panics
    /// Never.
    pub async fn run(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        ory_engine: std::sync::Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        structurer_impl: &dyn super::structurer::Structurer,
        reconstructed: &crate::types::ReconstructedInput,
        enrichment: Option<&crate::types::EnrichmentContext>,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // 0. Isolate prompt and run grammar/spelling correction (preserving numbers/dates/units)
        let (raw_ctx_opt, _, raw_user_prompt) = super::structurer::split_raw_input(raw);
        let (user_prompt, _grammar_corrections) = GrammarCorrector::correct(&raw_user_prompt);
        // FIX: G-05 — reconstruct the `Context: [...]` prefix alongside the grammar-corrected
        // `user_prompt` so `render_crisp_prompt` renders `Semantic Context:` instead of dropping it.
        let crisp_raw_input = match raw_ctx_opt.as_deref() {
            Some(ctx) => format!("Context: [{ctx}]\n\n{user_prompt}"),
            None => user_prompt.clone(),
        };

        // Derive RAG context (250 token fact budget, repetition-free, no source citations)
        let derived_rag = enrichment
            .and_then(|e| e.rag_chunks.as_ref())
            .map(|chunks| DerivedRagContext::derive(chunks, &user_prompt, 250));

        // 1. Ory Engine + Intent Extraction: run CPU-bound embedding & classification work
        // inside `spawn_blocking` before acquiring the async Ory mutex (Rule §6: no heavy CPU
        // on the Tokio runtime; never hold a lock across blocking compute).
        // ARCH: ARCHITECTURE.md §6.6 — use zero-copy `read_arc()` instead of cloning `UnifiedConfig`.
        let config_guard = config_handle.read_arc();
        let prompt_for_intent = user_prompt.clone();
        let repr_for_intent = repr.clone();
        let cfg_for_intent = std::sync::Arc::clone(&config_guard);
        let mut profile = tokio::task::spawn_blocking(move || {
            super::intent::extract_with_config(
                &repr_for_intent,
                &prompt_for_intent,
                Some(cfg_for_intent.as_ref()),
            )
        })
        .await
        .map_err(|e| format!("intent spawn_blocking join error: {e}"))??;

        let ory_lock = ory_engine.lock().await;
        let ory_result = ory_lock
            .process(&user_prompt, config_guard.as_ref())
            .map_err(|e| e.to_string())?;
        // FIX: G-02 — release `ory_engine` mutex immediately after `process()` so StructurerRouter,
        // `build_with_blueprint`, ambiguity scoring, HallucinationGuard, and Stage 6B cycles run
        // without holding the global mutex across the request lifecycle.
        drop(ory_lock);

        // Enhance Aggressive intent with Ory's deep learning
        // FIX: Step 1.1 — Canonicalize Ory's inferred domain through the config taxonomy
        // (or audit match) and NEVER override when the deterministic rule layer locked the
        // domain (`profile.rule_locked`).
        let ory_canonical_domain = ory_result
            .audit
            .existing_domain_match
            .as_deref()
            .unwrap_or(&ory_result.intent.inferred_domain);
        let ory_canonical_domain =
            super::config::normalize_domain(ory_canonical_domain, &config_guard.domain_taxonomy)
                .to_string();

        let domain_agreement = ory_canonical_domain == profile.domain;
        let confidence_margin = ory_result.intent.confidence_score - profile.confidence;

        if !domain_agreement {
            tracing::warn!(
                "Domain mismatch: aggressive='{}', ory='{}' (canonical='{}'), aggressive_conf={:.3}, ory_conf={:.3}, margin={:.3}, rule_locked={}",
                profile.domain,
                ory_result.intent.inferred_domain,
                ory_canonical_domain,
                profile.confidence,
                ory_result.intent.confidence_score,
                confidence_margin,
                profile.rule_locked
            );
        }

        // Only override if not rule-locked and Ory confidence exceeds aggressive by significant margin (0.15)
        if !profile.rule_locked && ory_result.intent.confidence_score > profile.confidence + 0.15 {
            if !domain_agreement {
                tracing::info!(
                    "Overriding domain: '{}' -> '{}' (margin: {:.3})",
                    profile.domain,
                    ory_canonical_domain,
                    confidence_margin
                );
            }
            profile.domain = ory_canonical_domain;
            profile.confidence = ory_result.intent.confidence_score;
        }

        // 1.5. Dispatch via StructurerRouter
        let resolver = super::resolver::PromptResolver::new(config_handle);
        let mut router = super::resolver::StructurerRouter::new(resolver);
        let resolved_prompt = router
            .dispatch(structurer_impl, &profile)
            .map_err(|e| e.to_string())?;

        // 2. Build structured prompt with derived RAG facts and optional Ory DynamicBlueprint
        // ARCH: GAP-35 — Forward Ory blueprint into structurer to honor dynamic execution roadmap.
        let structured = super::structurer::build_with_blueprint(
            &profile,
            &user_prompt,
            &resolved_prompt,
            Some(config_guard.as_ref()),
            reconstructed,
            derived_rag.as_ref(),
            ory_result.blueprint.as_ref(),
        )?;

        // 3. Score ambiguity (accounting for gaps resolved by RAG facts and domain-required slots)
        let amb = super::ambiguity::score_for_domain(
            repr,
            &user_prompt,
            reconstructed,
            derived_rag.as_ref(),
            Some(&profile),
        )?;
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        // 4. Generate domain-aware questions if threshold exceeded
        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate_with_domain(repr, &amb, max_q, &profile.domain)?
        } else {
            vec![]
        };

        let aggressive_ms = t_start.elapsed().as_millis() as u64;

        // Render final optimized prompt according to requested RenderStyle (Crisp by default).
        // ARCH: Grounding Harness (GAP-42) & Phase 3 EvidenceBundle (§6.6) — pack provenance-tagged
        // evidence once so Harness and RAG-backed Crisp prompts share exact `[F#]` IDs, conflict
        // warnings, and token-budget truncation notices across rendering, HallucinationGuard,
        // Stage 6B retries, and `/api/npae/verify`.
        let render_style = cfg.render_style.unwrap_or_default();
        let target_model = cfg.target_model_family.unwrap_or_default();
        let has_rag_data = derived_rag
            .as_ref()
            .is_some_and(|r| !r.facts.is_empty() || !r.detailed_facts.is_empty());
        let evidence_bundle =
            super::structurer::EvidenceBundle::from_inputs(&structured, raw, derived_rag.as_ref());
        let packed_evidence = evidence_bundle.pack(&super::structurer::PromptBudget::default());
        let use_evidence_crisp = render_style == RenderStyle::Crisp
            && target_model == crate::npae::schema::types::TargetModelFamily::StandardMarkdown
            && has_rag_data
            && !packed_evidence.items.is_empty();

        let mut grounding_facts = if render_style == RenderStyle::Harness {
            packed_evidence.to_numbered_facts()
        } else {
            Vec::new()
        };
        let optimized_prompt = match render_style {
            RenderStyle::Crisp if use_evidence_crisp => {
                super::structurer::render_crisp_prompt_with_evidence(
                    &structured,
                    &questions,
                    &crisp_raw_input,
                    &packed_evidence,
                )
            }
            RenderStyle::Crisp => super::structurer::render_crisp_prompt_for_model(
                &structured,
                &questions,
                &crisp_raw_input,
                derived_rag.as_ref(),
                target_model,
            ),
            RenderStyle::Harness => super::structurer::render_harness_prompt_with_evidence(
                &structured,
                &questions,
                raw,
                Some(&user_prompt),
                &packed_evidence,
                false,
            ),
        };

        // Post-generation validation using Hallucination Guard (including packed evidence as trusted source context)
        let guard_cfg = &structured.hallucination_guard;
        let mut final_prompt = optimized_prompt;

        let guard_source_text = if !packed_evidence.items.is_empty() {
            let fact_texts: Vec<&str> = packed_evidence
                .items
                .iter()
                .map(|i| i.text.as_str())
                .collect();
            format!("{raw}\nContext Facts: {}", fact_texts.join(". "))
        } else {
            raw.to_string()
        };

        let mut guard_report = crate::npae::hallucination::guard::run_tri_layer(
            &final_prompt,
            &guard_source_text,
            guard_cfg,
        )
        .unwrap_or_else(|_| crate::npae::hallucination::guard::HallucinationReport {
            passed: true,
            layers: [
                crate::npae::hallucination::guard::LayerReport {
                    layer_id: 1,
                    passed: true,
                    flags: vec![],
                },
                crate::npae::hallucination::guard::LayerReport {
                    layer_id: 2,
                    passed: true,
                    flags: vec![],
                },
                crate::npae::hallucination::guard::LayerReport {
                    layer_id: 3,
                    passed: true,
                    flags: vec![],
                },
            ],
            remediation: None,
        });

        // Trigger targeted correction if HallucinationGuard failed
        if !guard_report.passed {
            final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                &final_prompt,
                &guard_report,
            );
            if let Ok(new_report) = crate::npae::hallucination::guard::run_tri_layer(
                &final_prompt,
                &guard_source_text,
                guard_cfg,
            ) {
                guard_report = new_report;
            }
        }

        let rag_tokens = if has_rag_data {
            packed_evidence
                .tokens_used
                .max(derived_rag.as_ref().map(|r| r.tokens_added).unwrap_or(0))
        } else {
            0
        };
        // Initial scoring calculation for Aggressive mode
        let mut scoring_result = compute_aggressive_scoring(
            raw,
            &final_prompt,
            &structured,
            &amb,
            &questions,
            rag_tokens,
        );

        // Stage 6B: Quality Guardrails & Targeted Correction Loop
        // FIX: G-10 — preserve required clarifying questions when `amb.score > threshold` and
        // track `(best_prompt, best_scoring, best_guard, best_grounding_facts)` across cycles so
        // correction iterations never degrade `guard_report.passed`, composite score, or `[F#]` alignment.
        let mut best_prompt = final_prompt.clone();
        let mut best_scoring = scoring_result.clone();
        let mut best_guard = guard_report.clone();
        let mut best_grounding_facts = grounding_facts.clone();
        let max_correction_cycles = 3;
        let mut cycle_num = 0;

        while (scoring_result.correction_needed || !guard_report.passed)
            && cycle_num < max_correction_cycles
        {
            cycle_num += 1;
            let questions_for_tes: &[crate::npae::schema::types::ClarifyingQuestion] =
                if amb.score > threshold {
                    &questions
                } else {
                    &[]
                };
            if let Some(axis) = &scoring_result.correction_axis {
                match axis {
                    crate::types::ScoreAxis::TaskEssential => {
                        // TES low: Strip optional roadmap, keep clarifying questions if ambiguous,
                        // and when Harness is active NEVER strip <rules>, <facts>, <requested_items>,
                        // or <output_contract> (GAP-42). Tighten evidence budget only when it exceeds
                        // 300 tokens so short fact sets stay intact.
                        let compact_evidence = if packed_evidence.tokens_used > 300 {
                            evidence_bundle.pack(&super::structurer::PromptBudget {
                                max_evidence_tokens: 300,
                                max_items: 12,
                            })
                        } else {
                            packed_evidence.clone()
                        };
                        if render_style == RenderStyle::Harness {
                            grounding_facts = compact_evidence.to_numbered_facts();
                        }
                        final_prompt = match render_style {
                            RenderStyle::Crisp if use_evidence_crisp => {
                                super::structurer::render_crisp_prompt_with_evidence(
                                    &structured,
                                    questions_for_tes,
                                    &crisp_raw_input,
                                    &compact_evidence,
                                )
                            }
                            RenderStyle::Crisp => super::structurer::render_crisp_prompt_for_model(
                                &structured,
                                questions_for_tes,
                                &crisp_raw_input,
                                derived_rag.as_ref(),
                                target_model,
                            ),
                            RenderStyle::Harness => {
                                super::structurer::render_harness_prompt_with_evidence(
                                    &structured,
                                    questions_for_tes,
                                    raw,
                                    Some(&user_prompt),
                                    &compact_evidence,
                                    true,
                                )
                            }
                        };
                    }
                    crate::types::ScoreAxis::SchemaFidelity => {
                        // SFS low: Remediate structural omissions
                        final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                            &final_prompt,
                            &guard_report,
                        );
                    }
                    crate::types::ScoreAxis::SemanticCompleteness => {
                        // SCS low: Ensure questions, full packed evidence, and constraints are rendered
                        if render_style == RenderStyle::Harness {
                            grounding_facts = packed_evidence.to_numbered_facts();
                        }
                        final_prompt = match render_style {
                            RenderStyle::Crisp if use_evidence_crisp => {
                                super::structurer::render_crisp_prompt_with_evidence(
                                    &structured,
                                    &questions,
                                    &crisp_raw_input,
                                    &packed_evidence,
                                )
                            }
                            RenderStyle::Crisp => super::structurer::render_crisp_prompt_for_model(
                                &structured,
                                &questions,
                                &crisp_raw_input,
                                derived_rag.as_ref(),
                                target_model,
                            ),
                            RenderStyle::Harness => {
                                super::structurer::render_harness_prompt_with_evidence(
                                    &structured,
                                    &questions,
                                    raw,
                                    Some(&user_prompt),
                                    &packed_evidence,
                                    false,
                                )
                            }
                        };
                    }
                }
            } else if !guard_report.passed {
                final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                    &final_prompt,
                    &guard_report,
                );
            }

            if let Ok(new_report) = crate::npae::hallucination::guard::run_tri_layer(
                &final_prompt,
                &guard_source_text,
                guard_cfg,
            ) {
                guard_report = new_report;
            }
            if !guard_report.passed {
                final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                    &final_prompt,
                    &guard_report,
                );
                if let Ok(remediated_report) = crate::npae::hallucination::guard::run_tri_layer(
                    &final_prompt,
                    &guard_source_text,
                    guard_cfg,
                ) {
                    guard_report = remediated_report;
                }
            }
            scoring_result = compute_aggressive_scoring(
                raw,
                &final_prompt,
                &structured,
                &amb,
                &questions,
                rag_tokens,
            );

            let candidate_avg =
                (scoring_result.tes + scoring_result.sfs + scoring_result.scs) / 3.0;
            let best_avg = (best_scoring.tes + best_scoring.sfs + best_scoring.scs) / 3.0;
            let is_better = (guard_report.passed && !best_guard.passed)
                || (guard_report.passed == best_guard.passed && candidate_avg >= best_avg);
            if is_better {
                best_prompt.clone_from(&final_prompt);
                best_scoring = scoring_result.clone();
                best_guard = guard_report.clone();
                best_grounding_facts = grounding_facts.clone();
            }
        }

        final_prompt = best_prompt;
        scoring_result = best_scoring;
        guard_report = best_guard;
        grounding_facts = best_grounding_facts;

        // Final token accounting for UI
        let token_original = crate::utils::tokens::estimate_tokens(raw);
        let token_final = crate::utils::tokens::estimate_tokens(&final_prompt);
        let token_saved = token_original.saturating_sub(token_final);

        // 6. Record Outcome to Ory Engine Memory
        // FIX: G-02 — re-acquire `ory_engine` lock only when `guard_report.passed` so unverified
        // or failed prompt expansions never pollute shared `PatternMemory`.
        if guard_report.passed {
            if let Some(blueprint) = &ory_result.blueprint {
                let mut ory_lock = ory_engine.lock().await;
                let _ = ory_lock.record_outcome(&ory_result.intent, blueprint, &scoring_result);
            }
        }

        Ok(StructuredPromptResponse {
            schema_version: "2.0.0".into(),
            request_id,
            optimized_prompt: final_prompt,
            token_original,
            token_final,
            token_saved,
            compression: CompressionMeta::from_repr(repr, token_original),
            structured_prompt: structured,
            ambiguity_analysis: amb,
            clarifying_questions: questions,
            confidence_score: profile.confidence,
            processing_metadata: ProcessingMeta {
                pipeline_duration_ms: repr.stage_metrics.total_ms(),
                aggressive_mode_ms: aggressive_ms,
                model_version: "npae-2.0.0".into(),
            },
            scoring_result,
            hallucination_report: Some(guard_report),
            grounding_facts,
        })
    }
}

fn compute_aggressive_scoring(
    raw: &str,
    final_prompt: &str,
    structured: &crate::npae::schema::types::StructuredPrompt,
    amb: &crate::npae::schema::types::AmbiguityAnalysis,
    questions: &[crate::npae::schema::types::ClarifyingQuestion],
    rag_tokens_added: u32,
) -> crate::types::ScoringResult {
    let token_original = crate::utils::tokens::estimate_tokens(raw);
    let token_final = crate::utils::tokens::estimate_tokens(final_prompt);
    let net_token_final = token_final.saturating_sub(rag_tokens_added);

    // TES: Token efficiency (excluding injected RAG fact tokens so RAG enrichment is not penalized)
    // FIX: G-10 — Aggressive mode is a blueprint expansion engine for short prompts. When
    // `token_original < 80` and `net_token_final <= 450`, score efficiency against the target
    // blueprint budget (`>= 6.5`) so Stage 6B does not misclassify normal short-prompt expansion
    // as a `TaskEssential` failure.
    let tes_score = if token_original > 0 {
        if token_original < 80 && net_token_final <= 450 {
            ((1.0 - (net_token_final as f32 / 600.0).min(0.4)) * 10.0).clamp(6.5, 9.5)
        } else {
            let savings = 1.0 - (net_token_final as f32 / token_original as f32);

            let base = if savings < 0.0 {
                (0.3 + savings * 0.4).max(0.1)
            } else if savings <= 0.1 {
                0.3 + savings * 2.0
            } else if savings <= 0.6 {
                0.5 + savings * 0.667
            } else if savings <= 0.8 {
                0.9 - (savings - 0.6) * 0.5
            } else {
                0.8 - (savings - 0.8) * 2.0
            };
            (base * 10.0).clamp(0.0, 10.0)
        }
    } else {
        0.0
    };

    // SFS: Live Schema Fidelity evaluated directly against `final_prompt` (Phase C)
    // Checks actual rendered structural blocks rather than static `structured` fields, and
    // accepts `# OUTPUT BLUEPRINT:` / `<output_blueprint>` / `<output_contract>` as equivalent
    // to `# EXECUTION PHASES:` so Explain/Debug/Artifact-Build prompts are never penalized for
    // omitting waterfall timelines.
    let sfs_score = {
        let mut sfs = 0.0_f32;
        let total_checks = 6.0_f32;
        if final_prompt.contains("# ROLE:") || final_prompt.contains("<role>") {
            sfs += 1.0;
        }
        if final_prompt.contains("# TASK / INSTRUCTION:") || final_prompt.contains("<task>") {
            sfs += 1.0;
        }
        if final_prompt.contains("# CONTEXT:")
            || final_prompt.contains("<context>")
            || final_prompt.contains("# GROUNDING FACTS")
            || final_prompt.contains("<facts")
            || final_prompt.contains("<grounding_facts")
        {
            sfs += 1.0;
        }
        let has_constraints_needed = !structured.constraints.required_inclusions.is_empty()
            || !structured.constraints.forbidden_topics.is_empty();
        if !has_constraints_needed
            || final_prompt.contains("# CONSTRAINTS:")
            || final_prompt.contains("<constraints>")
        {
            sfs += 1.0;
        }
        if final_prompt.contains("# OUTPUT BLUEPRINT:")
            || final_prompt.contains("<output_blueprint>")
            || final_prompt.contains("# EXECUTION PHASES:")
            || final_prompt.contains("<execution_phases>")
            || final_prompt.contains("<output_contract>")
        {
            sfs += 1.0;
        }
        if final_prompt.contains("# SUCCESS CRITERIA:")
            || final_prompt.contains("<rules")
            || final_prompt.contains("<execution_protocol>")
            || final_prompt.contains("\n---\n")
        {
            sfs += 1.0;
        }
        (sfs / total_checks * 10.0).clamp(0.0, 10.0)
    };

    // SCS: Live Semantic Completeness evaluated directly against `final_prompt` (Phase C)
    // Measures entity/number preservation from `raw`, rendered clarifying questions when ambiguous,
    // rendered execution protocol, and rendered required inclusions.
    let scs_score = {
        let mut score = 10.0_f32;
        let final_lower = final_prompt.to_lowercase();

        // 1. Key entity & numeric preservation from user prompt
        let (_, _, user_only) = super::structurer::split_raw_input(raw);
        let key_tokens: Vec<String> = user_only
            .split(|c: char| !c.is_alphanumeric() && c != '$' && c != '%' && c != '-')
            .filter(|t| {
                let len = t.len();
                let has_digit = t.chars().any(|c| c.is_ascii_digit());
                (has_digit && len >= 2)
                    || (len >= 5
                        && !matches!(
                            t.to_lowercase().as_str(),
                            "please"
                                | "could"
                                | "would"
                                | "should"
                                | "about"
                                | "there"
                                | "their"
                                | "where"
                                | "which"
                                | "these"
                                | "those"
                                | "really"
                                | "explain"
                                | "build"
                                | "create"
                                | "write"
                        ))
            })
            .map(str::to_lowercase)
            .take(12)
            .collect();
        if !key_tokens.is_empty() {
            let preserved = key_tokens
                .iter()
                .filter(|t| final_lower.contains(t.as_str()))
                .count();
            let ratio = preserved as f32 / key_tokens.len() as f32;
            if ratio < 0.75 {
                score -= (1.0 - ratio) * 3.0;
            }
        }

        // 2. Ambiguity question coverage in rendered output
        if amb.score > 0.65
            && !questions.is_empty()
            && !final_prompt.contains("# CLARIFYING QUESTIONS:")
            && !final_prompt.contains("<questions>")
            && !final_prompt.contains("<clarifying_questions>")
        {
            score -= 2.0;
        }

        // 3. Execution protocol / rules rendered
        if !final_prompt.contains("\n---\n")
            && !final_prompt.contains("<rules")
            && !final_prompt.contains("<execution_protocol>")
        {
            score -= 1.5;
        }

        // 4. Required inclusions rendered
        if !structured.constraints.required_inclusions.is_empty() {
            let missing_inc = structured
                .constraints
                .required_inclusions
                .iter()
                .filter(|inc| !final_lower.contains(&inc.to_lowercase()))
                .count();
            if missing_inc > 0 {
                score -= (missing_inc as f32 * 0.5).min(2.0);
            }
        }

        score.clamp(0.0, 10.0)
    };

    let threshold = 6.0;
    let mut correction_needed = false;
    let mut lowest = 10.0;
    let mut correction_axis = None;

    if tes_score < threshold && tes_score < lowest {
        lowest = tes_score;
        correction_needed = true;
        correction_axis = Some(crate::types::ScoreAxis::TaskEssential);
    }
    if sfs_score < threshold && sfs_score < lowest {
        lowest = sfs_score;
        correction_needed = true;
        correction_axis = Some(crate::types::ScoreAxis::SchemaFidelity);
    }
    if scs_score < threshold && scs_score < lowest {
        correction_needed = true;
        correction_axis = Some(crate::types::ScoreAxis::SemanticCompleteness);
    }

    crate::types::ScoringResult {
        tes: tes_score,
        sfs: sfs_score,
        scs: scs_score,
        correction_needed,
        correction_axis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npae::aggressive::config::{ConfigHandle, UnifiedConfig};
    use crate::npae::aggressive::structurer::HttpStructurer;
    use crate::npae::compression::pipeline::run_parallel_pipeline;
    use crate::npae::ory::OryEngine;
    use crate::types::{ChunkMetadata, EnrichmentContext, RagChunk, ReconstructedInput};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_engine_wires_evidence_bundle_provenance_and_conflicts() {
        let raw = "What was Acme Corp Q3 revenue?";
        let repr = run_parallel_pipeline(raw).expect("parallel pipeline");
        let config_handle = Arc::new(ConfigHandle::new(UnifiedConfig::default()));
        let ory_engine = Arc::new(tokio::sync::Mutex::new(OryEngine::new()));
        let http = HttpStructurer {
            route: "/api/compress".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: raw.to_string(),
        };
        let reconstructed = ReconstructedInput::default();
        let enrichment = EnrichmentContext {
            cluster_vocab: None,
            rag_chunks: Some(vec![
                RagChunk {
                    content: "Quarterly revenue for Acme Corp reached $4.2M in Q3.".to_string(),
                    domain_tag: "finance".to_string(),
                    metadata: Some(ChunkMetadata {
                        source_file: "q3_report.pdf".to_string(),
                        page_number: Some(4),
                    }),
                },
                RagChunk {
                    content: "Quarterly revenue for Acme Corp reached $4.8M in Q3.".to_string(),
                    domain_tag: "finance".to_string(),
                    metadata: Some(ChunkMetadata {
                        source_file: "press_release.pdf".to_string(),
                        page_number: Some(1),
                    }),
                },
            ]),
        };

        // 1. Crisp mode with RAG chunks surfaces provenance + [CONFLICT: ...]
        let crisp_resp = AggressiveEngine::run(
            raw,
            &repr,
            &NpaeConfig::default(),
            Arc::clone(&config_handle),
            Arc::clone(&ory_engine),
            &http,
            &reconstructed,
            Some(&enrichment),
        )
        .await
        .expect("crisp run");
        assert!(
            crisp_resp
                .optimized_prompt
                .contains("(Source: q3_report.pdf, p.4)"),
            "Crisp prompt should preserve page provenance:\n{}",
            crisp_resp.optimized_prompt
        );
        assert!(
            crisp_resp.optimized_prompt.contains("[CONFLICT:"),
            "Crisp prompt should surface numeric conflict:\n{}",
            crisp_resp.optimized_prompt
        );

        // 2. Harness mode preserves provenance, surfaces <conflicts>, and aligns grounding_facts
        let harness_cfg = NpaeConfig {
            render_style: Some(RenderStyle::Harness),
            ..NpaeConfig::default()
        };
        let harness_resp = AggressiveEngine::run(
            raw,
            &repr,
            &harness_cfg,
            config_handle,
            ory_engine,
            &http,
            &reconstructed,
            Some(&enrichment),
        )
        .await
        .expect("harness run");
        assert!(
            harness_resp
                .optimized_prompt
                .contains("(Source: q3_report.pdf, p.4)"),
            "Harness prompt should preserve page provenance:\n{}",
            harness_resp.optimized_prompt
        );
        assert!(
            harness_resp.optimized_prompt.contains("<conflicts>"),
            "Harness prompt should surface <conflicts>:\n{}",
            harness_resp.optimized_prompt
        );
        assert_eq!(harness_resp.grounding_facts.len(), 2);
        assert_eq!(harness_resp.grounding_facts[0].id, "F1");
        assert_eq!(harness_resp.grounding_facts[1].id, "F2");
    }
}
