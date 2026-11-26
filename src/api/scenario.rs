//! Scenario registry and Ask RAG (`Cloud` vs `Own Data`) HTTP handlers.
//!
//! // ARCH: ARCHITECTURE.md §2.1 Ask RAG Data Source Boundary — Validates `ScenarioAskRequest`
//! // and persists scenario runs via `Repository::insert_token_history_record_scoped`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};

use super::middleware::{
    validate_prompt_byte_and_word_bounds, MAX_LOCAL_CHUNKS, MAX_LOCAL_CHUNK_BYTES,
    REQUEST_TIMEOUT_SECS,
};
use crate::{data::Repository, domain::AuthContext, errors::AppError, AppState};

/// Request payload accepted by `POST /api/scenario/ask`.
#[derive(Debug, serde::Deserialize)]
pub struct ScenarioAskRequest {
    /// Execution mode (`"scenario"` or `"regular"`).
    pub app_mode: String,
    /// Optional domain identifier when `app_mode == "scenario"`.
    pub domain: Option<String>,
    /// Optional style identifier when `app_mode == "scenario"`.
    pub style: Option<String>,
    /// Optional geography identifier (alphanumeric, `_`, or `-`, <= 32 chars).
    pub geography: Option<String>,
    /// User question or prompt text.
    pub question: String,
    /// Optional compression mode (`gentle`, `balanced`, `aggressive`, `concise`, `detailed`).
    pub mode: Option<String>,
    /// Exclusive RAG data source (`"cloud"` or `"own"`).
    #[serde(default)]
    pub data_source: Option<String>,
    /// Local chunks supplied by the client companion when `data_source == "own"`.
    #[serde(default)]
    pub local_chunks: Option<Vec<crate::rag::local::LocalRetrievedChunk>>,
}

/// Returns the available scenario domains and styles via `GET /api/scenario/registry`.
pub(crate) async fn scenario_get_registry(
    State(state): State<AppState>,
) -> axum::response::Response {
    let domains = &state.scenario_router.domain_registry.domains;
    let styles = &state.scenario_router.style_registry.styles;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "domains": domains,
            "styles": styles,
        })),
    )
        .into_response()
}

/// Validates and maps `data_source` + `local_chunks` into an exclusive [`crate::scenario::ScenarioDataSource`].
fn resolve_scenario_data_source(
    data_source: Option<&str>,
    local_chunks: Option<Vec<crate::rag::local::LocalRetrievedChunk>>,
) -> Result<crate::scenario::ScenarioDataSource, AppError> {
    let source_mode = data_source.unwrap_or("cloud").trim().to_ascii_lowercase();

    match source_mode.as_str() {
        "cloud" => {
            if local_chunks.as_ref().is_some_and(|c| !c.is_empty()) {
                return Err(AppError::Validation(
                    "local_chunks cannot be provided when data_source is 'cloud'".to_string(),
                ));
            }
            Ok(crate::scenario::ScenarioDataSource::Cloud)
        }
        "own" => {
            let raw_chunks = local_chunks.unwrap_or_default();
            if raw_chunks.len() > MAX_LOCAL_CHUNKS {
                return Err(AppError::Validation(format!(
                    "local_chunks exceeds maximum of {MAX_LOCAL_CHUNKS} chunks"
                )));
            }
            let mut mapped = Vec::with_capacity(raw_chunks.len());
            for c in raw_chunks {
                if c.content.len() > MAX_LOCAL_CHUNK_BYTES {
                    return Err(AppError::Validation(format!(
                        "local_chunk content exceeds maximum of {MAX_LOCAL_CHUNK_BYTES} bytes"
                    )));
                }
                let trimmed = c.content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                mapped.push(crate::types::RagChunk {
                    domain_tag: "local_own_data".to_string(),
                    content: trimmed.to_string(),
                    metadata: Some(crate::types::ChunkMetadata {
                        page_number: Some(c.page_number),
                        source_file: c.source_file.trim().to_string(),
                    }),
                });
            }
            Ok(crate::scenario::ScenarioDataSource::Own {
                local_chunks: mapped,
            })
        }
        other => Err(AppError::Validation(format!(
            "invalid data_source '{other}'; expected 'cloud' or 'own'"
        ))),
    }
}

