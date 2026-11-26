//! Feedback signal persistence and active use-case catalog lookups.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — `feedback_signals` and `use_cases` queries.

use db::DbPool;

use super::Repository;
use crate::models::{feedback_signal::FeedbackSignal, use_case::UseCase};

impl Repository {
    /// Inserts a [`FeedbackSignal`] defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn insert_feedback(
        pool: &DbPool,
        signal: &FeedbackSignal,
    ) -> Result<(), sqlx::Error> {
        Self::insert_feedback_scoped(pool, signal, "default").await
    }

    /// Inserts a [`FeedbackSignal`] with explicit `tenant_id` scoping.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn insert_feedback_scoped(
        pool: &DbPool,
        signal: &FeedbackSignal,
        tenant_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO feedback_signals (id, user_id, tenant_id, history_id, signal_type, signal_layer, value, meta, detected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&signal.id)
        .bind(&signal.user_id)
        .bind(tenant_id)
        .bind(&signal.history_id)
        .bind(&signal.signal_type)
        .bind(signal.signal_layer)
        .bind(signal.value)
        .bind(&signal.meta)
        .bind(signal.detected_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Lists active use-case definitions (`active = 1`).
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_use_cases(pool: &DbPool) -> Result<Vec<UseCase>, sqlx::Error> {
        sqlx::query_as::<_, UseCase>(
            "SELECT id, key, version, role_frame, output_format, chunk_strategy, description, active, created_at, updated_at
             FROM use_cases WHERE active = 1",
        )
        .fetch_all(pool)
        .await
    }
}
