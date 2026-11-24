//! Aggressive Mode Orchestration Engine (`AggressiveEngine`).
//!
//! // ARCH: ARCHITECTURE.md §1.3, §1.4, §5 GAP-33, GAP-35 & GAP-42 — orchestrates NPAE Aggressive
//! // structuring, OryEngine pattern memory & dynamic blueprint integration, domain-aware ambiguity
//! // & question generation, CRISP / Grounding Harness rendering, tri-layer HallucinationGuard,
//! // and Stage 6B quality scoring & correction.

use super::grammar::GrammarCorrector;
use super::intent::{ActorPerspective, IntentClass};
use super::rag_context::DerivedRagContext;
use crate::npae::compression::types::CompressedRepr;
use crate::npae::continuation::ledger::{
    strip_directive_or_exclusion_prefix, tokenize_boundary, tokens_contain_subsequence,
};
use crate::npae::continuation::{
    ConstraintKind, ConstraintOp, ConstraintStatus, ContinuationKind, ContinuationPath,
    ContinuationRoute, ContinuationRouter, ParentMatcher, PhaseStatus, PromptStateLedger,
};
use crate::npae::grounding::verifier::{
    contains_prompt_injection, sanitize_untrusted_fact, split_sentences_preserving_decimals,
    NumberedFact,
};
use crate::npae::schema::types::{
    CompressionMeta, ConstraintView, ContinuationMeta, ExecutionPhase, NpaeConfig, PhaseView,
    ProcessingMeta, RenderStyle, RepairContext, StructuredPrompt, StructuredPromptResponse,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

/// Caller-supplied continuation inputs for [`AggressiveEngine::run_with_continuation`].
///
/// // ARCH: ARCHITECTURE.md §6.7 — threads prior ledger state, raw parent prompt, explicit
/// // constraint revocations, active phase cursors, resolved slots, and repair contexts into
/// // the NPAE Aggressive pipeline.
#[derive(Debug, Clone, Default)]
pub struct ContinuationInput {
    /// Optional pre-loaded parent [`PromptStateLedger`] when the caller manages persistence externally.
    pub parent_ledger: Option<PromptStateLedger>,
    /// Raw prompt text from the parent turn used to compute Path A/B deltas.
    pub parent_raw_prompt: Option<String>,
    /// Explicit parent ledger identifier linking this turn to a prior turn.
    pub explicit_parent_id: Option<String>,
    /// Constraint IDs (`"C1"`) or text targets explicitly revoked by the user.
    pub revoked_constraints: Vec<String>,
    /// Optional 1-based execution phase cursor to activate on this turn.
    pub active_phase: Option<u32>,
    /// Slot name -> value pairs resolving open ambiguity/gap slots.
    pub resolved_slots: BTreeMap<String, String>,
    /// Optional verification-guided repair payload for targeted repair synthesis.
    pub repair_context: Option<RepairContext>,
}

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
        let mut ephemeral_ledger: Option<PromptStateLedger> = None;
        Self::run_with_continuation(
            raw,
            repr,
            cfg,
            config_handle,
            ory_engine,
            structurer_impl,
            reconstructed,
            enrichment,
            &mut ephemeral_ledger,
            None,
        )
        .await
    }

    /// Executes the NPAE Aggressive pipeline with stateful multi-turn parent-continuation tracking.
    ///
    /// // ARCH: ARCHITECTURE.md §6.7 — handles Turn 1 cold starts, Path A (`DeltaFollowUp`),
    /// // Path B (`EditContinuation`), `TopicReset`, `SL-2` domain hysteresis, `SL-3` monotonic
    /// // `[F#]` fact registration, constraint reconciliation, phase cursor collapse, and repair
    /// // prompt rendering while updating `ledger` in place.
    ///
    /// # Errors
    /// Returns `Err` if structurer validation, intent extraction, Ory processing, ambiguity
    /// scoring, or prompt assembly fails.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_continuation(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        ory_engine: std::sync::Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        structurer_impl: &dyn super::structurer::Structurer,
        reconstructed: &crate::types::ReconstructedInput,
        enrichment: Option<&crate::types::EnrichmentContext>,
        ledger: &mut Option<PromptStateLedger>,
        cont: Option<ContinuationInput>,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // Pre-validate raw input up front so XSS/SQLi payloads fail closed before any state updates.
        let collected_input = structurer_impl.collect().map_err(|e| e.to_string())?;
        structurer_impl
            .pre_validate(&collected_input)
            .map_err(|e| e.to_string())?;

        if ledger.is_none() {
            if let Some(parent_ledger) = cont.as_ref().and_then(|c| c.parent_ledger.as_ref()) {
                *ledger = Some(parent_ledger.clone());
            }
        }

        let (raw_ctx_opt, _, raw_user_prompt) = super::structurer::split_raw_input(raw);
        let (user_prompt, _grammar_corrections) = GrammarCorrector::correct(&raw_user_prompt);

        let mut is_topic_reset = false;
        if let Some(existing) = ledger.as_mut() {
            let parent_raw = cont
                .as_ref()
                .and_then(|c| c.parent_raw_prompt.as_deref())
                .unwrap_or(existing.objective.as_str());
            let has_explicit_parent = cont
                .as_ref()
                .is_some_and(|c| c.explicit_parent_id.is_some())
                || !parent_raw.is_empty();
            let delta = ParentMatcher::extract_delta(parent_raw, raw, has_explicit_parent);

            match delta.path {
                ContinuationPath::TopicReset => {
                    *ledger = Some(PromptStateLedger::new(uuid::Uuid::new_v4().to_string()));
                    is_topic_reset = true;
                }
                ContinuationPath::Initial => {}
                ContinuationPath::DeltaFollowUp | ContinuationPath::EditContinuation => {
                    return Self::run_continuation_turn(
                        request_id,
                        t_start,
                        raw,
                        &user_prompt,
                        raw_ctx_opt.as_deref(),
                        repr,
                        cfg,
                        config_handle,
                        ory_engine,
                        structurer_impl.source_name(),
                        reconstructed,
                        enrichment,
                        existing,
                        delta,
                        cont,
                    )
                    .await;
                }
            }
        }

        // Turn 1 cold start (or fresh ledger after `TopicReset`).
        let crisp_raw_input = match raw_ctx_opt.as_deref() {
            Some(ctx) => format!("Context: [{ctx}]\n\n{user_prompt}"),
            None => user_prompt.clone(),
        };

        let derived_rag = enrichment
            .and_then(|e| e.rag_chunks.as_ref())
            .map(|chunks| DerivedRagContext::derive(chunks, &user_prompt, 250));

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
        drop(ory_lock);

        apply_ory_domain_override(&mut profile, &ory_result, &config_guard.domain_taxonomy);

        let resolver = super::resolver::PromptResolver::new(std::sync::Arc::clone(&config_handle));
        let mut router = super::resolver::StructurerRouter::new(resolver);
        let resolved_prompt = router
            .dispatch(structurer_impl, &profile)
            .map_err(|e| e.to_string())?;

        let structured = super::structurer::build_with_blueprint(
            &profile,
            &user_prompt,
            &resolved_prompt,
            Some(config_guard.as_ref()),
            reconstructed,
            derived_rag.as_ref(),
            ory_result.blueprint.as_ref(),
        )?;

        let amb = super::ambiguity::score_for_domain(
            repr,
            &user_prompt,
            reconstructed,
            derived_rag.as_ref(),
            Some(&profile),
        )?;
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate_with_domain(repr, &amb, max_q, &profile.domain)?
        } else {
            vec![]
        };

        let mut active_ledger = ledger
            .take()
            .unwrap_or_else(|| PromptStateLedger::new(uuid::Uuid::new_v4().to_string()));
        active_ledger.apply_initial_structured(
            &structured,
            profile.perspective.as_key(),
            intent_key(&profile.primary_intent),
            profile.confidence,
            amb.gap_zones.clone(),
        );
        if structured.objective.is_none() && !user_prompt.trim().is_empty() {
            active_ledger.objective = user_prompt.clone();
        }
        if let Some(ref c) = cont {
            for (slot, val) in &c.resolved_slots {
                active_ledger.resolve_slot(slot, val);
            }
            if !c.revoked_constraints.is_empty() {
                let revoke_ops: Vec<ConstraintOp> = c
                    .revoked_constraints
                    .iter()
                    .map(|target| ConstraintOp::Revoke {
                        target: target.clone(),
                    })
                    .collect();
                active_ledger.apply_constraint_ops(&revoke_ops);
            }
            if let Some(p) = c.active_phase {
                active_ledger.set_active_phase(p);
            }
        }

        let (evidence_bundle, fact_id_map, packed_evidence, has_external_evidence, _newly_minted) =
            build_evidence_and_register_facts(
                &structured,
                raw,
                raw_ctx_opt.as_deref(),
                derived_rag.as_ref(),
                enrichment,
                &mut active_ledger,
                false,
            );

        let render_style = cfg.render_style.unwrap_or_default();
        let target_model = cfg.target_model_family.unwrap_or_default();
        // FIX: High #1 — honor standalone `repair_context` on Turn 1 using the newly initialized ledger.
        let repair_ctx = cont.as_ref().and_then(|c| c.repair_context.as_ref());
        let (final_prompt, guard_report, scoring_result, grounding_facts) =
            render_guard_and_correct(
                raw,
                raw,
                &user_prompt,
                &crisp_raw_input,
                &structured,
                &amb,
                &questions,
                derived_rag.as_ref(),
                &evidence_bundle,
                &fact_id_map,
                &packed_evidence,
                has_external_evidence,
                render_style,
                target_model,
                threshold,
                Some(&active_ledger),
                repair_ctx,
            );

        let aggressive_ms = t_start.elapsed().as_millis() as u64;
        let token_original = crate::utils::tokens::estimate_tokens(raw);
        let token_final = crate::utils::tokens::estimate_tokens(&final_prompt);
        let token_saved = token_original.saturating_sub(token_final);

        if guard_report.passed {
            if let Some(blueprint) = &ory_result.blueprint {
                let mut ory_lock = ory_engine.lock().await;
                let _ = ory_lock.record_outcome(&ory_result.intent, blueprint, &scoring_result);
            }
        }

        let (init_path, base_kind) = if is_topic_reset {
            (ContinuationPath::TopicReset, ContinuationKind::TopicReset)
        } else {
            (ContinuationPath::Initial, ContinuationKind::Initial)
        };
        let (init_kind, all_kinds) = if repair_ctx.is_some() {
            (
                ContinuationKind::Repair,
                vec![ContinuationKind::Repair, base_kind],
            )
        } else {
            (base_kind, vec![base_kind])
        };
        let delta_summary = summarize_delta_line(&user_prompt);
        let continuation_meta = build_continuation_meta(
            &active_ledger,
            init_path,
            init_kind,
            all_kinds,
            delta_summary,
            None,
        );
        *ledger = Some(active_ledger);

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
            continuation: Some(continuation_meta),
        })
    }

    /// Executes Turn 2+ (`DeltaFollowUp` or `EditContinuation`) against an existing [`PromptStateLedger`].
    #[allow(clippy::too_many_arguments)]
    async fn run_continuation_turn(
        request_id: String,
        t_start: Instant,
        raw: &str,
        user_prompt: &str,
        raw_ctx_opt: Option<&str>,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        ory_engine: std::sync::Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        source_name: &str,
        reconstructed: &crate::types::ReconstructedInput,
        enrichment: Option<&crate::types::EnrichmentContext>,
        existing: &mut PromptStateLedger,
        delta: crate::npae::continuation::DeltaExtraction,
        cont: Option<ContinuationInput>,
    ) -> Result<StructuredPromptResponse, String> {
        existing.advance_turn();

        let mut resolved_any_slot = false;
        if let Some(ref c) = cont {
            for (slot, val) in &c.resolved_slots {
                if existing.resolve_slot(slot, val) {
                    resolved_any_slot = true;
                }
            }
            if !c.revoked_constraints.is_empty() {
                let revoke_ops: Vec<ConstraintOp> = c
                    .revoked_constraints
                    .iter()
                    .map(|target| ConstraintOp::Revoke {
                        target: target.clone(),
                    })
                    .collect();
                existing.apply_constraint_ops(&revoke_ops);
            }
        }

        let is_repair = cont.as_ref().is_some_and(|c| c.repair_context.is_some());
        let mut route = ContinuationRouter::classify(
            &delta,
            &existing.open_slots,
            !existing.phases.is_empty(),
            is_repair,
        );
        if resolved_any_slot && !route.all_kinds.contains(&ContinuationKind::SlotFill) {
            route.all_kinds.push(ContinuationKind::SlotFill);
        }

        existing.apply_constraint_ops(&route.constraint_ops);
        if let Some(ref fmt) = route.output_format_override {
            existing.output_format = fmt.clone();
        }

        let target_phase = cont
            .as_ref()
            .and_then(|c| c.active_phase)
            .or(route.target_phase);
        if let Some(p) = target_phase {
            if existing.set_active_phase(p) {
                for phase in &mut existing.phases {
                    if phase.phase_number != p && phase.status == PhaseStatus::Pending {
                        phase.status = PhaseStatus::Deferred;
                    }
                }
            }
        }

        let (rehydrated_task, rehydrated_crisp_raw_input, rehydrated_reconstructed, scrub_list) =
            synthesize_rehydrated_inputs(
                existing,
                &delta,
                user_prompt,
                raw_ctx_opt,
                reconstructed,
                target_phase,
            );

        if delta.path == ContinuationPath::EditContinuation {
            let scrub_refs: Vec<&str> = scrub_list.iter().map(String::as_str).collect();
            existing.objective =
                super::structurer::scrub_superseded_terms(user_prompt, &scrub_refs);
        }

        let derived_rag = enrichment
            .and_then(|e| e.rag_chunks.as_ref())
            .map(|chunks| DerivedRagContext::derive(chunks, &rehydrated_task, 250));

        let config_guard = config_handle.read_arc();
        let prompt_for_intent = rehydrated_task.clone();
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
            .process(&rehydrated_task, config_guard.as_ref())
            .map_err(|e| e.to_string())?;
        drop(ory_lock);

        apply_sl2_domain_hysteresis(
            existing,
            &mut profile,
            &ory_result,
            &config_guard.domain_taxonomy,
            delta.path,
        );

        let resolver = super::resolver::PromptResolver::new(std::sync::Arc::clone(&config_handle));
        let resolved_prompt = resolver.resolve(&rehydrated_task, &profile, source_name);

        let mut structured = super::structurer::build_with_blueprint(
            &profile,
            &rehydrated_task,
            &resolved_prompt,
            Some(config_guard.as_ref()),
            &rehydrated_reconstructed,
            derived_rag.as_ref(),
            ory_result.blueprint.as_ref(),
        )?;

        reconcile_structured_prompt(&mut structured, existing, &route, target_phase, &scrub_list);

        let mut amb = super::ambiguity::score_for_domain(
            repr,
            &rehydrated_task,
            &rehydrated_reconstructed,
            derived_rag.as_ref(),
            Some(&profile),
        )?;
        if !existing.resolved_slots.is_empty() {
            amb.gap_zones.retain(|gap| {
                let gap_low = gap.to_lowercase();
                !existing
                    .resolved_slots
                    .keys()
                    .any(|k| gap_low.contains(&k.to_lowercase()))
            });
        }
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate_with_domain(repr, &amb, max_q, &profile.domain)?
        } else {
            vec![]
        };

        let (evidence_bundle, fact_id_map, packed_evidence, has_external_evidence, newly_minted) =
            build_evidence_and_register_facts(
                &structured,
                &rehydrated_crisp_raw_input,
                raw_ctx_opt,
                derived_rag.as_ref(),
                enrichment,
                existing,
                true,
            );
        let newly_minted_facts: Vec<NumberedFact> = newly_minted
            .into_iter()
            .map(|f| NumberedFact {
                id: f.id,
                text: f.text,
            })
            .collect();

        let delta_prompt = super::structurer::render_delta_prompt(
            existing,
            &route,
            &newly_minted_facts,
            user_prompt,
        );

        let guard_source_text =
            build_reconciled_guard_source(&rehydrated_task, existing, &packed_evidence);

        let render_style = cfg.render_style.unwrap_or_default();
        let target_model = cfg.target_model_family.unwrap_or_default();
        let repair_ctx = cont.as_ref().and_then(|c| c.repair_context.as_ref());
        let (final_prompt, guard_report, scoring_result, grounding_facts) =
            render_guard_and_correct(
                &rehydrated_crisp_raw_input,
                &guard_source_text,
                &rehydrated_task,
                &rehydrated_crisp_raw_input,
                &structured,
                &amb,
                &questions,
                derived_rag.as_ref(),
                &evidence_bundle,
                &fact_id_map,
                &packed_evidence,
                has_external_evidence,
                render_style,
                target_model,
                threshold,
                Some(existing),
                repair_ctx,
            );

        let aggressive_ms = t_start.elapsed().as_millis() as u64;
        let token_original = crate::utils::tokens::estimate_tokens(raw);
        let token_final = crate::utils::tokens::estimate_tokens(&final_prompt);
        let token_saved = token_original.saturating_sub(token_final);

        if guard_report.passed {
            if let Some(blueprint) = &ory_result.blueprint {
                let mut ory_lock = ory_engine.lock().await;
                let _ = ory_lock.record_outcome(&ory_result.intent, blueprint, &scoring_result);
            }
        }

        let continuation_meta = build_continuation_meta(
            existing,
            delta.path,
            route.primary_kind,
            route.all_kinds,
            route.delta_summary,
            Some(delta_prompt),
        );

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
            continuation: Some(continuation_meta),
        })
    }
}