const ALLOWED_SCENARIO_MODES: &[&str] =
    &["gentle", "balanced", "aggressive", "concise", "detailed"];

/// Validates question bounds, `app_mode`, `mode`, and `geography` for `POST /api/scenario/ask`.
fn validate_scenario_ask_request(req: &ScenarioAskRequest) -> Result<(), AppError> {
    validate_prompt_byte_and_word_bounds(&req.question)?;

    let app_mode_lower = req.app_mode.trim().to_ascii_lowercase();
    if app_mode_lower != "scenario" && app_mode_lower != "regular" {
        return Err(AppError::Validation(format!(
            "Invalid app_mode '{}'; expected 'scenario' or 'regular'",
            req.app_mode
        )));
    }

    if let Some(ref m) = req.mode {
        let m_lower = m.trim().to_ascii_lowercase();
        if !ALLOWED_SCENARIO_MODES.contains(&m_lower.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid mode '{m}'; expected one of: gentle, balanced, aggressive, concise, detailed"
            )));
        }
    }

    if let Some(ref g) = req.geography {
        let g_trimmed = g.trim();
        if g_trimmed.is_empty()
            || g_trimmed.len() > 32
            || !g_trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(AppError::Validation(
                "Invalid geography identifier: must be non-empty, <= 32 chars, and contain only alphanumeric, '_', or '-'".into(),
            ));
        }
    }

    Ok(())
}

/// Executes a Cloud or Own Data scenario query and logs its token history via `POST /api/scenario/ask`.
pub(crate) async fn scenario_ask(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(mut req): Json<ScenarioAskRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_scenario_ask_request(&req)?;

    let local_chunks = req.local_chunks.take();
    let scenario_source = resolve_scenario_data_source(req.data_source.as_deref(), local_chunks)?;

    let _permit = if req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"))
    {
        Some(
            state
                .aggressive_sem
                .clone()
                .try_acquire_owned()
                .map_err(|_| {
                    AppError::TooManyRequests(
                        "Deep Blueprint capacity reached; please retry shortly".into(),
                    )
                })?,
        )
    } else {
        None
    };

    let user = Repository::find_user_by_id(&state.pool, &auth.user_id)
        .await
        .map_err(AppError::Database)?;
    let business_type = user.map_or_else(|| "general".to_string(), |u| u.business_type);

    let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        state.scenario_router.route_and_execute_with_tenant(
            &auth.tenant_id,
            &req.app_mode,
            req.domain.as_deref(),
            req.style.as_deref(),
            req.geography.as_deref().map(str::trim),
            &req.question,
            &auth.user_id,
            &business_type,
            req.mode.as_deref(),
            &retriever,
            Some(&state.pipeline),
            scenario_source,
        ),
    )
    .await
    .map_err(|_| AppError::Timeout("Scenario query timed out".into()))?
    .map_err(|err_msg| {
        if err_msg.starts_with("Validation Error:") {
            AppError::Validation(err_msg)
        } else {
            AppError::Engine(err_msg)
        }
    })?;

    let history_id = uuid::Uuid::new_v4().to_string();
    let original_tokens = i64::from(crate::utils::tokens::estimate_tokens(&req.question));
    let final_tokens = i64::from(crate::utils::tokens::estimate_tokens(&resp.output_text));
    let saved_tokens = (original_tokens - final_tokens).max(0);
    let use_case = req
        .domain
        .as_deref()
        .unwrap_or_else(|| req.style.as_deref().unwrap_or("scenario"));
    let mode_str = req.mode.as_deref().unwrap_or(&req.app_mode);
    let warnings_json =
        serde_json::to_value(&resp.warnings).unwrap_or_else(|_| serde_json::json!([]));

    Repository::insert_token_history_record_scoped(
        &state.pool,
        &history_id,
        &auth.user_id,
        &auth.tenant_id,
        &req.question,
        &resp.output_text,
        saved_tokens,
        original_tokens,
        final_tokens,
        use_case,
        mode_str,
        "3.1.0",
        serde_json::json!([]),
        warnings_json,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to record scenario interaction in token_history: {e}");
        AppError::Database(e)
    })?;

    Ok((StatusCode::OK, Json(resp)).into_response())
}
