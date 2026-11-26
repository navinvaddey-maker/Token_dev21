//! Authentication, session inspection, and service health HTTP handlers.
//!
//! // ARCH: ARCHITECTURE.md §2 Full Component Map — Thin Axum handlers (zero direct SQL).

use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    data::Repository,
    domain::{self, AuthContext},
    errors::AppError,
    models::user::{LoginRequest, RegisterRequest},
    AppState,
};

/// Registers a new user account via `POST /api/register`.
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<axum::response::Response, AppError> {
    if req.username.len() < 3 || req.username.len() > 50 {
        return Err(AppError::Validation(
            "Username must be 3-50 characters".into(),
        ));
    }
    if !req
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Username must contain only alphanumeric characters, underscores, or hyphens".into(),
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation(
            "Password must be at least 8 characters".into(),
        ));
    }
    if !req.email.contains('@') || !req.email.contains('.') {
        return Err(AppError::Validation("Invalid email format".into()));
    }

    let user = domain::register_user(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(user)).into_response())
}

/// Authenticates credentials and sets the `HttpOnly` `tce_session` cookie via `POST /api/login`.
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<axum::response::Response, AppError> {
    let resp = domain::login_user(&state.pool, req).await?;
    let cookie_val = format!(
        "tce_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400",
        resp.token
    );
    let mut http_resp = (StatusCode::OK, Json(resp)).into_response();
    if let Ok(hv) = axum::http::HeaderValue::from_str(&cookie_val) {
        http_resp
            .headers_mut()
            .insert(axum::http::header::SET_COOKIE, hv);
    }
    Ok(http_resp)
}

/// Clears the `tce_session` cookie via `POST /api/logout`.
pub(crate) async fn logout() -> axum::response::Response {
    let mut http_resp = StatusCode::NO_CONTENT.into_response();
    http_resp.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_static(
            "tce_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        ),
    );
    http_resp
}

/// Returns the authenticated user's profile summary via `GET /api/me`.
pub(crate) async fn me(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let user = Repository::find_user_by_id(&state.pool, &auth.user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": user.id,
            "username": user.username,
            "business_type": user.business_type,
            "role": user.role,
            "tenant_id": user.tenant_id,
        })),
    )
        .into_response())
}

/// Returns API liveness status via `GET /api/health`.
pub(crate) async fn health_check() -> axum::response::Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy",
            "engine_version": "2.0.0",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
        .into_response()
}