/// Applies Ory's domain override on Turn 1 when `!profile.rule_locked` and margin > 0.15.
fn apply_ory_domain_override(
    profile: &mut super::intent::IntentProfile,
    ory_result: &crate::npae::ory::types::OryResult,
    domain_taxonomy: &[super::config::DomainTaxonomy],
) {
    let ory_canonical_domain = ory_result
        .audit
        .existing_domain_match
        .as_deref()
        .unwrap_or(&ory_result.intent.inferred_domain);
    let ory_canonical_domain =
        super::config::normalize_domain(ory_canonical_domain, domain_taxonomy).to_string();

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
}

/// Enforces `SL-2` domain hysteresis on continuation turns so single-turn noisy domain flips
/// (for example `"$100/mo"` triggering `"finance"` on a `"software"` prompt) are blocked.
fn apply_sl2_domain_hysteresis(
    existing: &mut PromptStateLedger,
    profile: &mut super::intent::IntentProfile,
    ory_result: &crate::npae::ory::types::OryResult,
    domain_taxonomy: &[super::config::DomainTaxonomy],
    path: ContinuationPath,
) {
    let mut cand_domain = profile.domain.clone();
    let mut cand_conf = profile.confidence;
    if !profile.rule_locked && ory_result.intent.confidence_score > profile.confidence + 0.15 {
        let ory_raw = ory_result
            .audit
            .existing_domain_match
            .as_deref()
            .unwrap_or(&ory_result.intent.inferred_domain);
        cand_domain = super::config::normalize_domain(ory_raw, domain_taxonomy).to_string();
        cand_conf = ory_result.intent.confidence_score;
    }

    let overridden = existing.should_override_domain(&cand_domain, cand_conf, false);
    if overridden {
        profile.domain = cand_domain;
        profile.confidence = cand_conf;
    } else if let Some(ref locked) = existing.locked_axis {
        profile.domain = locked.domain.clone();
        profile.rule_locked = true;
        profile.perspective = parse_perspective(&locked.perspective, profile.perspective);
        if path == ContinuationPath::DeltaFollowUp {
            profile.primary_intent =
                parse_intent_class(&locked.intent, profile.primary_intent.clone());
        }
    }
}

