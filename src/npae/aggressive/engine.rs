use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{NpaeConfig, StructuredPromptResponse, CompressionMeta, ProcessingMeta};
use std::time::Instant;

pub struct AggressiveEngine;

impl AggressiveEngine {
    pub async fn run(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        ory_engine: std::sync::Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        structurer_impl: &dyn super::structurer::Structurer,
        reconstructed: &crate::types::ReconstructedInput,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // Isolate the actual user request from the RAG chunks and semantic context
        let (_, _, user_prompt) = super::structurer::split_raw_input(raw);

        // 1. Ory Engine: Meta-Orchestration and Deep Learning (using isolated prompt)
        let mut ory_lock = ory_engine.lock().await;
        let config_guard = config_handle.read();
        let ory_result = ory_lock.process(&user_prompt, &config_guard).map_err(|e| e.to_string())?;
        
        let mut profile = super::intent::extract_with_config(repr, &user_prompt, Some(&config_guard))?;
        
        // Enhance Aggressive intent with Ory's deep learning
        let domain_agreement = ory_result.intent.inferred_domain == profile.domain;
        let confidence_margin = ory_result.intent.confidence_score - profile.confidence;

        if !domain_agreement {
            tracing::warn!(
                "Domain mismatch: aggressive='{}', ory='{}', aggressive_conf={:.3}, ory_conf={:.3}, margin={:.3}",
                profile.domain,
                ory_result.intent.inferred_domain,
                profile.confidence,
                ory_result.intent.confidence_score,
                confidence_margin
            );
        }

        // Only override if Ory confidence exceeds aggressive by significant margin (0.15)
        // This prevents low-confidence Ory predictions from overriding high-confidence aggressive predictions
        if ory_result.intent.confidence_score > profile.confidence + 0.15 {
            if !domain_agreement {
                tracing::info!(
                    "Overriding domain: '{}' -> '{}' (margin: {:.3})",
                    profile.domain,
                    ory_result.intent.inferred_domain,
                    confidence_margin
                );
            }
            profile.domain = ory_result.intent.inferred_domain.clone();
            profile.confidence = ory_result.intent.confidence_score;
        }

        // 1.5. Dispatch via StructurerRouter
        let resolver = super::resolver::PromptResolver::new(config_handle);
        let mut router = super::resolver::StructurerRouter::new(resolver);
        let resolved_prompt = router.dispatch(structurer_impl, &profile).map_err(|e| e.to_string())?;

        // 2. Build structured prompt (using isolated prompt so inference isn't confused by RAG chunks)
        let structured = super::structurer::build(&profile, &user_prompt, &resolved_prompt, Some(&config_guard), reconstructed)?;


        // 3. Score ambiguity
        let amb = super::ambiguity::score(repr, &user_prompt, reconstructed)?;
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        // 4. Generate domain-aware questions if threshold exceeded
        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate_with_domain(repr, &amb, max_q, &profile.domain)?
        } else {
            vec![]
        };

        let aggressive_ms = t_start.elapsed().as_millis() as u64;

        // Render final optimized prompt for UI
        let optimized_prompt = super::structurer::render_crisp_prompt(&structured, &questions, raw);
        
        // Post-generation validation using Hallucination Guard
        let guard_cfg = &structured.hallucination_guard;
        let mut final_prompt = optimized_prompt;
        let mut guard_report = crate::npae::hallucination::guard::run_tri_layer(&final_prompt, raw, guard_cfg)
            .unwrap_or_else(|_| crate::npae::hallucination::guard::HallucinationReport {
                passed: true,
                layers: [
                    crate::npae::hallucination::guard::LayerReport { layer_id: 1, passed: true, flags: vec![] },
                    crate::npae::hallucination::guard::LayerReport { layer_id: 2, passed: true, flags: vec![] },
                    crate::npae::hallucination::guard::LayerReport { layer_id: 3, passed: true, flags: vec![] },
                ],
                remediation: None,
            });

        // Trigger targeted correction if HallucinationGuard failed
        if !guard_report.passed {
            final_prompt = crate::npae::hallucination::guard::remediate_hallucination(&final_prompt, &guard_report);
            if let Ok(new_report) = crate::npae::hallucination::guard::run_tri_layer(&final_prompt, raw, guard_cfg) {
                guard_report = new_report;
            }
        }
        
