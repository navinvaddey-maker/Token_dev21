//! Tenant-scoped administrative domain logic and monitoring orchestration.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — RBAC checks and tenant-scoped admin queries.

use db::DbPool;

use super::auth::ALLOWED_BUSINESS_TYPES;
use crate::{
    data::{DeleteUserOutcome, Repository},
    errors::AppError,
    models::{token_history::TokenHistory, user::User},
};

/// Checks whether the user has administrative privileges based on `user.role == "admin"`.
///
/// # Errors
/// Returns [`AppError::Unauthorized`] if user is not found or [`AppError::Database`] on query error.
///
/// # Panics
/// Never.
pub async fn is_admin(pool: &DbPool, user_id: &str) -> Result<bool, AppError> {
    let user = Repository::find_user_by_id(pool, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;
    // ARCH: §0, §7 — Strict RBAC check based on `role`, not `business_type`
    Ok(user.role.eq_ignore_ascii_case("admin"))
}

/// Lists users scoped to a specific `tenant_id` for administrative review.
///
/// # Errors
/// Returns [`AppError::Database`] on query error.
///
/// # Panics
/// Never.
pub async fn admin_list_users(
    pool: &DbPool,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<User>, AppError> {
    Repository::list_users_by_tenant(pool, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

/// Deletes a user account inside `tenant_id`, preventing deletion of the last admin.
///
/// # Errors
/// Returns [`AppError::NotFound`] if the user does not exist in `tenant_id`,
/// [`AppError::Validation`] if attempting to delete the last admin, or [`AppError::Database`].
///
/// # Panics
/// Never.
pub async fn admin_delete_user(pool: &DbPool, tenant_id: &str, id: &str) -> Result<(), AppError> {
    match Repository::delete_user_preserving_last_admin_scoped(pool, tenant_id, id)
        .await
        .map_err(AppError::Database)?
    {
        DeleteUserOutcome::Deleted => Ok(()),
        DeleteUserOutcome::NotFound => Err(AppError::NotFound("User not found in tenant".into())),
        DeleteUserOutcome::LastAdminProtected => {
            Err(AppError::Validation("Cannot delete the last admin".into()))
        }
    }
}

/// Updates a user's `business_type` scoped to `tenant_id`, rejecting attempts to elevate to admin.
///
/// # Errors
/// Returns [`AppError::Validation`] if `business_type` is `"admin"` or invalid,
/// [`AppError::NotFound`] if the user is not found in `tenant_id`, or [`AppError::Database`].
///
/// # Panics
/// Never.
pub async fn admin_update_user(
    pool: &DbPool,
    tenant_id: &str,
    id: &str,
    business_type: &str,
) -> Result<(), AppError> {
    let b_type_lower = business_type.trim().to_lowercase();
    if b_type_lower == "admin" || !ALLOWED_BUSINESS_TYPES.contains(&b_type_lower.as_str()) {
        return Err(AppError::Validation(
            "Cannot set business_type to admin or unapproved type. Roles must be managed explicitly."
                .into(),
        ));
    }
    let affected = Repository::update_user_business_type_scoped(pool, tenant_id, id, business_type)
        .await
        .map_err(AppError::Database)?;
    if affected == 0 {
        return Err(AppError::NotFound("User not found in tenant".into()));
    }
    Ok(())
}

/// Returns platform statistics scoped to a specific `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_tenant_stats(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Lists compression history records scoped to a specific `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_list_history(
    pool: &DbPool,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history_by_tenant(pool, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

/// Returns recent 1-hour system performance diagnostics scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_system_performance(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_system_performance_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Returns learning engine cluster distribution statistics scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_learning_engine_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_learning_engine_stats_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Returns compression analytics aggregated over `time_window` scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_compression_analytics(
    pool: &DbPool,
    tenant_id: &str,
    time_window: Option<String>,
) -> Result<serde_json::Value, AppError> {
    Repository::get_compression_analytics_scoped(pool, tenant_id, time_window)
        .await
        .map_err(AppError::Database)
}

/// Returns user engagement statistics scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_user_engagement_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_user_engagement_stats_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Returns explicit and implicit feedback signal analytics scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_feedback_analysis(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_feedback_analysis_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Returns 24-hour compression error and warning metrics scoped to `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn admin_get_error_metrics(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_error_metrics_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}
