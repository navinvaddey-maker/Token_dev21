//! Compression, decompression, history, feedback, dev config, and user stats HTTP handlers.
//!
//! // ARCH: ARCHITECTURE.md §1.3, §2 Full Component Map — Thin Axum handlers for prompt compression.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

use super::middleware::{
    validate_prompt_byte_and_word_bounds, PaginationParams, REQUEST_TIMEOUT_SECS,
};
use crate::{
    domain::{self, AuthContext},
    errors::AppError,
    AppState,
};

/// Request payload accepted by `/api/compress` and `/api/demo/compress`.
#[derive(Debug, serde::Deserialize)]
pub struct CompressRequest {
    /// Raw user prompt text to compress or structure.
    pub raw_text: String,
    /// Optional explicit task override.
    pub task: Option<String>,
    /// Optional use-case hint.
    pub use_case: Option<String>,
    /// Optional compression mode (`"gentle"`, `"balanced"`, `"aggressive"`).
    pub mode: Option<String>,
    /// Optional maximum output token budget.
    pub max_tokens: Option<usize>,
    /// Whether to include verbose diagnostics.
    pub verbose: Option<bool>,
    /// Whether to enrich compression with server-side RAG chunks.
    pub rag_enabled: Option<bool>,
    /// Optional document ID allow-list for RAG retrieval.
    pub rag_document_ids: Option<Vec<String>>,
    /// Optional maximum number of RAG chunks to retrieve.
    pub rag_top_k: Option<usize>,
    /// Optional NPAE ambiguity score threshold override.
    pub npae_ambiguity_threshold: Option<f32>,
    /// Optional NPAE maximum clarifying questions override.
    pub npae_max_questions: Option<u8>,
    /// Optional NPAE confidence threshold override.
    pub npae_confidence_threshold: Option<f32>,
    /// Optional NPAE Aggressive render style (`crisp`, `harness`, or `cached_multi_turn`).
    #[serde(default)]
    pub npae_render_style: Option<crate::npae::schema::types::RenderStyle>,
    /// Optional parent ledger ID linking this turn to an earlier prompt state.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Optional constraint IDs (`"C1"`) or target terms to revoke on this turn.
    #[serde(default)]
    pub revoked_constraints: Option<Vec<String>>,
    /// Optional 1-based execution phase number to focus on this turn.
    #[serde(default)]
    pub active_phase: Option<u32>,
    /// Optional slot name -> value overrides resolving open ambiguity gaps.
    #[serde(default)]
    pub resolved_slots: Option<std::collections::BTreeMap<String, String>>,
    /// Optional verification-guided repair context for targeted repair synthesis.
    #[serde(default)]
    pub repair_context: Option<crate::npae::schema::types::RepairContext>,
}

/// Acquires a permit from `aggressive_sem` or `compress_sem` matching `domain::compress_new_scoped` routing.
pub(crate) fn acquire_compression_semaphore_permit(
    state: &AppState,
    req: &CompressRequest,
) -> Result<tokio::sync::OwnedSemaphorePermit, AppError> {
    // FIX: Medium #13 & High #2 — treat `CachedMultiTurn` and `repair_context` as Aggressive
    // for `aggressive_sem` acquisition to match `domain::compress_new_scoped`.
    let is_aggressive = req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"))
        || req.npae_render_style == Some(crate::npae::schema::types::RenderStyle::CachedMultiTurn)
        || req.repair_context.is_some();

    if is_aggressive {
        state
            .aggressive_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::TooManyRequests(
                    "Deep Blueprint capacity reached; please retry shortly".into(),
                )
            })
    } else {
        state.compress_sem.clone().try_acquire_owned().map_err(|_| {
            AppError::TooManyRequests("Compression capacity reached; please retry shortly".into())
        })
    }
}

/// Executes stateless sandbox compression for unauthenticated visitors via `POST /api/demo/compress`.
pub(crate) async fn demo_compress(
    State(state): State<AppState>,
    Json(mut req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    const MAX_DEMO_BYTES: usize = 8 * 1024;
    const MAX_DEMO_WORDS: usize = 500;

    if req.raw_text.trim().is_empty() {
        return Err(AppError::EmptyInput);
    }
    if req.raw_text.len() > MAX_DEMO_BYTES
        || req.raw_text.split_whitespace().count() > MAX_DEMO_WORDS
    {
        return Err(AppError::InputTooLarge);
    }

    // Never allow unauthenticated demo requests to query server-side RAG.
    req.rag_enabled = Some(false);
    req.rag_document_ids = None;

    let _permit = acquire_compression_semaphore_permit(&state, &req)?;

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        domain::compress_demo(&state, req),
    )
    .await
    .map_err(|_| AppError::Timeout("Demo compression request timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

/// Executes authenticated, tenant-scoped compression via `POST /api/compress`.
pub(crate) async fn compress(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_byte_and_word_bounds(&req.raw_text)?;

    let _permit = acquire_compression_semaphore_permit(&state, &req)?;

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        domain::compress_new_scoped(&state, &auth.user_id, &auth.tenant_id, req),
    )
    .await
    .map_err(|_| AppError::Timeout("Compression request timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

/// Retrieves the original uncompressed prompt for a history entry via `GET /api/tokens/:id/decompress`.
pub(crate) async fn decompress(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let record =
        domain::get_history_record_scoped(&state.pool, &auth.user_id, &auth.tenant_id, &id).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "original_prompt": record.original_prompt })),
    )
        .into_response())
}

/// Lists paginated compression history for the authenticated user via `GET /api/history`.
pub(crate) async fn history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records =
        domain::list_history_scoped(&state.pool, &auth.user_id, &auth.tenant_id, limit, offset)
            .await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

/// Records explicit user feedback on a compression run via `POST /api/feedback`.
pub(crate) async fn feedback(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<crate::models::feedback_signal::ExplicitFeedback>,
) -> Result<axum::response::Response, AppError> {
    domain::record_explicit_feedback_scoped(&state, &auth.user_id, &auth.tenant_id, req).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Returns active use-case and engine configuration via `GET /api/dev/config`.
pub(crate) async fn dev_config(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let config = domain::get_dev_config(&state.pool).await?;
    Ok((StatusCode::OK, Json(config)).into_response())
}

/// Returns aggregated compression telemetry for the authenticated user via `GET /api/stats`.
pub(crate) async fn stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::get_stats_scoped(&state.pool, &auth.user_id, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}
