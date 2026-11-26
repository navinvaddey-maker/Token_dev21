//! Domain history retrieval, explicit feedback recording, dev config, and user stats.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — Tenant-and-user-scoped history & feedback.

use db::DbPool;
use uuid::Uuid;

use super::compress::ENGINE_VERSION;
use crate::{
    data::Repository,
    errors::AppError,
    models::{feedback_signal::ExplicitFeedback, token_history::TokenHistory},
    AppState,
};

/// Fetches a specific history record defaulting to `'default'` tenant.
///
/// # Errors
/// Returns [`AppError::NotFound`] or [`AppError::Database`].
///
/// # Panics
/// Never.
pub async fn get_history_record(
    pool: &DbPool,
    user_id: &str,
    id: &str,
) -> Result<TokenHistory, AppError> {
    get_history_record_scoped(pool, user_id, "default", id).await
}

/// Fetches a specific history record with explicit `tenant_id` scoping.
///
/// # Errors
/// Returns [`AppError::NotFound`] or [`AppError::Database`].
///
/// # Panics
/// Never.
pub async fn get_history_record_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
    id: &str,
) -> Result<TokenHistory, AppError> {
    Repository::get_history_scoped(pool, user_id, tenant_id, id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(id.into()))
}

/// Lists history records defaulting to `'default'` tenant.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn list_history(
    pool: &DbPool,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    list_history_scoped(pool, user_id, "default", limit, offset).await
}

/// Lists history records with explicit `tenant_id` scoping.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn list_history_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history_scoped(pool, user_id, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

/// Records explicit feedback defaulting to `'default'` tenant.
///
/// # Errors
/// Returns [`AppError`] on database failure or unauthorized request.
///
/// # Panics
/// Never.
pub async fn record_explicit_feedback(
    state: &AppState,
    user_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    record_explicit_feedback_scoped(state, user_id, "default", req).await
}

/// Records explicit feedback with explicit `tenant_id` scoping.
///
/// # Errors
/// Returns [`AppError::Unauthorized`] if `user_id` is not a valid UUID,
/// [`AppError::NotFound`] if the history record does not exist in `(tenant_id, user_id)`,
/// or [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn record_explicit_feedback_scoped(
    state: &AppState,
    user_id: &str,
    tenant_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    let pool = &state.pool;
    let feedback_val = if req.rating == "thumbs_up" {
        1.0f32
    } else {
        -1.0f32
    };
    let u_id = Uuid::parse_str(user_id).map_err(|_| AppError::Unauthorized)?;

    if req.history_id.starts_with("mock-") {
        tracing::info!(
            "Received feedback for mock history ID: {}. Skipping database lookup.",
            req.history_id
        );
        return Ok(());
    }

    let history = Repository::get_history_scoped(pool, user_id, tenant_id, &req.history_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "History record {} not found for user",
                req.history_id
            ))
        })?;

    let sig_id = Uuid::new_v4().to_string();
    let sig = crate::models::feedback_signal::FeedbackSignal {
        id: sig_id,
        user_id: user_id.to_string(),
        history_id: req.history_id.clone(),
        signal_type: req.rating.clone(),
        signal_layer: 5,
        value: f64::from(feedback_val),
        meta: serde_json::json!({}),
        detected_at: chrono::Utc::now(),
    };

    Repository::insert_feedback_scoped(pool, &sig, tenant_id)
        .await
        .map_err(AppError::Database)?;

    let _ = state
        .learning_pool
        .apply_feedback(tenant_id, u_id, &history.original_prompt, feedback_val)
        .await;

    let elapsed_ms = chrono::Utc::now()
        .signed_duration_since(history.created_at)
        .num_milliseconds();
    let engagement_ms = u64::try_from(elapsed_ms.max(0)).unwrap_or(u64::MAX);

    let ctx = crate::behavior::feedback_detector::DetectionContext {
        user_id: user_id.to_string(),
        history_id: req.history_id.clone(),
        prev_prompt: None,
        curr_prompt: history.original_prompt.clone(),
        response_time_ms: 0,
        engagement_ms,
    };

    let signals = crate::behavior::feedback_detector::detect(&ctx);
    for sig in signals {
        if sig.signal_type == "long_engagement" {
            let weight = sig.map_to_weight();
            if weight != 0.0 {
                let _ = state
                    .learning_pool
                    .apply_feedback(tenant_id, u_id, &history.original_prompt, weight)
                    .await;

                let sig_model = crate::models::feedback_signal::FeedbackSignal {
                    id: Uuid::new_v4().to_string(),
                    user_id: user_id.to_string(),
                    history_id: req.history_id.clone(),
                    signal_type: sig.signal_type,
                    signal_layer: sig.signal_layer,
                    value: sig.value,
                    meta: sig.meta,
                    detected_at: chrono::Utc::now(),
                };
                let _ = Repository::insert_feedback_scoped(pool, &sig_model, tenant_id).await;
            }
        }
    }

    Ok(())
}

/// Returns configuration for development environments.
///
/// # Errors
/// Returns [`AppError::Database`] on failure.
///
/// # Panics
/// Never.
pub async fn get_dev_config(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    let use_cases = Repository::list_use_cases(pool)
        .await
        .map_err(AppError::Database)?;
    Ok(serde_json::json!({
        "engine_version": ENGINE_VERSION,
        "use_cases":      use_cases,
        "modes":          ["gentle", "balanced", "aggressive"],
    }))
}

/// Returns compression statistics for a user defaulting to `'default'` tenant.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn get_stats(pool: &DbPool, user_id: &str) -> Result<serde_json::Value, AppError> {
    get_stats_scoped(pool, user_id, "default").await
}

/// Returns compression statistics for a user scoped by `tenant_id`.
///
/// # Errors
/// Returns [`AppError::Database`] on query failure.
///
/// # Panics
/// Never.
pub async fn get_stats_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    let stats = Repository::get_user_stats_scoped(pool, user_id, tenant_id)
        .await
        .map_err(AppError::Database)?;
    Ok(serde_json::json!(stats))
}