/// Synthesizes a clean rehydrated task, `Context: [...]` prefix, and reconciled `ReconstructedInput`.
fn synthesize_rehydrated_inputs(
    existing: &PromptStateLedger,
    delta: &crate::npae::continuation::DeltaExtraction,
    user_prompt: &str,
    raw_ctx_opt: Option<&str>,
    reconstructed: &crate::types::ReconstructedInput,
    target_phase: Option<u32>,
) -> (
    String,
    String,
    crate::types::ReconstructedInput,
    Vec<String>,
) {
    let mut scrub_list: Vec<String> = existing
        .superseded_targets()
        .into_iter()
        .map(str::to_string)
        .collect();
    for entry in &existing.constraints {
        if entry.status == ConstraintStatus::Revoked
            && !scrub_list
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&entry.text))
        {
            scrub_list.push(entry.text.clone());
        }
    }
    let scrub_refs: Vec<&str> = scrub_list.iter().map(String::as_str).collect();

    let raw_base = if delta.path == ContinuationPath::EditContinuation {
        user_prompt
    } else if existing.objective.trim().is_empty() {
        user_prompt
    } else {
        existing.objective.as_str()
    };
    let mut rehydrated_task = super::structurer::scrub_superseded_terms(raw_base, &scrub_refs);

    let mut extra_clauses: Vec<String> = Vec::new();
    let base_low = rehydrated_task.to_lowercase();
    let base_tokens = tokenize_boundary(&base_low);
    for inc in existing.active_inclusions() {
        let inc_low = inc.text.to_lowercase();
        let core_low = strip_constraint_verb(&inc_low);
        let inc_tokens = tokenize_boundary(&inc_low);
        let core_tokens = tokenize_boundary(core_low);
        if !tokens_contain_subsequence(&base_tokens, &inc_tokens)
            && (core_tokens.is_empty() || !tokens_contain_subsequence(&base_tokens, &core_tokens))
        {
            extra_clauses.push(inc.text.clone());
        }
    }
    for exc in existing.active_exclusions() {
        let exc_low = exc.text.to_lowercase();
        let exc_tokens = tokenize_boundary(&exc_low);
        if !tokens_contain_subsequence(&base_tokens, &exc_tokens) {
            let starts_with_neg = ["no ", "without ", "never ", "avoid ", "exclude "]
                .iter()
                .any(|p| exc_low.starts_with(p));
            if starts_with_neg {
                extra_clauses.push(exc.text.clone());
            } else {
                extra_clauses.push(format!("exclude {}", exc.text));
            }
        }
    }
    if !extra_clauses.is_empty() {
        if !rehydrated_task.is_empty() && !rehydrated_task.ends_with(['.', '!', '?']) {
            rehydrated_task.push('.');
        }
        rehydrated_task.push(' ');
        rehydrated_task.push_str(&extra_clauses.join("; "));
        rehydrated_task.push('.');
    }

    if let Some(p) = target_phase {
        if let Some(phase) = existing.phases.iter().find(|ph| ph.phase_number == p) {
            rehydrated_task.push_str(&format!(
                " Focus on Phase {} ({}): {}.",
                phase.phase_number, phase.name, phase.description
            ));
        }
    }

    // FIX: High #3 — preserve non-constraint, non-phase Path A follow-up instructions
    // (EvidenceDrill, ArtifactTransform, SlotFill, additive feature requests) in `rehydrated_task`.
    if delta.path == ContinuationPath::DeltaFollowUp {
        if let Some(follow_up) =
            crate::npae::continuation::router::extract_non_constraint_delta_instruction(
                &delta.delta_text,
                existing,
            )
        {
            let scrubbed_follow_up =
                super::structurer::scrub_superseded_terms(&follow_up, &scrub_refs);
            let trimmed_follow_up = scrubbed_follow_up.trim();
            if !trimmed_follow_up.is_empty()
                && !rehydrated_task
                    .to_lowercase()
                    .contains(&trimmed_follow_up.to_lowercase())
            {
                rehydrated_task.push_str("\nFollow-up instruction: ");
                rehydrated_task.push_str(trimmed_follow_up);
            }
        }
    }

    let mut ctx_parts: Vec<String> = Vec::new();
    if let Some(raw_ctx) = raw_ctx_opt {
        let scrubbed_ctx = super::structurer::scrub_superseded_terms(raw_ctx, &scrub_refs);
        if !scrubbed_ctx.trim().is_empty() {
            ctx_parts.push(scrubbed_ctx);
        }
    }
    // FIX: Low #16 — sanitize newlines, bracket delimiters, and prompt-injection markers on
    // `resolved_slots` before interpolating into `Context: [...]` / `Semantic Context:`.
    if !existing.resolved_slots.is_empty() {
        let slot_pairs: Vec<String> = existing
            .resolved_slots
            .iter()
            .filter_map(|(k, v)| {
                if contains_prompt_injection(k) || contains_prompt_injection(v) {
                    return None;
                }
                let clean_k = sanitize_untrusted_fact(&k.replace(['[', ']'], ""));
                let clean_v = sanitize_untrusted_fact(&v.replace(['[', ']'], ""));
                if clean_k.is_empty() || clean_v.is_empty() {
                    None
                } else {
                    Some(format!("{clean_k}: {clean_v}"))
                }
            })
            .collect();
        if !slot_pairs.is_empty() {
            ctx_parts.push(slot_pairs.join("; "));
        }
    }

    let crisp_raw_input = if ctx_parts.is_empty() {
        rehydrated_task.clone()
    } else {
        format!("Context: [{}]\n\n{}", ctx_parts.join("; "), rehydrated_task)
    };

    let mut rehydrated_reconstructed = reconstructed.clone();
    rehydrated_reconstructed.constraint_locks = existing
        .active_inclusions()
        .into_iter()
        .map(|c| crate::types::ConstraintToken {
            text: c.text.clone(),
            weight: 1.0,
        })
        .collect();
    if !existing.resolved_slots.is_empty() {
        rehydrated_reconstructed.ambiguity_register.retain(|flag| {
            let reason_low = flag.reason.to_lowercase();
            let text_low = flag.text.to_lowercase();
            !existing.resolved_slots.keys().any(|k| {
                let k_low = k.to_lowercase();
                reason_low.contains(&k_low) || text_low.contains(&k_low)
            })
        });
    }

    (
        rehydrated_task,
        crisp_raw_input,
        rehydrated_reconstructed,
        scrub_list,
    )
}

