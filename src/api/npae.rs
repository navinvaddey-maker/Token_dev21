//! NPAE compression, Aggressive structuring, hallucination check, and grounding verification handlers.
//!
//! // ARCH: ARCHITECTURE.md §1.3, §2, §5 (GAP-42) — Thin Axum handlers for `/api/v1/*` and `/api/npae/verify`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};

use super::middleware::{validate_prompt_byte_and_word_bounds, REQUEST_TIMEOUT_SECS};
use crate::{
    domain::{self, AuthContext},
    errors::AppError,
    npae::schema::types::{AggressiveRequest, HallucinationCheckRequest},
    AppState,
};

/// Executes parallel NPAE representation compression via `POST /api/v1/compress`.
pub(crate) async fn npae_compress(
    State(state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_byte_and_word_bounds(&req.prompt)?;
    let _permit = state
        .compress_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("Compression capacity reached; please retry shortly".into())
        })?;
    let prompt_owned = req.prompt.clone();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            crate::npae::compression::pipeline::run_parallel_pipeline(&prompt_owned)
        }),
    )
    .await
    .map_err(|_| AppError::Timeout("NPAE compression timed out".into()))?
    .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
    .map_err(AppError::Engine)?;

    Ok((StatusCode::OK, Json(result)).into_response())
}

/// Executes Aggressive Mode compression and structuring via `POST /api/v1/aggressive`.
pub(crate) async fn npae_aggressive(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_byte_and_word_bounds(&req.prompt)?;

    let _permit = state
        .aggressive_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests(
                "Deep Blueprint capacity reached; please retry shortly".into(),
            )
        })?;

    let fut = async {
        let empty_cfg = crate::npae::schema::types::NpaeConfig::default();
        let cfg = req.config.as_ref().unwrap_or(&empty_cfg);

        let mut enrichment = crate::types::EnrichmentContext::default();
        if req.rag_enabled.unwrap_or(false) {
            let history_id = uuid::Uuid::new_v4().to_string();
            let query = crate::rag::types::RetrievalQuery {
                query_text: req.prompt.clone(),
                user_id: auth.user_id.clone(),
                document_ids: req.rag_document_ids.clone(),
                top_k: req.rag_top_k.unwrap_or(5).clamp(1, 20),
                min_similarity: 0.45,
            };
            let retriever =
                crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
            if let Ok(retrieved_chunks) = retriever
                .retrieve_scoped(&auth.tenant_id, &query, Some(&history_id))
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

        domain::run_aggressive_scoped(
            &state,
            &auth.user_id,
            &auth.tenant_id,
            &req.prompt,
            "/api/v1/aggressive",
            "authenticated",
            cfg,
            &enrichment,
            req.parent_id.as_deref(),
            req.revoked_constraints.unwrap_or_default(),
            req.active_phase,
            req.resolved_slots.unwrap_or_default(),
            req.repair_context,
        )
        .await
    };

    let resp = tokio::time::timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS), fut)
        .await
        .map_err(|_| AppError::Timeout("Aggressive compression timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

/// Runs tri-layer hallucination verification via `POST /api/v1/hallucination-check`.
pub(crate) async fn npae_hallucination_check(
    State(_state): State<AppState>,
    Json(req): Json<HallucinationCheckRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_byte_and_word_bounds(&req.original_prompt)?;
    validate_prompt_byte_and_word_bounds(&req.output)?;
    let default_cfg = crate::npae::schema::types::HallucinationGuardConfig {
        self_critique_enabled: true,
        confidence_threshold: 0.75,
        contradiction_check: true,
        claim_verification_rules: vec![],
        uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
    };
    let cfg = req.guard_config.as_ref().unwrap_or(&default_cfg);
    let report =
        crate::npae::hallucination::guard::run_tri_layer(&req.output, &req.original_prompt, cfg)
            .map_err(AppError::Engine)?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

/// Verifies a downstream LLM response against numbered `[F#]` grounding facts (`GAP-42`).
pub(crate) async fn npae_verify_grounding(
    State(state): State<AppState>,
    Json(req): Json<crate::npae::grounding::verifier::VerifyGroundingRequest>,
) -> Result<axum::response::Response, AppError> {
    // FIX: Empty `llm_answer` must map to 400 Bad Request (`AppError::Validation`) to match
    // `GroundingError::EmptyAnswer`, while oversized payloads still hit `validate_prompt_byte_and_word_bounds` (422).
    if req.llm_answer.trim().is_empty() {
        return Err(AppError::Validation("llm_answer must not be empty".into()));
    }
    validate_prompt_byte_and_word_bounds(&req.llm_answer)?;
    let _permit = state
        .compress_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("Verification capacity reached; please retry shortly".into())
        })?;
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            crate::npae::grounding::verifier::verify_grounding(&req)
        }),
    )
    .await
    .map_err(|_| AppError::Timeout("Grounding verification timed out".into()))?
    .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
    .map_err(|e| AppError::Validation(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

/// Returns the JSON Schema for grounded NPAE responses via `GET /api/v1/schema`.
pub(crate) async fn npae_schema() -> axum::response::Response {
    let schema_json = serde_json::json!({
        "version": "2.0.0",
        "schema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "NpaeGroundedResponse",
            "type": "object",
            "additionalProperties": false,
            "required": [
                "schema_version",
                "status",
                "items",
                "clarification_needed"
            ],
            "properties": {
                "schema_version": { "type": "string", "const": "2.0.0" },
                "status": {
                    "type": "string",
                    "enum": ["GROUNDED", "PARTIAL", "ABSTAIN", "CLARIFICATION_REQUIRED"]
                },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["requested_item", "status", "answer", "cited_facts"],
                        "properties": {
                            "requested_item": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["FOUND", "PARTIAL_MATCH", "NOT_FOUND"]
                            },
                            "answer": { "type": "string" },
                            "cited_facts": {
                                "type": "array",
                                "items": { "type": "string", "pattern": "^F[0-9]+$" }
                            }
                        }
                    }
                },
                "clarification_needed": { "type": "boolean" },
                "clarifying_questions": { "type": "array", "items": { "type": "string" } },
                "unverified_claims": { "type": "array", "items": { "type": "string" } }
            }
        }
    });
    (StatusCode::OK, Json(schema_json)).into_response()
}

/// Returns NPAE subsystem liveness via `GET /api/v1/health`.
pub(crate) async fn npae_health() -> axum::response::Response {
    let health_json = serde_json::json!({
        "status": "ok",
        "version": "npae-1.0.0",
        "uptime_ms": 0,
    });
    (StatusCode::OK, Json(health_json)).into_response()
}
