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

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin)
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
    pub task: String,
    pub deliverables: Option<String>,
    pub constraints: Option<String>,
    pub reproducibility: Option<String>,
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