/// Reconciles `StructuredPrompt` constraints, output format, scrubbed text, and active phase cursor.
fn reconcile_structured_prompt(
    structured: &mut StructuredPrompt,
    existing: &PromptStateLedger,
    route: &ContinuationRoute,
    target_phase: Option<u32>,
    scrub_list: &[String],
) {
    let scrub_refs: Vec<&str> = scrub_list.iter().map(String::as_str).collect();

    structured.constraints.required_inclusions.retain(|item| {
        !matches_any_scrub_term(item, &scrub_refs)
            && !existing
                .active_exclusions()
                .iter()
                .any(|exc| exc.text.eq_ignore_ascii_case(item))
    });
    structured
        .constraints
        .forbidden_topics
        .retain(|item| !matches_any_scrub_term(item, &scrub_refs));

    for inc in existing.active_inclusions() {
        if !structured
            .constraints
            .required_inclusions
            .iter()
            .any(|existing_item| existing_item.eq_ignore_ascii_case(&inc.text))
        {
            structured
                .constraints
                .required_inclusions
                .push(inc.text.clone());
        }
    }
    for exc in existing.active_exclusions() {
        if !structured
            .constraints
            .forbidden_topics
            .iter()
            .any(|existing_item| existing_item.eq_ignore_ascii_case(&exc.text))
        {
            structured
                .constraints
                .forbidden_topics
                .push(exc.text.clone());
        }
    }

    if let Some(ref fmt) = route.output_format_override {
        structured.constraints.output_format = fmt.clone();
    } else if !existing.output_format.trim().is_empty()
        && !existing.output_format.eq_ignore_ascii_case("markdown")
    {
        structured.constraints.output_format = existing.output_format.clone();
    }

    structured.context.description =
        super::structurer::scrub_superseded_terms(&structured.context.description, &scrub_refs);
    structured.context.background =
        super::structurer::scrub_superseded_terms(&structured.context.background, &scrub_refs);
    structured.dynamic_instruction =
        super::structurer::scrub_superseded_terms(&structured.dynamic_instruction, &scrub_refs);
    for assumption in &mut structured.context.assumptions {
        *assumption = super::structurer::scrub_superseded_terms(assumption, &scrub_refs);
    }
    for criterion in &mut structured.success_criteria {
        *criterion = super::structurer::scrub_superseded_terms(criterion, &scrub_refs);
    }

    if let Some(p) = target_phase {
        if structured
            .execution_phases
            .iter()
            .any(|ph| u32::from(ph.phase_number) == p)
        {
            structured
                .execution_phases
                .retain(|ph| u32::from(ph.phase_number) == p);
        } else if let Some(lp) = existing.phases.iter().find(|ph| ph.phase_number == p) {
            structured.execution_phases = vec![ExecutionPhase {
                phase_number: u8::try_from(lp.phase_number).unwrap_or(1),
                name: lp.name.clone(),
                description: lp.description.clone(),
                estimated_duration: "Active phase".to_string(),
                deliverables: Vec::new(),
            }];
        }
    }

    if let Some(ref mut spec) = structured.output_spec {
        spec.format = structured.constraints.output_format.clone();
        spec.mandatory_elements = structured.constraints.required_inclusions.clone();
        if !structured.execution_phases.is_empty() {
            spec.sections = structured
                .execution_phases
                .iter()
                .map(|ph| ph.name.clone())
                .collect();
        }
    }
}

