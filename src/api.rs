use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::SqlitePool;
use serde::Deserialize;
use crate::{domain, errors::AppError, models::user::{LoginRequest, RegisterRequest}};

pub fn router(pool: SqlitePool) -> Router {
    let public = Router::new()
        .route("/api/register", post(register))
        .route("/api/login",    post(login));

    let protected = Router::new()
        .route("/api/compress",                post(compress))
        .route("/api/tokens/:id/decompress",   get(decompress))
        .route("/api/history",                 get(history))
        .route("/api/feedback",                post(feedback))
        .route("/api/dev/config",              get(dev_config))
        .route("/api/stats",                   get(stats))
        .layer(middleware::from_fn_with_state(pool.clone(), auth_middleware));

    Router::new()
        .merge(public)
        .merge(protected)
        .with_state(pool)
}

// ── Auth endpoints ─────────────────────────────────────────────────────────

async fn register(
    State(pool): State<SqlitePool>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user = domain::register_user(&pool, req).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn login(
    State(pool): State<SqlitePool>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let resp = domain::login_user(&pool, req).await?;
    Ok((StatusCode::OK, Json(resp)))
}

// ── Compression endpoint ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CompressRequest {
    pub raw_text:   String,
    pub task:       String,
    pub use_case:   Option<String>,
    pub mode:       Option<String>,
    pub max_tokens: Option<usize>,
}

async fn compress(
    State(pool): State<SqlitePool>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<CompressRequest>,
) -> Result<impl IntoResponse, AppError> {
    let resp = domain::compress(&pool, &user_id, req).await?;
    Ok((StatusCode::OK, Json(resp)))
}

async fn decompress(
    State(pool): State<SqlitePool>,
    axum::Extension(user_id): axum::Extension<String>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let record = domain::get_history_record(&pool, &user_id, &id).await?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "original_prompt": record.original_prompt }))))
}

async fn history(
    State(pool): State<SqlitePool>,
    axum::Extension(user_id): axum::Extension<String>,
) -> Result<impl IntoResponse, AppError> {
    let records = domain::list_history(&pool, &user_id).await?;
    Ok((StatusCode::OK, Json(records)))
}

async fn feedback(
    State(pool): State<SqlitePool>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<crate::models::feedback_signal::ExplicitFeedback>,
) -> Result<impl IntoResponse, AppError> {
    domain::record_explicit_feedback(&pool, &user_id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn dev_config(
    State(pool): State<SqlitePool>,
) -> Result<impl IntoResponse, AppError> {
    let config = domain::get_dev_config(&pool).await?;
    Ok((StatusCode::OK, Json(config)))
}

async fn stats(
    State(pool): State<SqlitePool>,
    axum::Extension(user_id): axum::Extension<String>,
) -> Result<impl IntoResponse, AppError> {
    let s = domain::get_stats(&pool, &user_id).await?;
    Ok((StatusCode::OK, Json(s)))
}

// ── JWT middleware ─────────────────────────────────────────────────────────

async fn auth_middleware(
    State(_pool): State<SqlitePool>,
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
