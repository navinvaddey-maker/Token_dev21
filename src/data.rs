use sqlx::SqlitePool;
use crate::models::{
    feedback_signal::FeedbackSignal,
    token_history::TokenHistory,
    use_case::UseCase,
    user::User,
};
use crate::engine::pipeline::PipelineOutput;

pub struct Repository;

impl Repository {
    // ── Users ──────────────────────────────────────────────────────────────

    pub async fn create_user(
        pool: &SqlitePool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, username, password_hash, email, business_type)
             VALUES (?1, ?2, ?3, ?4, ?5)
             RETURNING id, username, password_hash, email, business_type, license, created_at, updated_at"
        )
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .bind(email)
        .bind(business_type)
        .fetch_one(pool)
        .await
    }

    pub async fn find_user_by_username(
        pool: &SqlitePool,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, license, created_at, updated_at
             FROM users WHERE username = ?1"
        )
        .bind(username)
        .fetch_optional(pool)
        .await
    }

    // ── Token History ──────────────────────────────────────────────────────

    pub async fn insert_history(
        pool: &SqlitePool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
    ) -> Result<(), sqlx::Error> {
        let principle_logs = serde_json::to_string(&output.principle_logs).unwrap_or_else(|_| "[]".into());
        let warnings = serde_json::to_string(&output.warnings).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO token_history (id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
        )
        .bind(id)
        .bind(user_id)
        .bind(original_prompt)
        .bind(&output.optimized_prompt)
        .bind(output.token_saved as i64)
        .bind(output.token_original as i64)
        .bind(output.token_final as i64)
        .bind(&output.use_case)
        .bind(&output.mode)
        .bind(&output.engine_version)
        .bind(&principle_logs)
        .bind(&warnings)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get_history(
        pool: &SqlitePool,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE id = ?1 AND user_id = ?2"
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_history(
        pool: &SqlitePool,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2"
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    // ── Feedback ───────────────────────────────────────────────────────────

    pub async fn insert_feedback(
        pool: &SqlitePool,
        signal: &FeedbackSignal,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO feedback_signals (id, user_id, history_id, signal_type, signal_layer, value, meta, detected_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )
        .bind(&signal.id)
        .bind(&signal.user_id)
        .bind(&signal.history_id)
        .bind(&signal.signal_type)
        .bind(signal.signal_layer)
        .bind(signal.value)
        .bind(&signal.meta)
        .bind(&signal.detected_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    // ── Use Cases ──────────────────────────────────────────────────────────

    pub async fn list_use_cases(pool: &SqlitePool) -> Result<Vec<UseCase>, sqlx::Error> {
        sqlx::query_as::<_, UseCase>(
            "SELECT id, key, version, role_frame, output_format, chunk_strategy, description, active, created_at, updated_at
             FROM use_cases WHERE active = 1"
        )
        .fetch_all(pool)
        .await
    }

    // ── Stats ──────────────────────────────────────────────────────────────

    pub async fn get_user_stats(
        pool: &SqlitePool,
        user_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM token_history WHERE user_id = ?1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

        let saved: (i64,) = sqlx::query_as("SELECT COALESCE(SUM(tokens_saved), 0) FROM token_history WHERE user_id = ?1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

        let avg_saved: (f64,) = sqlx::query_as("SELECT COALESCE(AVG(tokens_saved), 0.0) FROM token_history WHERE user_id = ?1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

        Ok(serde_json::json!({
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
            "avg_tokens_saved":   avg_saved.0,
        }))
    }
}
