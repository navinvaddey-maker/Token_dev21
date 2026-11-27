//! Prompt compression orchestration across Demo, Balanced/Gentle, and Aggressive modes.
//!
//! // ARCH: ARCHITECTURE.md §1.3 Dual Routing Split, §1.5 Balanced Mode, §6.7 Continuation Ledgers.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::continuation::{
    build_legacy_continuation_meta, lookup_scoped_continuation_ledger,
    reconcile_legacy_continuation_turn, store_scoped_continuation_ledger,
};
use crate::{
    classifier::ScenarioClassifier,
    data::Repository,
    engine::evaluation,
    errors::AppError,
    npae::aggressive::engine::{AggressiveEngine, ContinuationInput},
    npae::aggressive::structurer::HttpStructurer,
    npae::continuation::{ContinuationPath, PhaseStatus, PromptStateLedger},
    npae::schema::types::{
        ContinuationMeta, NpaeConfig, RenderStyle, RepairContext, StructuredPromptResponse,
    },
    session::SessionHistory,
    utils::{security::OryxSecuritySanitizer, sync::OryxSyncGuard, tokens::estimate_tokens},
    AppState,
};

/// Canonical engine version recorded in `token_history` and `/api/dev/config`.
pub(crate) const ENGINE_VERSION: &str = "2.0.0";

/// Maximum number of per-`(tenant_id, user_id)` session entries retained in memory.
const MAX_USER_SESSIONS: usize = 10_000;

/// Executes prompt compression defaulting to `'default'` tenant.
///
/// # Errors
/// Returns [`AppError`] on execution or persistence failure.
///
/// # Panics
/// Never.
pub async fn compress_new(
    state: &AppState,
    user_id_str: &str,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    compress_new_scoped(state, user_id_str, "default", req).await
}

/// Executes stateless prompt compression for unauthenticated sandbox visitors.
///
/// # Arguments
/// * `state` - Shared application state containing pipeline and classifier handles.
/// * `req` - Compression request payload (RAG flags are ignored in demo mode).
///
/// # Errors
/// Returns [`AppError::Engine`] or [`AppError::Internal`] if pipeline execution or serialization fails.
///
/// # Panics
/// Never.
pub async fn compress_demo(
    state: &AppState,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    // ARCH: §1.3 & §1.5 — Runs the canonical PipelineOrchestrator statelessly (zero DB writes,
    // zero tenant/RAG access) so guests experience real CRISP/NPAE compression.
    let scenario = ScenarioClassifier::new(&state.pool, &state.embedding_engine)
        .classify_query_into_scenario(&req.raw_text)
        .await;

    let mut session = SessionHistory::new(5);
    let npae_cfg = NpaeConfig {
        ambiguity_threshold: req.npae_ambiguity_threshold,
        max_questions: req.npae_max_questions,
        confidence_threshold: req.npae_confidence_threshold,
        skip_stage: None,
        render_style: req.npae_render_style,
        target_model_family: None,
    };

    let orchestrator_response = if req.repair_context.is_some() {
        // FIX: Low #2 — route demo requests carrying `repair_context` through
        // `AggressiveEngine::run_with_continuation` with an ephemeral ledger (100% stateless).
        let prompt_for_repr = req.raw_text.clone();
        let repr = tokio::task::spawn_blocking(move || {
            crate::npae::compression::pipeline::run_parallel_pipeline(&prompt_for_repr)
        })
        .await
        .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
        .map_err(AppError::Engine)?;

        let structurer = HttpStructurer {
            route: "/api/demo/compress".to_string(),
            remote_addr: "demo".to_string(),
            body: req.raw_text.clone(),
        };
        let reconstructed = state.pipeline.reconstruct_input(&req.raw_text);
        let mut ephemeral_ledger: Option<PromptStateLedger> = None;
        let cont_input = ContinuationInput {
            revoked_constraints: req.revoked_constraints.unwrap_or_default(),
            active_phase: req.active_phase,
            resolved_slots: req.resolved_slots.unwrap_or_default(),
            repair_context: req.repair_context,
            ..ContinuationInput::default()
        };
        let resp = AggressiveEngine::run_with_continuation(
            &req.raw_text,
            &repr,
            &npae_cfg,
            state.npae_config.clone(),
            state.ory_engine.clone(),
            &structurer,
            &reconstructed,
            None,
            &mut ephemeral_ledger,
            Some(cont_input),
        )
        .await
        .map_err(AppError::Engine)?;
        crate::types::OrchestratorResponse::Aggressive(resp)
    } else {
        state
            .pipeline
            .process(
                &req.raw_text,
                &mut session,
                scenario,
                req.mode.as_deref(),
                Some(&npae_cfg),
                None,
            )
            .await
            .map_err(|e: Box<dyn std::error::Error>| AppError::Engine(e.to_string()))?
    };

    let demo_id = format!("demo-{}", Uuid::new_v4());
    serialize_demo_orchestrator_response(&demo_id, orchestrator_response)
}

