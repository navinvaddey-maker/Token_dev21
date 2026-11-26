//! Tenant-scoped administrative user management, history audit, and monitoring HTTP handlers.
//!
//! // ARCH: ARCHITECTURE.md §2 Full Component Map — Thin Axum handlers protected by `admin_middleware`.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

use super::middleware::PaginationParams;
use crate::{
    domain::{self, AuthContext},
    errors::AppError,
    AppState,
};

/// Returns tenant-scoped platform summary statistics via `GET /api/admin/stats`.
pub(crate) async fn admin_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::admin_get_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

/// Lists paginated users within the admin's tenant via `GET /api/admin/users`.
pub(crate) async fn admin_users(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let users = domain::admin_list_users(&state.pool, &auth.tenant_id, limit, offset).await?;
    Ok((StatusCode::OK, Json(users)).into_response())
}

/// Request payload for `PUT /api/admin/users/:id`.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct AdminUpdateUserRequest {
    business_type: String,
}

/// Updates a tenant user's `business_type` via `PUT /api/admin/users/:id`.
pub(crate) async fn admin_update_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_update_user(&state.pool, &auth.tenant_id, &id, &req.business_type).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Deletes a tenant user (preserving the final admin) via `DELETE /api/admin/users/:id`.
pub(crate) async fn admin_delete_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_delete_user(&state.pool, &auth.tenant_id, &id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Lists paginated compression history within the admin's tenant via `GET /api/admin/history`.
pub(crate) async fn admin_history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records = domain::admin_list_history(&state.pool, &auth.tenant_id, limit, offset).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

/// Returns recent 1-hour system performance metrics via `GET /api/admin/system-performance`.
pub(crate) async fn admin_get_system_performance(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let metrics = domain::admin_get_system_performance(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}

/// Returns tenant-scoped learning engine cluster statistics via `GET /api/admin/learning-engine-stats`.
pub(crate) async fn admin_get_learning_engine_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let metrics = domain::admin_get_learning_engine_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}

/// Query parameters for `GET /api/admin/compression-analytics`.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct CompressionAnalyticsParams {
    time_window: Option<String>,
}

/// Returns tenant-scoped compression analytics over `time_window` via `GET /api/admin/compression-analytics`.
pub(crate) async fn admin_get_compression_analytics(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<CompressionAnalyticsParams>,
) -> Result<axum::response::Response, AppError> {
    let metrics =
        domain::admin_get_compression_analytics(&state.pool, &auth.tenant_id, params.time_window)
            .await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}

/// Returns tenant-scoped user engagement statistics via `GET /api/admin/user-engagement`.
pub(crate) async fn admin_get_user_engagement_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let metrics = domain::admin_get_user_engagement_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}

/// Returns tenant-scoped feedback signal breakdown via `GET /api/admin/feedback-analysis`.
pub(crate) async fn admin_get_feedback_analysis(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let metrics = domain::admin_get_feedback_analysis(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}

/// Returns 24-hour compression error and warning metrics via `GET /api/admin/error-metrics`.
pub(crate) async fn admin_get_error_metrics(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let metrics = domain::admin_get_error_metrics(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(metrics)).into_response())
}