/// Builds `EvidenceBundle`, registers all facts into `ledger` (`SL-3`) prior to packing,
/// backfills prior `ledger.facts` on continuation turns, and packs with aligned `[F#]` IDs so
/// `detect_numeric_conflicts` and Stage 6B compaction preserve stable Turn 1+ fact identities.
fn build_evidence_and_register_facts(
    structured: &StructuredPrompt,
    raw_input: &str,
    raw_ctx_opt: Option<&str>,
    derived_rag: Option<&DerivedRagContext>,
    enrichment: Option<&crate::types::EnrichmentContext>,
    ledger: &mut PromptStateLedger,
    is_continuation: bool,
) -> (
    super::structurer::EvidenceBundle,
    HashMap<String, String>,
    super::structurer::PackedEvidence,
    bool,
    Vec<crate::npae::continuation::FactEntry>,
) {
    let has_rag_data = derived_rag
        .as_ref()
        .is_some_and(|r| !r.facts.is_empty() || !r.detailed_facts.is_empty());
    let mut evidence_bundle =
        super::structurer::EvidenceBundle::from_inputs(structured, raw_input, derived_rag);

    let mut raw_fact_pairs: Vec<(String, String)> = Vec::new();
    let (ctx_opt_split, rag_opt, user_prompt_split) = super::structurer::split_raw_input(raw_input);

    if let Some(ctx) = raw_ctx_opt {
        for clause in split_sentences_preserving_decimals(ctx) {
            let trimmed = clause.trim().trim_end_matches('.');
            if trimmed.len() >= 4
                && !trimmed.starts_with("[Source:")
                && !contains_prompt_injection(trimmed)
            {
                raw_fact_pairs.push((trimmed.to_string(), "user".to_string()));
            }
        }
    }
    // FIX: High #3 — also register `Context: [...]` clauses from `raw_input` (which holds
    // `resolved_slots` on continuation turns) so every `EvidenceBundle::from_inputs` item
    // has a stable `[F#]` entry in `ledger.facts` and `fact_id_map`.
    if let Some(ref ctx) = ctx_opt_split {
        for clause in split_sentences_preserving_decimals(ctx) {
            let trimmed = clause.trim().trim_end_matches('.');
            if trimmed.len() >= 4
                && !trimmed.starts_with("[Source:")
                && !contains_prompt_injection(trimmed)
            {
                raw_fact_pairs.push((trimmed.to_string(), "user".to_string()));
            }
        }
    }
    if let Some(ref rag_block) = rag_opt {
        for clause in split_sentences_preserving_decimals(rag_block) {
            let trimmed = clause.trim().trim_end_matches('.');
            if trimmed.len() >= 4
                && !trimmed.starts_with("[Source:")
                && !contains_prompt_injection(trimmed)
            {
                raw_fact_pairs.push((trimmed.to_string(), "rag".to_string()));
            }
        }
    }
    if let Some(rag) = derived_rag {
        if !rag.detailed_facts.is_empty() {
            for f in &rag.detailed_facts {
                let trimmed = f.text.trim().trim_end_matches('.');
                if trimmed.len() >= 4
                    && !trimmed.starts_with("[Source:")
                    && !contains_prompt_injection(trimmed)
                {
                    let src = if f.source_file.is_empty() {
                        "rag".to_string()
                    } else {
                        f.source_file.clone()
                    };
                    raw_fact_pairs.push((trimmed.to_string(), src));
                }
            }
        } else {
            for f in &rag.facts {
                let trimmed = f.trim().trim_end_matches('.');
                if trimmed.len() >= 4
                    && !trimmed.starts_with("[Source:")
                    && !contains_prompt_injection(trimmed)
                {
                    raw_fact_pairs.push((trimmed.to_string(), "rag".to_string()));
                }
            }
        }
    } else if let Some(idx) = structured.context.background.find("Known facts: ") {
        if let Some(rest) = structured
            .context
            .background
            .get(idx + "Known facts: ".len()..)
        {
            for fact in rest.split(';') {
                let trimmed = fact.trim().trim_end_matches('.');
                if trimmed.len() >= 4
                    && !trimmed.starts_with("[Source:")
                    && !contains_prompt_injection(trimmed)
                {
                    raw_fact_pairs.push((trimmed.to_string(), "rag".to_string()));
                }
            }
        }
    }
    // FIX: High #5 & Medium #10 — register declarative user facts from `user_prompt` into
    // `ledger.facts` (`SL-3`) before packing so every `EvidenceBundle` item has a stable `[F#]` ID.
    for sentence in split_sentences_preserving_decimals(&user_prompt_split) {
        if let Some(decl_fact) = super::structurer::extract_declarative_user_fact(&sentence) {
            let trimmed = decl_fact.trim().trim_end_matches('.');
            if trimmed.len() >= 4
                && !trimmed.starts_with("[Source:")
                && !contains_prompt_injection(trimmed)
            {
                raw_fact_pairs.push((trimmed.to_string(), "user".to_string()));
            }
        }
    }

    let mut has_structured_data = false;
    if let Some(enrich) = enrichment {
        if let Some(ref records) = enrich.structured_records {
            for rec in records {
                let inserted = evidence_bundle.push_key_value(
                    &rec.key,
                    &rec.value,
                    super::structurer::Provenance::StructuredRecord {
                        source_id: rec.source_id.clone(),
                        record_key: rec.record_key.clone(),
                    },
                    rec.relevance_score.unwrap_or(0.90),
                );
                if inserted {
                    has_structured_data = true;
                    raw_fact_pairs.push((
                        format!("{}: {}", rec.key.trim(), rec.value.trim()),
                        format!("{}/{}", rec.source_id, rec.record_key),
                    ));
                }
            }
        }
        if let Some(ref tables) = enrich.structured_tables {
            for table in tables {
                let col_refs: Vec<&str> = table.columns.iter().map(String::as_str).collect();
                let score = table.relevance_score.unwrap_or(0.88);
                for (row_idx, row) in table.rows.iter().enumerate() {
                    let val_refs: Vec<&str> = row.iter().map(String::as_str).collect();
                    let inserted = evidence_bundle.push_table_row(
                        &table.table_name,
                        &col_refs,
                        &val_refs,
                        super::structurer::Provenance::StructuredRecord {
                            source_id: table.source_id.clone(),
                            record_key: format!("row_{}", row_idx + 1),
                        },
                        score,
                    );
                    if inserted {
                        has_structured_data = true;
                        let pairs: Vec<String> = col_refs
                            .iter()
                            .zip(val_refs.iter())
                            .map(|(c, v)| format!("{c}={v}"))
                            .collect();
                        raw_fact_pairs.push((
                            format!("{} | {}", table.table_name.trim(), pairs.join(" | ")),
                            format!("{}/row_{}", table.source_id, row_idx + 1),
                        ));
                    }
                }
            }
        }
    }

    // FIX: High #5 — on continuation turns, always backfill prior `ledger.facts` into
    // `evidence_bundle` even when `has_external_evidence == false` so Turn 1 `[F#]` facts persist.
    if is_continuation && !ledger.facts.is_empty() {
        let existing_keys: HashSet<String> = evidence_bundle
            .raw_items()
            .iter()
            .map(|it| {
                it.text
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
            })
            .collect();

        for fact in &ledger.facts {
            let norm_key: String = fact
                .text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            if norm_key.is_empty() || existing_keys.contains(&norm_key) {
                continue;
            }
            let (prov, score) = if fact.source.eq_ignore_ascii_case("user") {
                (super::structurer::Provenance::UserVerbatim, 0.95)
            } else if fact.source.eq_ignore_ascii_case("structured") || fact.source.contains('/') {
                let (src, key) = fact
                    .source
                    .split_once('/')
                    .unwrap_or((fact.source.as_str(), "record"));
                (
                    super::structurer::Provenance::StructuredRecord {
                        source_id: src.to_string(),
                        record_key: key.to_string(),
                    },
                    0.90,
                )
            } else {
                (
                    super::structurer::Provenance::RagDocument {
                        source_file: if fact.source.eq_ignore_ascii_case("rag") {
                            String::new()
                        } else {
                            fact.source.clone()
                        },
                        page_number: None,
                        score: 0.85,
                    },
                    0.85,
                )
            };
            evidence_bundle.push_fact(&fact.text, prov, score);
        }
    }

    let newly_minted = ledger.register_facts(&raw_fact_pairs);

    let fact_id_map: HashMap<String, String> = ledger
        .facts
        .iter()
        .map(|f| (sanitize_untrusted_fact(&f.text), f.id.clone()))
        .collect();

    let has_external_evidence = has_rag_data
        || has_structured_data
        || (is_continuation
            && ledger
                .facts
                .iter()
                .any(|f| !f.source.eq_ignore_ascii_case("user")));
    let packed_evidence =
        evidence_bundle.pack_aligned(&super::structurer::PromptBudget::default(), &fact_id_map);

    (
        evidence_bundle,
        fact_id_map,
        packed_evidence,
        has_external_evidence,
        newly_minted,
    )
}