/// Serializes an [`OrchestratorResponse`] into the guest demo JSON payload with `id` and `demo: true`.
fn serialize_demo_orchestrator_response(
    demo_id: &str,
    orchestrator_response: crate::types::OrchestratorResponse,
) -> Result<serde_json::Value, AppError> {
    match orchestrator_response {
        crate::types::OrchestratorResponse::Legacy(resp) => {
            let mut json_resp = serde_json::to_value(&resp).map_err(|e| {
                AppError::Internal(format!("Failed to serialize demo response: {e}"))
            })?;
            if let Some(obj) = json_resp.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(demo_id));
                obj.insert("demo".to_string(), serde_json::json!(true));
            }
            Ok(json_resp)
        }
        crate::types::OrchestratorResponse::Aggressive(resp) => {
            let tes = resp.scoring_result.tes;
            let sfs = resp.scoring_result.sfs;
            let scs = resp.scoring_result.scs;
            let overall = (tes + sfs + scs) / 3.0;
            let mut json_resp = serde_json::to_value(&resp).map_err(|e| {
                AppError::Internal(format!("Failed to serialize aggressive demo response: {e}"))
            })?;
            if let Some(obj) = json_resp.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(demo_id));
                obj.insert("demo".to_string(), serde_json::json!(true));
                obj.insert("mode".to_string(), serde_json::json!("aggressive"));
                obj.insert("tes_score".to_string(), serde_json::json!(tes));
                obj.insert(
                    "dual_score".to_string(),
                    serde_json::json!({
                        "tes": tes,
                        "sfs": sfs,
                        "scs": scs,
                        "overall": overall,
                    }),
                );
                obj.entry("warnings".to_string())
                    .or_insert_with(|| serde_json::json!([]));
            }
            Ok(json_resp)
        }
    }
}

