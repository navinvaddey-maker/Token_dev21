use crate::{
    domain,
    errors::AppError,
    models::user::{LoginRequest, RegisterRequest},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};

pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login));

    let protected = Router::new()
        .route("/api/compress", post(compress))
        .route("/api/tokens/:id/decompress", get(decompress))
        .route("/api/history", get(history))
        .route("/api/feedback", post(feedback))
        .route("/api/dev/config", get(dev_config))
        .route("/api/stats", get(stats))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let admin = Router::new()
        .route("/api/admin/stats", get(admin_stats))
        .route("/api/admin/users", get(admin_users))
        .route(
            "/api/admin/users/:id",
            put(admin_update_user).delete(admin_delete_user),
        )
        .route("/api/admin/history", get(admin_history))
        .route(
            "/api/admin/system-performance",
            get(admin_get_system_performance),
        )
        .route(
            "/api/admin/learning-engine-stats",
            get(admin_get_learning_engine_stats),
        )
        .route(
            "/api/admin/compression-analytics",
            get(admin_get_compression_analytics),
        )
        .route(
            "/api/admin/user-engagement",
            get(admin_get_user_engagement_stats),
        )
        .route(
            "/api/admin/feedback-analysis",
            get(admin_get_feedback_analysis),
        )
        .route("/api/admin/error-metrics", get(admin_get_error_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let npae_routes = Router::new()
        .route("/api/v1/compress", post(npae_compress))
        .route("/api/v1/aggressive", post(npae_aggressive))
        .route("/api/v1/hallucination-check", post(npae_hallucination_check))
        .route("/api/v1/schema", get(npae_schema))
        .route("/api/v1/health", get(npae_health));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin)
        .merge(npae_routes)
        .with_state(state)
}

// ── Auth endpoints ─────────────────────────────────────────────────────────

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<axum::response::Response, AppError> {
    let user = domain::register_user(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(user)).into_response())
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<axum::response::Response, AppError> {
    let resp = domain::login_user(&state.pool, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

// ── Compression endpoint ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CompressRequest {
    pub raw_text: String,
    pub task: Option<String>,

    pub model: Option<String>,
    pub use_case: Option<String>,
    pub mode: Option<String>,
    pub max_tokens: Option<usize>,
    // When enabled, the response will include extra SQLx/engine diagnostics.
    pub verbose: Option<bool>,
}

async fn compress(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    // Process universally through domain::compress_new, which uses PipelineOrchestrator
    // The mode differentiation is now strictly handled by PipelineOrchestrator::process
    let resp = domain::compress_new(&state, &user_id, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn decompress(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let record = domain::get_history_record(&state.pool, &user_id, &id).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "original_prompt": record.original_prompt })),
    )
        .into_response())
}

async fn history(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
) -> Result<axum::response::Response, AppError> {
    let records = domain::list_history(&state.pool, &user_id).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

async fn feedback(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<crate::models::feedback_signal::ExplicitFeedback>,
) -> Result<axum::response::Response, AppError> {
    domain::record_explicit_feedback(&state.pool, &user_id, req).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn dev_config(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let config = domain::get_dev_config(&state.pool).await?;
    Ok((StatusCode::OK, Json(config)).into_response())
}

async fn stats(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::get_stats(&state.pool, &user_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

// ── Admin endpoints ────────────────────────────────────────────────────────

async fn admin_stats(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let s = domain::admin_get_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

async fn admin_users(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let users = domain::admin_list_users(&state.pool).await?;
    Ok((StatusCode::OK, Json(users)).into_response())
}

#[derive(serde::Deserialize)]
struct AdminUpdateUserRequest {
    business_type: String,
}

async fn admin_update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_update_user(&state.pool, &id, &req.business_type).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_delete_user(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_history(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let records = domain::admin_list_history(&state.pool).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

// ── Admin Monitoring Endpoints ────────────────────────────────────────────────

async fn admin_get_system_performance(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_system_performance(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_learning_engine_stats(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_learning_engine_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

#[derive(serde::Deserialize)]
struct CompressionAnalyticsParams {
    time_window: Option<String>,
}

async fn admin_get_compression_analytics(
    State(state): State<AppState>,
    Query(params): Query<CompressionAnalyticsParams>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_compression_analytics(&state.pool, params.time_window).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_user_engagement_stats(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_user_engagement_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_feedback_analysis(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_feedback_analysis(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_error_metrics(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_error_metrics(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

// ── JWT and Admin middleware ───────────────────────────────────────────────

async fn admin_middleware(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let is_admin = domain::is_admin(&state.pool, &user_id).await?;
    if !is_admin {
        return Err(AppError::Unauthorized);
    }
    Ok(next.run(req).await)
}

async fn auth_middleware(
    State(_state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let user_id = crate::domain::verify_jwt(token)?;
    req.extensions_mut().insert(user_id);
    Ok(next.run(req).await)
}

// ── NPAE Handlers ─────────────────────────────────────────────────────────
use crate::npae::schema::types::{AggressiveRequest, HallucinationCheckRequest};

async fn npae_compress(
    State(_state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    let result = crate::npae::compression::pipeline::run_parallel_pipeline(&req.prompt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(result)).into_response())
}

async fn npae_aggressive(
    State(state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    let empty_cfg = crate::npae::schema::types::NpaeConfig { ambiguity_threshold: None, max_questions: None, confidence_threshold: None, skip_stage: None };
    let cfg = req.config.as_ref().unwrap_or(&empty_cfg);
    let repr = crate::npae::compression::pipeline::run_parallel_pipeline(&req.prompt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
        
    let structurer = crate::npae::aggressive::structurer::HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: req.prompt.clone(),
    };
    
    let resp = crate::npae::aggressive::engine::AggressiveEngine::run(&req.prompt, &repr, cfg, state.npae_config.clone(), &structurer)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn npae_hallucination_check(
    State(_state): State<AppState>,
    Json(req): Json<HallucinationCheckRequest>,
) -> Result<axum::response::Response, AppError> {
    let default_cfg = crate::npae::schema::types::HallucinationGuardConfig {
        self_critique_enabled: true,
        confidence_threshold: 0.75,
        contradiction_check: true,
        claim_verification_rules: vec![],
        uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
    };
    let cfg = req.guard_config.as_ref().unwrap_or(&default_cfg);
    let report = crate::npae::hallucination::guard::run_tri_layer(&req.output, &req.original_prompt, cfg)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

async fn npae_schema() -> Result<axum::response::Response, AppError> {
    // Phase 7.4
    let schema_json = serde_json::json!({ "version": "1.0", "message": "Schema endpoint stub" });
    Ok((StatusCode::OK, Json(schema_json)).into_response())
}

async fn npae_health() -> Result<axum::response::Response, AppError> {
    // Phase 7.5
    let health_json = serde_json::json!({
        "status": "ok",
        "version": "npae-1.0.0",
        "uptime_ms": 0,
    });
    Ok((StatusCode::OK, Json(health_json)).into_response())
}