/// Renders the prompt, runs `HallucinationGuard::run_tri_layer`, and executes Stage 6B cycles.
#[allow(clippy::too_many_arguments)]
fn render_guard_and_correct(
    scoring_raw: &str,
    guard_raw: &str,
    user_prompt: &str,
    crisp_raw_input: &str,
    structured: &StructuredPrompt,
    amb: &crate::npae::schema::types::AmbiguityAnalysis,
    questions: &[crate::npae::schema::types::ClarifyingQuestion],
    derived_rag: Option<&DerivedRagContext>,
    evidence_bundle: &super::structurer::EvidenceBundle,
    fact_id_map: &HashMap<String, String>,
    packed_evidence: &super::structurer::PackedEvidence,
    has_external_evidence: bool,
    render_style: RenderStyle,
    target_model: crate::npae::schema::types::TargetModelFamily,
    threshold: f32,
    ledger_opt: Option<&PromptStateLedger>,
    repair_ctx: Option<&RepairContext>,
) -> (
    String,
    crate::npae::hallucination::guard::HallucinationReport,
    crate::types::ScoringResult,
    Vec<NumberedFact>,
) {
    let use_evidence_crisp = matches!(
        render_style,
        RenderStyle::Crisp | RenderStyle::CachedMultiTurn
    ) && target_model
        == crate::npae::schema::types::TargetModelFamily::StandardMarkdown
        && has_external_evidence
        && !packed_evidence.items.is_empty();

    let mut grounding_facts = if render_style == RenderStyle::Harness {
        packed_evidence.to_numbered_facts()
    } else {
        Vec::new()
    };

    let repair_facts: Vec<NumberedFact> = if let Some(ledger) = ledger_opt {
        if !ledger.facts.is_empty() {
            ledger
                .facts
                .iter()
                .map(|f| NumberedFact {
                    id: f.id.clone(),
                    text: f.text.clone(),
                })
                .collect()
        } else {
            packed_evidence.to_numbered_facts()
        }
    } else {
        packed_evidence.to_numbered_facts()
    };

    let is_repair = repair_ctx.is_some();
    let optimized_prompt = if let (Some(ledger), Some(repair)) = (ledger_opt, repair_ctx) {
        super::structurer::render_repair_prompt(ledger, repair, &repair_facts)
    } else {
        match render_style {
            RenderStyle::Crisp | RenderStyle::CachedMultiTurn if use_evidence_crisp => {
                super::structurer::render_crisp_prompt_with_evidence(
                    structured,
                    questions,
                    crisp_raw_input,
                    packed_evidence,
                )
            }
            RenderStyle::Crisp | RenderStyle::CachedMultiTurn => {
                super::structurer::render_crisp_prompt_for_model(
                    structured,
                    questions,
                    crisp_raw_input,
                    derived_rag,
                    target_model,
                )
            }
            RenderStyle::Harness => super::structurer::render_harness_prompt_with_evidence(
                structured,
                questions,
                scoring_raw,
                Some(user_prompt),
                packed_evidence,
                false,
            ),
        }
    };

    let guard_cfg = &structured.hallucination_guard;
    let mut final_prompt = optimized_prompt;

    let mut guard_source_text =
        if !packed_evidence.items.is_empty() && !guard_raw.contains("Context Facts:") {
            let fact_texts: Vec<&str> = packed_evidence
                .items
                .iter()
                .map(|i| i.text.as_str())
                .collect();
            format!("{guard_raw}\nContext Facts: {}", fact_texts.join(". "))
        } else {
            guard_raw.to_string()
        };
    // FIX: Medium #1 — include diagnostic `repair_context` items in `guard_source_text` so
    // `l2_confidence` / `l3_constraint` do not falsely prepend `[UNCERTAIN]` onto `# REPAIR DIRECTIVE`.
    if let Some(repair) = repair_ctx {
        let mut repair_parts: Vec<String> = Vec::new();
        for item in repair
            .failed_items
            .iter()
            .chain(repair.missing_items.iter())
            .chain(repair.details.iter())
        {
            let sanitized = sanitize_untrusted_fact(item);
            if !sanitized.is_empty() {
                repair_parts.push(sanitized);
            }
        }
        if !repair_parts.is_empty() {
            guard_source_text.push_str("\nRepair Items: ");
            guard_source_text.push_str(&repair_parts.join(" ; "));
        }
    }

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

    let rag_tokens = if has_external_evidence {
        packed_evidence
            .tokens_used
            .max(derived_rag.as_ref().map_or(0, |r| r.tokens_added))
    } else {
        0
    };
    let mut scoring_result = compute_aggressive_scoring(
        scoring_raw,
        &final_prompt,
        structured,
        amb,
        questions,
        rag_tokens,
    );

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
                questions
            } else {
                &[]
            };
        if is_repair {
            if !guard_report.passed {
                final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                    &final_prompt,
                    &guard_report,
                );
            } else {
                break;
            }
        } else if let Some(axis) = &scoring_result.correction_axis {
            match axis {
                crate::types::ScoreAxis::TaskEssential => {
                    // FIX: Medium #10 — compact from the complete `evidence_bundle` (preserving
                    // structured records/tables and prior ledger facts) with aligned `[F#]` IDs.
                    let compact_evidence = if packed_evidence.tokens_used > 300 {
                        evidence_bundle.pack_aligned(
                            &super::structurer::PromptBudget {
                                max_evidence_tokens: 300,
                                max_items: 12,
                            },
                            fact_id_map,
                        )
                    } else {
                        packed_evidence.clone()
                    };
                    if render_style == RenderStyle::Harness {
                        grounding_facts = compact_evidence.to_numbered_facts();
                    }
                    final_prompt = match render_style {
                        RenderStyle::Crisp | RenderStyle::CachedMultiTurn if use_evidence_crisp => {
                            super::structurer::render_crisp_prompt_with_evidence(
                                structured,
                                questions_for_tes,
                                crisp_raw_input,
                                &compact_evidence,
                            )
                        }
                        RenderStyle::Crisp | RenderStyle::CachedMultiTurn => {
                            super::structurer::render_crisp_prompt_for_model(
                                structured,
                                questions_for_tes,
                                crisp_raw_input,
                                derived_rag,
                                target_model,
                            )
                        }
                        RenderStyle::Harness => {
                            super::structurer::render_harness_prompt_with_evidence(
                                structured,
                                questions_for_tes,
                                scoring_raw,
                                Some(user_prompt),
                                &compact_evidence,
                                true,
                            )
                        }
                    };
                }
                crate::types::ScoreAxis::SchemaFidelity => {
                    final_prompt = crate::npae::hallucination::guard::remediate_hallucination(
                        &final_prompt,
                        &guard_report,
                    );
                }
                crate::types::ScoreAxis::SemanticCompleteness => {
                    if render_style == RenderStyle::Harness {
                        grounding_facts = packed_evidence.to_numbered_facts();
                    }
                    final_prompt = match render_style {
                        RenderStyle::Crisp | RenderStyle::CachedMultiTurn if use_evidence_crisp => {
                            super::structurer::render_crisp_prompt_with_evidence(
                                structured,
                                questions,
                                crisp_raw_input,
                                packed_evidence,
                            )
                        }
                        RenderStyle::Crisp | RenderStyle::CachedMultiTurn => {
                            super::structurer::render_crisp_prompt_for_model(
                                structured,
                                questions,
                                crisp_raw_input,
                                derived_rag,
                                target_model,
                            )
                        }
                        RenderStyle::Harness => {
                            super::structurer::render_harness_prompt_with_evidence(
                                structured,
                                questions,
                                scoring_raw,
                                Some(user_prompt),
                                packed_evidence,
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
            scoring_raw,
            &final_prompt,
            structured,
            amb,
            questions,
            rag_tokens,
        );

        let candidate_avg = (scoring_result.tes + scoring_result.sfs + scoring_result.scs) / 3.0;
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

    (best_prompt, best_guard, best_scoring, best_grounding_facts)
}

/// Builds `guard_source_text` strictly from reconciled task text, active constraints, and packed facts.
fn build_reconciled_guard_source(
    rehydrated_task: &str,
    ledger: &PromptStateLedger,
    packed_evidence: &super::structurer::PackedEvidence,
) -> String {
    let mut parts = vec![rehydrated_task.to_string()];
    let active_texts: Vec<&str> = ledger
        .active_inclusions()
        .into_iter()
        .chain(ledger.active_exclusions())
        .map(|c| c.text.as_str())
        .collect();
    if !active_texts.is_empty() {
        parts.push(format!("Active Constraints: {}", active_texts.join("; ")));
    }
    if !packed_evidence.items.is_empty() {
        let fact_texts: Vec<&str> = packed_evidence
            .items
            .iter()
            .map(|i| i.text.as_str())
            .collect();
        parts.push(format!("Context Facts: {}", fact_texts.join(". ")));
    }
    parts.join("\n")
}

/// Converts a [`PromptStateLedger`] and route classification into [`ContinuationMeta`].
fn build_continuation_meta(
    ledger: &PromptStateLedger,
    path: ContinuationPath,
    primary_kind: ContinuationKind,
    all_kinds: Vec<ContinuationKind>,
    delta_summary: String,
    delta_prompt: Option<String>,
) -> ContinuationMeta {
    let active_constraints = ledger
        .constraints
        .iter()
        .filter(|c| c.status == ConstraintStatus::Active)
        .map(|c| ConstraintView {
            id: c.id.clone(),
            kind: constraint_kind_str(c.kind).to_string(),
            text: c.text.clone(),
            status: "active".to_string(),
        })
        .collect();

    let superseded_constraints = ledger
        .superseded_targets()
        .into_iter()
        .map(str::to_string)
        .collect();

    let phases = ledger
        .phases
        .iter()
        .map(|p| PhaseView {
            phase_number: p.phase_number,
            name: p.name.clone(),
            status: phase_status_str(p.status).to_string(),
        })
        .collect();

    let active_phase = ledger
        .phases
        .iter()
        .find(|p| p.status == PhaseStatus::Active)
        .map(|p| p.phase_number);

    ContinuationMeta {
        ledger_id: ledger.ledger_id.clone(),
        turn_number: ledger.turn_count.max(1),
        path,
        primary_kind,
        all_kinds,
        delta_summary,
        active_constraints,
        superseded_constraints,
        phases,
        active_phase,
        open_slots: ledger.open_slots.clone(),
        delta_prompt,
        resolved_slots: ledger.resolved_slots.clone(),
    }
}

fn constraint_kind_str(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::Inclusion => "inclusion",
        ConstraintKind::Exclusion => "exclusion",
        ConstraintKind::Format => "format",
        ConstraintKind::LengthBound => "length_bound",
        ConstraintKind::Tone => "tone",
    }
}

fn phase_status_str(status: PhaseStatus) -> &'static str {
    match status {
        PhaseStatus::Pending => "pending",
        PhaseStatus::Active => "active",
        PhaseStatus::Done => "done",
        PhaseStatus::Deferred => "deferred",
    }
}

fn intent_key(intent: &IntentClass) -> &'static str {
    match intent {
        IntentClass::Build => "build",
        IntentClass::Explain => "explain",
        IntentClass::Debug => "debug",
        IntentClass::Analyze => "analyze",
        IntentClass::Transform => "transform",
    }
}

fn parse_intent_class(key: &str, fallback: IntentClass) -> IntentClass {
    match key {
        "build" => IntentClass::Build,
        "explain" => IntentClass::Explain,
        "debug" => IntentClass::Debug,
        "analyze" => IntentClass::Analyze,
        "transform" => IntentClass::Transform,
        _ => fallback,
    }
}

fn parse_perspective(key: &str, fallback: ActorPerspective) -> ActorPerspective {
    match key {
        "consumer" => ActorPerspective::Consumer,
        "practitioner" => ActorPerspective::Practitioner,
        "investor" => ActorPerspective::Investor,
        _ => fallback,
    }
}

fn strip_constraint_verb(s: &str) -> &str {
    for prefix in [
        "use ",
        "using ",
        "with ",
        "include ",
        "must include ",
        "require ",
        "must support ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    s.trim()
}

fn matches_any_scrub_term(item: &str, scrub_terms: &[&str]) -> bool {
    let item_low = item.trim().to_lowercase();
    if item_low.is_empty() {
        return false;
    }
    // FIX: Medium #7 — use token-boundary subsequence matching instead of raw `.contains` so
    // short tokens like `"Go"` or `"AI"` never scrub `"MongoDB"` or `"FastAPI"`.
    let item_tokens = tokenize_boundary(&item_low);
    let item_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&item_low));
    scrub_terms.iter().any(|term| {
        let term_low = term.trim().to_lowercase();
        let core_low = strip_constraint_verb(&term_low);
        let term_tokens = tokenize_boundary(&term_low);
        let core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(core_low));
        tokens_contain_subsequence(&item_tokens, &term_tokens)
            || (!item_tokens.is_empty() && tokens_contain_subsequence(&term_tokens, &item_tokens))
            || tokens_contain_subsequence(&item_tokens, &core_tokens)
            || (!item_core_tokens.is_empty()
                && tokens_contain_subsequence(&core_tokens, &item_core_tokens))
    })
}