/// Executes Aggressive Mode with tenant-and-user-scoped stateful parent→continuation tracking.
///
/// // ARCH: ARCHITECTURE.md §6.7 — shared by `/api/compress` (Aggressive) and `/api/v1/aggressive`,
/// // enforcing `(tenant_id, user_id)` ledger isolation and bounded `MAX_PROMPT_LEDGERS` eviction.
///
/// # Errors
/// Returns [`AppError`] on worker failure or engine execution failure.
///
/// # Panics
/// Never.
#[allow(clippy::too_many_arguments)] // Shared entry point across `/api/compress` and `/api/v1/aggressive`.
pub async fn run_aggressive_scoped(
    state: &AppState,
    user_id_str: &str,
    tenant_id: &str,
    prompt: &str,
    route: &str,
    remote_addr: &str,
    cfg: &NpaeConfig,
    enrichment: &crate::types::EnrichmentContext,
    parent_id: Option<&str>,
    revoked_constraints: Vec<String>,
    active_phase: Option<u32>,
    resolved_slots: BTreeMap<String, String>,
    repair_context: Option<RepairContext>,
) -> Result<StructuredPromptResponse, AppError> {
    let found_parent = lookup_scoped_continuation_ledger(state, tenant_id, user_id_str, parent_id);
    let prev_parent_raw = found_parent.as_ref().map(|(_, raw)| raw.clone());
    let mut ledger_opt = found_parent.as_ref().map(|(l, _)| l.clone());

    let prompt_for_repr = prompt.to_string();
    let repr = tokio::task::spawn_blocking(move || {
        crate::npae::compression::pipeline::run_parallel_pipeline(&prompt_for_repr)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
    .map_err(AppError::Engine)?;

    let structurer = HttpStructurer {
        route: route.to_string(),
        remote_addr: remote_addr.to_string(),
        body: prompt.to_string(),
    };
    let reconstructed = state.pipeline.reconstruct_input(prompt);

    let cont_input = ContinuationInput {
        parent_ledger: ledger_opt.clone(),
        parent_raw_prompt: prev_parent_raw.clone(),
        explicit_parent_id: parent_id.map(str::to_string),
        revoked_constraints,
        active_phase,
        resolved_slots,
        repair_context,
    };

    let resp = AggressiveEngine::run_with_continuation(
        prompt,
        &repr,
        cfg,
        state.npae_config.clone(),
        state.ory_engine.clone(),
        &structurer,
        &reconstructed,
        Some(enrichment),
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .map_err(AppError::Engine)?;

    if let Some(updated_ledger) = ledger_opt {
        let parent_raw_to_store = match resp.continuation.as_ref().map(|c| c.path) {
            Some(ContinuationPath::DeltaFollowUp) => {
                prev_parent_raw.unwrap_or_else(|| prompt.to_string())
            }
            _ => prompt.to_string(),
        };
        store_scoped_continuation_ledger(
            state,
            tenant_id,
            user_id_str,
            updated_ledger,
            parent_raw_to_store,
        );
    }

    Ok(resp)
}

/// Detects and records implicit behavioral feedback signals from sequential prompt history.
async fn record_implicit_feedback_signals_scoped(
    state: &AppState,
    tenant_id: &str,
    user_id_str: &str,
    user_id: Uuid,
    history_id: &str,
    raw_text: &str,
) {
    let prev_history = Repository::list_history_scoped(&state.pool, user_id_str, tenant_id, 1, 1)
        .await
        .ok()
        .and_then(|list| list.into_iter().next());

    let response_time_ms = if let Some(ref prev) = prev_history {
        let elapsed_ms = chrono::Utc::now()
            .signed_duration_since(prev.created_at)
            .num_milliseconds();
        u64::try_from(elapsed_ms.max(0)).unwrap_or(u64::MAX)
    } else {
        0
    };

    let ctx = crate::behavior::feedback_detector::DetectionContext {
        user_id: user_id_str.to_string(),
        history_id: history_id.to_string(),
        prev_prompt: prev_history.as_ref().map(|h| h.original_prompt.clone()),
        curr_prompt: raw_text.to_string(),
        response_time_ms,
        engagement_ms: 0,
    };

    let signals = crate::behavior::feedback_detector::detect(&ctx);
    for sig in signals {
        let weight = sig.map_to_weight();
        if weight != 0.0 {
            let target_prompt = prev_history
                .as_ref()
                .map_or(raw_text, |prev| prev.original_prompt.as_str());

            if let Err(err) = state
                .learning_pool
                .apply_feedback(tenant_id, user_id, target_prompt, weight)
                .await
            {
                tracing::warn!(tenant_id = %tenant_id, user_id = %user_id, error = %err, "Failed to apply implicit learning feedback");
            }

            let sig_model = crate::models::feedback_signal::FeedbackSignal {
                id: Uuid::new_v4().to_string(),
                user_id: user_id_str.to_string(),
                history_id: prev_history
                    .as_ref()
                    .map_or_else(|| history_id.to_string(), |prev| prev.id.clone()),
                signal_type: sig.signal_type,
                signal_layer: sig.signal_layer,
                value: sig.value,
                meta: sig.meta,
                detected_at: chrono::Utc::now(),
            };
            if let Err(err) =
                Repository::insert_feedback_scoped(&state.pool, &sig_model, tenant_id).await
            {
                tracing::warn!(tenant_id = %tenant_id, user_id = %user_id_str, error = %err, "Failed to persist implicit feedback signal");
            }
        }
    }
}

/// Reconciles and persists a continuation ledger when an auto-mode (`req.mode` non-aggressive)
/// request routes into `OrchestratorResponse::Aggressive`.
fn reconcile_auto_aggressive_ledger(
    state: &AppState,
    tenant_id: &str,
    user_id_str: &str,
    req: &crate::api::CompressRequest,
    resp: &mut StructuredPromptResponse,
    legacy_state_to_save: Option<(PromptStateLedger, String, ContinuationMeta)>,
) {
    // FIX: Medium #6, Medium #4 & Low #5 — when auto-mode (`req.mode` non-"aggressive")
    // routes to `OrchestratorResponse::Aggressive` on Turn 1 OR Turn 2+, update and
    // persist `legacy_state_to_save` in `state.prompt_ledgers`, unescaping XML entities
    // before re-registering `[F#]` facts.
    if let Some((mut ledger, parent_raw, mut meta)) = legacy_state_to_save {
        if matches!(
            meta.path,
            ContinuationPath::Initial | ContinuationPath::TopicReset
        ) {
            let perspective = ledger
                .locked_axis
                .as_ref()
                .map_or_else(|| "practitioner".to_string(), |a| a.perspective.clone());
            let intent = ledger
                .locked_axis
                .as_ref()
                .map_or_else(|| "build".to_string(), |a| a.intent.clone());
            ledger.apply_initial_structured(
                &resp.structured_prompt,
                &perspective,
                &intent,
                resp.confidence_score,
                resp.ambiguity_analysis.gap_zones.clone(),
            );
            if !req.raw_text.trim().is_empty() {
                ledger.objective.clone_from(&req.raw_text);
            }
            if let Some(p) = req.active_phase {
                ledger.set_active_phase(p);
            }
        } else if ledger.phases.is_empty() && !resp.structured_prompt.execution_phases.is_empty() {
            ledger.phases = resp
                .structured_prompt
                .execution_phases
                .iter()
                .enumerate()
                .map(|(idx, phase)| crate::npae::continuation::PhaseEntry {
                    phase_number: u32::from(phase.phase_number),
                    name: phase.name.clone(),
                    description: phase.description.clone(),
                    status: if meta
                        .active_phase
                        .is_some_and(|ap| ap == u32::from(phase.phase_number))
                        || (meta.active_phase.is_none() && idx == 0)
                    {
                        PhaseStatus::Active
                    } else {
                        PhaseStatus::Pending
                    },
                })
                .collect();
        }

        if !resp.grounding_facts.is_empty() {
            let pairs: Vec<(String, &str)> = resp
                .grounding_facts
                .iter()
                .map(|f| (OryxSecuritySanitizer::unescape_xml_entities(&f.text), "rag"))
                .collect();
            let ref_pairs: Vec<(&str, &str)> =
                pairs.iter().map(|(t, s)| (t.as_str(), *s)).collect();
            ledger.register_facts(&ref_pairs);
        }

        meta = build_legacy_continuation_meta(
            &ledger,
            meta.path,
            meta.primary_kind,
            meta.all_kinds,
            meta.delta_summary,
            meta.delta_prompt,
        );
        store_scoped_continuation_ledger(state, tenant_id, user_id_str, ledger, parent_raw);
        resp.continuation = Some(meta);
    }
}

/// Executes prompt compression with explicit tenant and user scoping, persisting history,
/// behavioral learning updates, and multi-turn continuation ledgers.
///
/// # Arguments
/// * `state` - Shared application state containing DB pool, pipeline, and ledger store.
/// * `user_id_str` - Authenticated user UUID string.
/// * `tenant_id` - Authenticated tenant identifier.
/// * `req` - Compression request payload.
///
/// # Errors
/// Returns [`AppError`] on validation, engine execution, or database persistence failure.
///
/// # Panics
/// Never.
#[allow(clippy::too_many_lines)] // Coordinates learning pool, RAG retrieval, dual pipeline execution, persistence, and implicit feedback.
pub async fn compress_new_scoped(
    state: &AppState,
    user_id_str: &str,
    tenant_id: &str,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    let user_id =
        Uuid::parse_str(user_id_str).map_err(|_| AppError::Internal("Invalid user_id".into()))?;

    // 1. Process via isolated per-(tenant_id, user_id) Learning Engine
    let engine_result = state
        .learning_pool
        .process(tenant_id, user_id, &req.raw_text)
        .await
        .map_err(AppError::Database)?;

    if engine_result.blocked {
        return Err(AppError::Engine("Prompt blocked by constraints".into()));
    }

    // 2. Prepare enrichment context (Cluster Vocab + RAG)
    let cluster_vocab = engine_result
        .cluster_vocab
        .iter()
        .map(|(token, _): &(String, f32)| token.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let history_id = Uuid::new_v4().to_string();

    let mut enrichment = crate::types::EnrichmentContext {
        cluster_vocab: if cluster_vocab.is_empty() {
            None
        } else {
            Some(cluster_vocab)
        },
        rag_chunks: None,
        ..Default::default()
    };

    if req.rag_enabled.unwrap_or(false) {
        let query = crate::rag::types::RetrievalQuery {
            query_text: req.raw_text.clone(),
            user_id: user_id_str.to_string(),
            document_ids: req.rag_document_ids.clone(),
            top_k: req.rag_top_k.unwrap_or(5).clamp(1, 20),
            min_similarity: 0.25,
        };
        let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
        if let Ok(retrieved_chunks) = retriever
            .retrieve_scoped(tenant_id, &query, Some(&history_id))
            .await
        {
            if !retrieved_chunks.is_empty() {
                let chunks = retrieved_chunks
                    .into_iter()
                    .map(crate::types::RagChunk::from)
                    .collect();
                enrichment.rag_chunks = Some(chunks);
            }
        }
    }

    // Scenario Classification
    let scenario = ScenarioClassifier::new(&state.pool, &state.embedding_engine)
        .classify_query_into_scenario(&req.raw_text)
        .await;

    tracing::debug!(
        "ScenarioClassifier result: {:?}",
        scenario
            .as_ref()
            .map(|s| (&s.domain, &s.intent_class, s.confidence))
    );

    // 3. Run the 7-stage pipeline (0A → 6B) with tenant-scoped, bounded session cache
    let session_key = format!("{tenant_id}:{user_id_str}");
    let mut session = state
        .sessions
        .get(&session_key)
        .map_or_else(|| SessionHistory::new(10), |entry| entry.value().clone());

    let npae_cfg = NpaeConfig {
        ambiguity_threshold: req.npae_ambiguity_threshold,
        max_questions: req.npae_max_questions,
        confidence_threshold: req.npae_confidence_threshold,
        skip_stage: None,
        render_style: req.npae_render_style,
        target_model_family: None,
    };

    // FIX: High #2 — repair continuations must always route through `run_aggressive_scoped`
    // regardless of whether the UI selected `balanced`/`gentle` or `aggressive`.
    let is_explicit_aggressive = req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"))
        || req.npae_render_style == Some(RenderStyle::CachedMultiTurn)
        || req.repair_context.is_some();

    let (orchestrator_response, legacy_state_to_save) = if is_explicit_aggressive {
        let resp = run_aggressive_scoped(
            state,
            user_id_str,
            tenant_id,
            &req.raw_text,
            "/api/compress",
            "authenticated",
            &npae_cfg,
            &enrichment,
            req.parent_id.as_deref(),
            req.revoked_constraints.clone().unwrap_or_default(),
            req.active_phase,
            req.resolved_slots.clone().unwrap_or_default(),
            req.repair_context.clone(),
        )
        .await?;
        (crate::types::OrchestratorResponse::Aggressive(resp), None)
    } else {
        let (prompt_for_pipeline, legacy_ledger, parent_raw_to_store, legacy_cont_meta) =
            reconcile_legacy_continuation_turn(
                state,
                user_id_str,
                tenant_id,
                &req,
                scenario.as_ref(),
            );
        let orch = state
            .pipeline
            .process(
                &prompt_for_pipeline,
                &mut session,
                scenario,
                req.mode.as_deref(),
                Some(&npae_cfg),
                Some(enrichment),
            )
            .await
            .map_err(|e: Box<dyn std::error::Error>| AppError::Engine(e.to_string()))?;
        (
            orch,
            Some((legacy_ledger, parent_raw_to_store, legacy_cont_meta)),
        )
    };

    OryxSyncGuard::insert_into_bounded_dashmap(
        &state.sessions,
        MAX_USER_SESSIONS,
        session_key,
        session,
    );

    let (compression_response, legacy_cont_meta_opt) = match orchestrator_response {
        crate::types::OrchestratorResponse::Legacy(resp) => {
            let cont_meta = if let Some((ledger, parent_raw, meta)) = legacy_state_to_save {
                store_scoped_continuation_ledger(state, tenant_id, user_id_str, ledger, parent_raw);
                Some(meta)
            } else {
                None
            };
            (resp, cont_meta)
        }
        crate::types::OrchestratorResponse::Aggressive(mut resp) => {
            reconcile_auto_aggressive_ledger(
                state,
                tenant_id,
                user_id_str,
                &req,
                &mut resp,
                legacy_state_to_save,
            );

            let start = std::time::Instant::now();
            let rows_affected = Repository::insert_token_history_record_scoped(
                &state.pool,
                &history_id,
                user_id_str,
                tenant_id,
                &req.raw_text,
                &resp.optimized_prompt,
                i64::from(resp.token_saved),
                i64::from(resp.token_original),
                i64::from(resp.token_final),
                "auto",
                "aggressive",
                ENGINE_VERSION,
                serde_json::json!({ "mode": "aggressive", "request_id": resp.request_id }),
                serde_json::json!([]),
            )
            .await
            .map_err(AppError::Database)?;

            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            tracing::debug!(rows_affected, duration_ms, "Aggressive history logged");

            let tes = resp.scoring_result.tes;
            let sfs = resp.scoring_result.sfs;
            let scs = resp.scoring_result.scs;
            let overall = (tes + sfs + scs) / 3.0;
            let mut json_resp = serde_json::to_value(&resp).map_err(|e| {
                AppError::Internal(format!("Failed to serialize aggressive response: {e}"))
            })?;
            if let Some(obj) = json_resp.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(history_id));
                obj.insert("mode".to_string(), serde_json::json!("aggressive"));
                obj.insert("tes_score".to_string(), serde_json::json!(tes));
                obj.insert(
                    "dual_score".to_string(),
                    serde_json::json!({
                        "tes": tes,
                        "sfs": sfs,
                        "scs": scs,
                        "overall": overall,
                    }),
                );
                obj.entry("warnings".to_string())
                    .or_insert_with(|| serde_json::json!([]));
            }

            record_implicit_feedback_signals_scoped(
                state,
                tenant_id,
                user_id_str,
                user_id,
                &history_id,
                &req.raw_text,
            )
            .await;

            return Ok(json_resp);
        }
    };

    // 4. Compute token counts
    let token_original = estimate_tokens(&req.raw_text) as usize;
    let token_final = estimate_tokens(&compression_response.response) as usize;
    let token_saved = token_original.saturating_sub(token_final);

    // 5. Run evaluation metrics
    let eval = evaluation::evaluate(&req.raw_text, &compression_response.response);

    // 6. Persist to DB with tenant scoping
    let mut verbose = crate::models::verbose::SqlxVerbose::default();
    Repository::insert_history_from_compression_scoped(
        &state.pool,
        &history_id,
        user_id_str,
        tenant_id,
        &req.raw_text,
        &compression_response,
        token_original,
        token_final,
        token_saved,
        ENGINE_VERSION,
        &mut verbose,
    )
    .await
    .map_err(AppError::Database)?;

    // 7. Build response
    let mut json_resp = serde_json::json!({
        "id":               history_id,
        "optimized_prompt": compression_response.response,
        "token_original":   token_original,
        "token_final":      token_final,
        "token_saved":      token_saved,
        "warnings":         compression_response.field_issues.iter()
                                .map(|i| format!("{}: {}", i.field_name, i.description))
                                .collect::<Vec<String>>(),
        "evaluation":       eval,
        "engine_version":   ENGINE_VERSION,
        "cluster_id":       engine_result.cluster_id,
        "is_new_domain":    engine_result.is_new_domain,

        "mode":             compression_response.mode,
        "error_score":      compression_response.error_score,
        "fidelity":         compression_response.fidelity,
        "dual_score":       {
            "tes": compression_response.scoring_result.tes,
            "sfs": compression_response.scoring_result.sfs,
            "scs": compression_response.scoring_result.scs,
            "overall": (compression_response.scoring_result.tes + compression_response.scoring_result.sfs + compression_response.scoring_result.scs) / 3.0
        },
        "topology":         format!("{:?}", compression_response.topology),
        "normalization":    compression_response.normalization,
        "field_issues":     compression_response.field_issues,
        "scope_injections": compression_response.scope_injections,
        "correction_cycle": compression_response.correction_cycle,
        "task":             compression_response.schema.task,
        "context":          compression_response.schema.context,
        "null_fields":      compression_response.null_fields,

        "wm_slots_used":    compression_response.wm_slots_used,
        "clusters":         compression_response.clusters,
        "delta_tokens":     compression_response.delta_tokens,
    });
    if let Some(cont_meta) = legacy_cont_meta_opt {
        if let (Some(obj), Ok(meta_val)) =
            (json_resp.as_object_mut(), serde_json::to_value(&cont_meta))
        {
            obj.insert("continuation".to_string(), meta_val);
        }
    }

    // 8. Implicit Signal Detection
    record_implicit_feedback_signals_scoped(
        state,
        tenant_id,
        user_id_str,
        user_id,
        &history_id,
        &req.raw_text,
    )
    .await;

    Ok(json_resp)
}