        // Initial scoring calculation for Aggressive mode
        let mut scoring_result = compute_aggressive_scoring(raw, &final_prompt, &structured, &amb, &questions);

        // Stage 6B: Quality Guardrails & Targeted Correction Loop
        let max_correction_cycles = 3;
        let mut cycle_num = 0;

        while (scoring_result.correction_needed || !guard_report.passed) && cycle_num < max_correction_cycles {
            cycle_num += 1;
            if let Some(axis) = &scoring_result.correction_axis {
                match axis {
                    crate::types::ScoreAxis::TaskEssential => {
                        // TES low: Strip non-essential questions / render crisp compact prompt
                        final_prompt = super::structurer::render_crisp_prompt(&structured, &[], raw);
                    }
                    crate::types::ScoreAxis::SchemaFidelity => {
                        // SFS low: Remediate structural omissions
                        final_prompt = crate::npae::hallucination::guard::remediate_hallucination(&final_prompt, &guard_report);
                    }
                    crate::types::ScoreAxis::SemanticCompleteness => {
                        // SCS low: Ensure questions and constraints are fully rendered
                        final_prompt = super::structurer::render_crisp_prompt(&structured, &questions, raw);
                    }
                }
            } else if !guard_report.passed {
                final_prompt = crate::npae::hallucination::guard::remediate_hallucination(&final_prompt, &guard_report);
            }

            if let Ok(new_report) = crate::npae::hallucination::guard::run_tri_layer(&final_prompt, raw, guard_cfg) {
                guard_report = new_report;
            }
            scoring_result = compute_aggressive_scoring(raw, &final_prompt, &structured, &amb, &questions);
        }

        // Final token accounting for UI
        let token_original = crate::utils::tokens::estimate_tokens(raw);
        let token_final = crate::utils::tokens::estimate_tokens(&final_prompt);
        let token_saved = token_original.saturating_sub(token_final);

        // 6. Record Outcome to Ory Engine Memory
        if let Some(blueprint) = &ory_result.blueprint {
            let _ = ory_lock.record_outcome(&ory_result.intent, blueprint, &scoring_result);
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
        })
    }
}

fn compute_aggressive_scoring(
    raw: &str,
    final_prompt: &str,
    structured: &crate::npae::schema::types::StructuredPrompt,
    amb: &crate::npae::schema::types::AmbiguityAnalysis,
    questions: &[crate::npae::schema::types::ClarifyingQuestion],
) -> crate::types::ScoringResult {
    let token_original = crate::utils::tokens::estimate_tokens(raw);
    let token_final = crate::utils::tokens::estimate_tokens(final_prompt);

    // TES: Token efficiency
    let tes_score = if token_original > 0 {
        let savings = 1.0 - (token_final as f32 / token_original as f32);
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
    } else {
        0.0
    };

    // SFS: Schema Fidelity
    let sfs_score = {
        let mut sfs = 0.0_f32;
        let total_checks = 7.0_f32;
        if !structured.role.primary.is_empty() { sfs += 1.0; }
        if !structured.role.persona_anchor.is_empty() { sfs += 1.0; }
        if !structured.context.description.is_empty() { sfs += 1.0; }
        if !structured.context.background.is_empty() { sfs += 1.0; }
        if !structured.constraints.required_inclusions.is_empty() || !structured.constraints.forbidden_topics.is_empty() { sfs += 1.0; }
        if !structured.execution_phases.is_empty() { sfs += 1.0; }
        if !structured.success_criteria.is_empty() { sfs += 1.0; }
        (sfs / total_checks * 10.0).clamp(0.0, 10.0)
    };

    // SCS: Semantic Completeness
    let scs_score = {
        let mut score = 10.0_f32;
        if amb.score > 0.5 && questions.is_empty() {
            score -= 2.0;
        }
        if structured.success_criteria.is_empty() {
            score -= 1.5;
        }
        if structured.dynamic_instruction.is_empty() {
            score -= 2.0;
        }
        if structured.constraints.required_inclusions.is_empty() && structured.constraints.forbidden_topics.is_empty() {
            score -= 1.0;
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