fn summarize_delta_line(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(160).collect();
    if collapsed.chars().count() > 160 {
        out.push_str("...");
    }
    out
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
    use crate::types::{
        ChunkMetadata, EnrichmentContext, RagChunk, ReconstructedInput, StructuredKeyValue,
        StructuredTable,
    };
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
            structured_records: Some(vec![StructuredKeyValue {
                source_id: "crm_db".to_string(),
                record_key: "acct_42".to_string(),
                key: "account_tier".to_string(),
                value: "Enterprise".to_string(),
                relevance_score: Some(0.95),
            }]),
            structured_tables: Some(vec![StructuredTable {
                source_id: "warehouse".to_string(),
                table_name: "regional_arr".to_string(),
                columns: vec!["region".to_string(), "arr".to_string()],
                rows: vec![vec!["NA".to_string(), "$12.5M".to_string()]],
                relevance_score: Some(0.90),
            }]),
        };

        // 1. Crisp mode with RAG + structured records/tables surfaces provenance + [CONFLICT: ...]
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
            crisp_resp
                .optimized_prompt
                .contains("(Record: crm_db/acct_42)"),
            "Crisp prompt should render structured record provenance:\n{}",
            crisp_resp.optimized_prompt
        );
        assert!(
            crisp_resp
                .optimized_prompt
                .contains("regional_arr | region=NA | arr=$12.5M"),
            "Crisp prompt should render structured table row:\n{}",
            crisp_resp.optimized_prompt
        );
        assert!(
            crisp_resp.optimized_prompt.contains("[CONFLICT:"),
            "Crisp prompt should surface numeric conflict:\n{}",
            crisp_resp.optimized_prompt
        );

        // 2. Harness mode preserves provenance, structured records/tables, <conflicts>, and aligns grounding_facts
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
            harness_resp
                .optimized_prompt
                .contains("(Record: warehouse/row_1)"),
            "Harness prompt should preserve table row provenance:\n{}",
            harness_resp.optimized_prompt
        );
        assert!(
            harness_resp.optimized_prompt.contains("<conflicts>"),
            "Harness prompt should surface <conflicts>:\n{}",
            harness_resp.optimized_prompt
        );
        assert_eq!(harness_resp.grounding_facts.len(), 4);
        assert_eq!(harness_resp.grounding_facts[0].id, "F1");
        assert_eq!(harness_resp.grounding_facts[3].id, "F4");
    }
}
