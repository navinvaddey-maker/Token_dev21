use crate::engine::pipeline::PipelineOutput;
use crate::models::{
    feedback_signal::FeedbackSignal,
    token_history::TokenHistory,
    use_case::UseCase,
    user::User,
    verbose::{SqlxEvent, SqlxVerbose},
};
use db::DbPool;
use serde_json::{json, Value};
use std::time::Instant;

pub struct Repository;

impl Repository {
    // ── Users ──────────────────────────────────────────────────────────────

    pub async fn create_user(
        pool: &DbPool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, username, password_hash, email, business_type)
             VALUES ($1, $2, $3, $4, $5)
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

    pub async fn find_user_by_id(pool: &DbPool, id: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, license, created_at, updated_at
             FROM users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_user_by_username(
        pool: &DbPool,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, license, created_at, updated_at
             FROM users WHERE username = $1"
        )
        .bind(username)
        .fetch_optional(pool)
        .await
    }

    // ── Token History ──────────────────────────────────────────────────────

    pub async fn insert_history(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
    ) -> Result<(), sqlx::Error> {
        let principle_logs =
            serde_json::to_string(&output.principle_logs).unwrap_or_else(|_| "[]".into());
        let warnings = serde_json::to_string(&output.warnings).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO token_history (id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
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

    pub async fn insert_history_verbose(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
        verbose: &mut SqlxVerbose,
    ) -> Result<(), sqlx::Error> {
        let sql = "INSERT INTO token_history (id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)";
        let start = Instant::now();
        let result = sqlx::query(sql)
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
            .bind(serde_json::json!(&output.principle_logs))
            .bind(serde_json::json!(&output.warnings))
            .execute(pool)
            .await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        verbose.push(SqlxEvent {
            operation: "insert_history".into(),
            duration_ms,
            row_count: Some(result.rows_affected() as i64),
            sql: Some("token_history.insert".into()),
        });

        Ok(())
    }

    pub async fn get_history(
        pool: &DbPool,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE id = $1 AND user_id = $2"
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_history(
        pool: &DbPool,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2"
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    // ── Feedback ───────────────────────────────────────────────────────────

    pub async fn insert_feedback(
        pool: &DbPool,
        signal: &FeedbackSignal,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO feedback_signals (id, user_id, history_id, signal_type, signal_layer, value, meta, detected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
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

    pub async fn list_use_cases(pool: &DbPool) -> Result<Vec<UseCase>, sqlx::Error> {
        sqlx::query_as::<_, UseCase>(
            "SELECT id, key, version, role_frame, output_format, chunk_strategy, description, active, created_at, updated_at
             FROM use_cases WHERE active = 1"
        )
        .fetch_all(pool)
        .await
    }

    // ── Stats ──────────────────────────────────────────────────────────────

    pub async fn get_user_stats(
        pool: &DbPool,
        user_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM token_history WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

        let saved: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(tokens_saved)::BIGINT, 0) FROM token_history WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        let avg_saved: (f64,) = sqlx::query_as(
            "SELECT COALESCE(AVG(tokens_saved)::FLOAT8, 0.0) FROM token_history WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(serde_json::json!({
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
            "avg_tokens_saved":   avg_saved.0,
        }))
    }

    // ── Admin ──────────────────────────────────────────────────────────────

    pub async fn list_all_users(pool: &DbPool) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, license, created_at, updated_at
             FROM users ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await
    }

    pub async fn delete_user(pool: &DbPool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn update_user_business_type(
        pool: &DbPool,
        id: &str,
        business_type: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET business_type = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
        )
        .bind(business_type)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get_global_stats(pool: &DbPool) -> Result<serde_json::Value, sqlx::Error> {
        let users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM token_history")
            .fetch_one(pool)
            .await?;

        let saved: (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(tokens_saved)::BIGINT, 0) FROM token_history")
                .fetch_one(pool)
                .await?;

        Ok(serde_json::json!({
            "total_users":        users.0,
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
        }))
    }

    pub async fn list_all_history(
        pool: &DbPool,
        limit: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
             "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
              FROM token_history ORDER BY created_at DESC LIMIT $1"
         )
         .bind(limit)
         .fetch_all(pool)
         .await
    }

    // ── Monitoring ───────────────────────────────────────────────────────────

    pub async fn get_system_performance(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // Get recent average response time and throughput metrics
        let recent_compressions: (Option<f64>, i64) = sqlx::query_as(
            "SELECT 
                  AVG(EXTRACT(EPOCH FROM (updated_at - created_at))) AS avg_processing_time,
                  COUNT(*) AS compression_count
               FROM token_history 
               WHERE created_at > NOW() - INTERVAL '1 hour'",
        )
        .fetch_one(pool)
        .await?;

        // Get concurrent user approximation (unique users in last hour)
        let active_users: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT user_id) 
               FROM token_history 
               WHERE created_at > NOW() - INTERVAL '1 hour'",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "avg_processing_time_seconds": recent_compressions.0.unwrap_or(0.0),
            "compressions_last_hour": recent_compressions.1,
            "active_users_last_hour": active_users.0
        }))
    }

    pub async fn get_learning_engine_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // Get cluster distribution and effectiveness stats
        let cluster_stats = sqlx::query_as::<_, (String, i64, Option<f64>, i64)>(
            "SELECT 
                   udp.cluster_label,
                   COUNT(*) as user_count,
                   AVG(udp.hits) as avg_hits,
                    SUM(udp.hits)::BIGINT as total_hits
                FROM user_domain_profile udp
                GROUP BY udp.cluster_label
                ORDER BY total_hits DESC",
        )
        .fetch_all(pool)
        .await?;

        let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;

        let total_clusters: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT cluster_id) FROM user_domain_profile")
                .fetch_one(pool)
                .await?;

        Ok(json!({
            "total_users": total_users.0,
            "total_clusters": total_clusters.0,
            "cluster_distribution": cluster_stats.iter().map(|(label, user_count, avg_hits, total_hits)| {
                json!({
                    "cluster_label": label,
                    "user_count": user_count,
                    "avg_hits_per_user": avg_hits.unwrap_or(0.0),
                    "total_hits": total_hits
                })
            }).collect::<Vec<_>>()
        }))
    }

    pub async fn get_compression_analytics(
        pool: &DbPool,
        time_window: Option<String>,
    ) -> Result<Value, sqlx::Error> {
        // Build time window condition
        let time_condition = match time_window.as_deref() {
            Some("24h") => "WHERE created_at > NOW() - INTERVAL '24 hours'",
            Some("7d") => "WHERE created_at > NOW() - INTERVAL '7 days'",
            Some("30d") => "WHERE created_at > NOW() - INTERVAL '30 days'",
            _ => "", // Default to all time
        };

        // Get compression effectiveness by use case and mode
        let analytics = sqlx::query_as::<_, (String, String, i64, Option<f64>, Option<f64>)>(
              &format!(
                  "SELECT 
                       use_case,
                       mode,
                       COUNT(*) as compression_count,
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((tokens_saved::FLOAT / NULLIF(token_original, 0)) * 100) as avg_savings_percent
                   FROM token_history 
                   {}
                   GROUP BY use_case, mode
                   ORDER BY compression_count DESC",
                  time_condition
              )
          )
          .fetch_all(pool)
          .await?;

        // Get overall stats
        let overall_stats: (Option<f64>, Option<f64>, i64) = sqlx::query_as(
              &format!(
                  "SELECT 
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((tokens_saved::FLOAT / NULLIF(token_original, 0)) * 100) as avg_savings_percent,
                       COUNT(*) as total_compressions
                   FROM token_history 
                   {}",
                  time_condition
              )
          )
          .fetch_one(pool)
          .await?;

        Ok(json!({
            "time_window": time_window.unwrap_or_else(|| "all_time".to_string()),
            "overall": {
                "avg_tokens_saved": overall_stats.0.unwrap_or(0.0),
                "avg_savings_percent": overall_stats.1.unwrap_or(0.0),
                "total_compressions": overall_stats.2
            },
            "by_use_case_and_mode": analytics.iter().map(|(use_case, mode, compression_count, avg_tokens_saved, avg_savings_percent)| {
                json!({
                    "use_case": use_case,
                    "mode": mode,
                    "compression_count": compression_count,
                    "avg_tokens_saved": avg_tokens_saved.unwrap_or(0.0),
                    "avg_savings_percent": avg_savings_percent.unwrap_or(0.0)
                })
            }).collect::<Vec<_>>()
        }))
    }

    pub async fn get_user_engagement_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // Get user engagement metrics
        let engagement_stats = sqlx::query_as::<
            _,
            (
                String,
                i64,
                chrono::DateTime<chrono::Utc>,
                chrono::DateTime<chrono::Utc>,
                Option<f64>,
            ),
        >(
            "SELECT 
                   user_id,
                   COUNT(*) as compression_count,
                   MAX(created_at) as last_activity,
                   MIN(created_at) as first_activity,
                   AVG(tokens_saved) as avg_tokens_saved
                FROM token_history 
                GROUP BY user_id
                ORDER BY compression_count DESC
                LIMIT 20",
        )
        .fetch_all(pool)
        .await?;

        let total_active_users: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT user_id) FROM token_history")
                .fetch_one(pool)
                .await?;

        let avg_compressions_per_user: (Option<f64>,) = sqlx::query_as(
            "SELECT AVG(compression_count) FROM (
                   SELECT COUNT(*) as compression_count 
                   FROM token_history 
                   GROUP BY user_id
               ) user_stats",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "total_active_users": total_active_users.0,
            "avg_compressions_per_user": avg_compressions_per_user.0.unwrap_or(0.0),
            "top_users": engagement_stats.iter().map(|(user_id, compression_count, last_activity, first_activity, avg_tokens_saved)| {
                json!({
                    "user_id": user_id,
                    "compression_count": compression_count,
                    "last_activity": last_activity,
                    "first_activity": first_activity,
                    "avg_tokens_saved": avg_tokens_saved.unwrap_or(0.0)
                })
            }).collect::<Vec<_>>()
        }))
    }

    pub async fn get_feedback_analysis(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // Get feedback trends and effectiveness
        let feedback_summary = sqlx::query_as::<_, (String, i64, Option<f64>)>(
            "SELECT 
                   signal_type,
                   COUNT(*) as signal_count,
                   AVG(value) as avg_value
                FROM feedback_signals 
                GROUP BY signal_type
                ORDER BY signal_count DESC",
        )
        .fetch_all(pool)
        .await?;

        // Get feedback by use case (joining with token_history)
        let feedback_by_use_case = sqlx::query_as::<_, (String, String, i64, Option<f64>)>(
            "SELECT 
                   th.use_case,
                   fs.signal_type,
                   COUNT(*) as signal_count,
                   AVG(fs.value) as avg_value
                FROM feedback_signals fs
                JOIN token_history th ON fs.history_id = th.id
                GROUP BY th.use_case, fs.signal_type
                ORDER BY th.use_case, signal_count DESC",
        )
        .fetch_all(pool)
        .await?;

        let total_feedback: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM feedback_signals")
            .fetch_one(pool)
            .await?;

        Ok(json!({
            "total_feedback_signals": total_feedback.0,
            "feedback_by_type": feedback_summary.iter().map(|(signal_type, signal_count, avg_value)| {
                json!({
                    "signal_type": signal_type,
                    "signal_count": signal_count,
                    "avg_value": avg_value.unwrap_or(0.0)
                })
            }).collect::<Vec<_>>(),
            "feedback_by_use_case": feedback_by_use_case.iter().map(|(use_case, signal_type, signal_count, avg_value)| {
                json!({
                    "use_case": use_case,
                    "signal_type": signal_type,
                    "signal_count": signal_count,
                    "avg_value": avg_value.unwrap_or(0.0)
                })
            }).collect::<Vec<_>>()
        }))
    }

    pub async fn get_error_metrics(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // Since we don't have an explicit error table, we'll monitor through
        // failed operations indicators and recent anomalies

        // Get recent compression failures (those with warnings or very low savings)
        let recent_issues: (i64, i64, f64) = sqlx::query_as(
            "SELECT 
                   COUNT(*) as total_recent,
                   COUNT(CASE WHEN warnings != '[]' THEN 1 END) as warnings_count,
                   AVG(CASE WHEN tokens_saved < 0 THEN 1 ELSE 0 END) * 100 as negative_savings_pct
                FROM token_history 
                WHERE created_at > NOW() - INTERVAL '24 hours'",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "monitoring_period": "last_24_hours",
            "total_compressions": recent_issues.0,
            "compressions_with_warnings": recent_issues.1,
            "percentage_with_warnings": if recent_issues.0 > 0 { recent_issues.1 as f64 / recent_issues.0 as f64 * 100.0 } else { 0.0 },
            "negative_savings_percentage": recent_issues.2
        }))
    }
}
