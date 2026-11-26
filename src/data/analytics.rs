//! User and tenant-scoped analytics, telemetry, and monitoring queries.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — Multi-tenant stats & admin metrics.

use db::DbPool;
use serde_json::{json, Value};

use super::Repository;

type UserTopRow = (String, String, String, i64, i64, i64, String, String);
type EngagementRow = (
    String,
    i64,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<f64>,
);

/// Formats user telemetry rows into the canonical `/api/stats` JSON payload.
fn format_user_stats_payload(
    summary: (i64, i64, f64, i64, i64, i64),
    mode_rows: Vec<(String, i64, i64)>,
    domain_rows: Vec<(String, i64, i64)>,
    daily_rows: Vec<(String, i64, i64, i64, i64)>,
    top_rows: Vec<UserTopRow>,
) -> Value {
    let mode_breakdown: Vec<Value> = mode_rows
        .into_iter()
        .map(|(mode, count, tokens_saved)| {
            json!({
                "mode": mode,
                "count": count,
                "tokens_saved": tokens_saved,
            })
        })
        .collect();

    let domain_breakdown: Vec<Value> = domain_rows
        .into_iter()
        .map(|(domain, count, tokens_saved)| {
            json!({
                "domain": domain,
                "count": count,
                "tokens_saved": tokens_saved,
            })
        })
        .collect();

    let daily_series: Vec<Value> = daily_rows
        .into_iter()
        .map(|(day, runs, original_tokens, final_tokens, tokens_saved)| {
            json!({
                "day": day,
                "runs": runs,
                "original_tokens": original_tokens,
                "final_tokens": final_tokens,
                "tokens_saved": tokens_saved,
            })
        })
        .collect();

    let top_runs: Vec<Value> = top_rows
        .into_iter()
        .map(
            |(
                id,
                use_case,
                mode,
                token_original,
                token_final,
                tokens_saved,
                created_at,
                preview,
            )| {
                json!({
                    "id": id,
                    "use_case": use_case,
                    "mode": mode,
                    "token_original": token_original,
                    "token_final": token_final,
                    "tokens_saved": tokens_saved,
                    "created_at": created_at,
                    "preview": preview,
                })
            },
        )
        .collect();

    json!({
        "total_compressions": summary.0,
        "total_tokens_saved": summary.1,
        "avg_tokens_saved": summary.2,
        "total_original_tokens": summary.3,
        "total_final_tokens": summary.4,
        "best_tokens_saved": summary.5,
        "mode_breakdown": mode_breakdown,
        "domain_breakdown": domain_breakdown,
        "daily_series": daily_series,
        "top_runs": top_runs,
    })
}

/// Resolves an allow-listed SQL `created_at` filter clause for `get_compression_analytics*`.
fn compression_time_filter_sql(time_window: Option<&str>, leading_conjunction: &str) -> String {
    // ARCH: §7 sqlx Rules — allow-list static time filter clauses rather than interpolating user input.
    let condition = match time_window {
        Some("24h") => "created_at > datetime('now', '-24 hours')",
        Some("7d") => "created_at > datetime('now', '-7 days')",
        Some("30d") => "created_at > datetime('now', '-30 days')",
        _ => return String::new(),
    };
    format!("{leading_conjunction} {condition}")
}

impl Repository {
    /// Returns aggregated compression telemetry defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] if any read query fails.
    ///
    /// # Panics
    /// Never.
    pub async fn get_user_stats(pool: &DbPool, user_id: &str) -> Result<Value, sqlx::Error> {
        Self::get_user_stats_scoped(pool, user_id, "default").await
    }

    /// Returns aggregated compression telemetry, mode/domain breakdowns, 14-day daily series,
    /// and top-saving runs for a given `user_id`, scoped strictly by `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] if any parameterized read query against `token_history` fails.
    ///
    /// # Panics
    /// Never.
    pub async fn get_user_stats_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let summary: (i64, i64, f64, i64, i64, i64) = sqlx::query_as(
            "SELECT
                COUNT(*),
                COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0),
                COALESCE(CAST(AVG(tokens_saved) AS REAL), 0.0),
                COALESCE(CAST(SUM(token_original) AS INTEGER), 0),
                COALESCE(CAST(SUM(token_final) AS INTEGER), 0),
                COALESCE(CAST(MAX(tokens_saved) AS INTEGER), 0)
             FROM token_history
             WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        let mode_rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT
                COALESCE(NULLIF(LOWER(mode), ''), 'balanced'),
                COUNT(*),
                COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0)
             FROM token_history
             WHERE user_id = $1 AND tenant_id = $2
             GROUP BY 1
             ORDER BY COUNT(*) DESC",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let domain_rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT
                COALESCE(NULLIF(use_case, ''), 'generic'),
                COUNT(*),
                COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0)
             FROM token_history
             WHERE user_id = $1 AND tenant_id = $2
             GROUP BY 1
             ORDER BY SUM(tokens_saved) DESC
             LIMIT 6",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let daily_rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT
                COALESCE(strftime('%Y-%m-%d', created_at), ''),
                COUNT(*),
                COALESCE(CAST(SUM(token_original) AS INTEGER), 0),
                COALESCE(CAST(SUM(token_final) AS INTEGER), 0),
                COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0)
             FROM token_history
             WHERE user_id = $1 AND tenant_id = $2 AND created_at >= datetime('now', '-14 days')
             GROUP BY 1
             ORDER BY 1 ASC",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let top_rows: Vec<UserTopRow> = sqlx::query_as(
            "SELECT
                id,
                COALESCE(NULLIF(use_case, ''), 'generic'),
                COALESCE(NULLIF(LOWER(mode), ''), 'balanced'),
                token_original,
                token_final,
                tokens_saved,
                COALESCE(CAST(created_at AS TEXT), ''),
                SUBSTR(original_prompt, 1, 120)
             FROM token_history
             WHERE user_id = $1 AND tenant_id = $2
             ORDER BY tokens_saved DESC
             LIMIT 3",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(format_user_stats_payload(
            summary,
            mode_rows,
            domain_rows,
            daily_rows,
            top_rows,
        ))
    }

    /// Returns global platform statistics across all tenants.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_global_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
        let users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM token_history")
            .fetch_one(pool)
            .await?;

        let saved: (i64,) = sqlx::query_as(
            "SELECT COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0) FROM token_history",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "total_users":        users.0,
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
        }))
    }

    /// Returns tenant-scoped platform statistics.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_tenant_stats(pool: &DbPool, tenant_id: &str) -> Result<Value, sqlx::Error> {
        let users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

        let total: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_history WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await?;

        let saved: (i64,) = sqlx::query_as(
            "SELECT COALESCE(CAST(SUM(tokens_saved) AS INTEGER), 0) FROM token_history WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "total_users":        users.0,
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
        }))
    }

    /// Computes global system performance diagnostics.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_system_performance(pool: &DbPool) -> Result<Value, sqlx::Error> {
        let recent_compressions: (Option<f64>, i64) = sqlx::query_as(
            "SELECT
                  AVG((julianday(updated_at) - julianday(created_at)) * 86400.0) AS avg_processing_time,
                  COUNT(*) AS compression_count
               FROM token_history
               WHERE created_at > datetime('now', '-1 hour')",
        )
        .fetch_one(pool)
        .await?;

        let active_users: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT user_id)
               FROM token_history
               WHERE created_at > datetime('now', '-1 hour')",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "avg_processing_time_seconds": recent_compressions.0.unwrap_or(0.0),
            "compressions_last_hour": recent_compressions.1,
            "active_users_last_hour": active_users.0
        }))
    }

    /// Computes system performance diagnostics scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_system_performance_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let recent_compressions: (Option<f64>, i64) = sqlx::query_as(
            "SELECT
                  AVG((julianday(updated_at) - julianday(created_at)) * 86400.0) AS avg_processing_time,
                  COUNT(*) AS compression_count
               FROM token_history
               WHERE tenant_id = $1 AND created_at > datetime('now', '-1 hour')",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        let active_users: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT user_id)
               FROM token_history
               WHERE tenant_id = $1 AND created_at > datetime('now', '-1 hour')",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "avg_processing_time_seconds": recent_compressions.0.unwrap_or(0.0),
            "compressions_last_hour": recent_compressions.1,
            "active_users_last_hour": active_users.0
        }))
    }

    /// Returns aggregate learning engine cluster distribution stats.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_learning_engine_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
        let cluster_stats = sqlx::query_as::<_, (String, i64, Option<f64>, i64)>(
            "SELECT
                   udp.cluster_label,
                   COUNT(*) as user_count,
                   AVG(udp.hits) as avg_hits,
                   CAST(SUM(udp.hits) AS INTEGER) as total_hits
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

    /// Returns aggregate learning engine cluster distribution stats scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_learning_engine_stats_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let cluster_stats = sqlx::query_as::<_, (String, i64, Option<f64>, i64)>(
            "SELECT
                   udp.cluster_label,
                   COUNT(*) as user_count,
                   AVG(udp.hits) as avg_hits,
                   CAST(SUM(udp.hits) AS INTEGER) as total_hits
             FROM user_domain_profile udp
             WHERE udp.tenant_id = $1
             GROUP BY udp.cluster_label
             ORDER BY total_hits DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

        let total_clusters: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT cluster_id) FROM user_domain_profile WHERE tenant_id = $1",
        )
        .bind(tenant_id)
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

    /// Returns compression analytics aggregated over a given `time_window`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_compression_analytics(
        pool: &DbPool,
        time_window: Option<String>,
    ) -> Result<Value, sqlx::Error> {
        let time_condition = compression_time_filter_sql(time_window.as_deref(), "WHERE");

        let analytics = sqlx::query_as::<_, (String, String, i64, Option<f64>, Option<f64>)>(
            &format!(
                "SELECT
                       use_case,
                       mode,
                       COUNT(*) as compression_count,
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent
                   FROM token_history
                   {time_condition}
                   GROUP BY use_case, mode
                   ORDER BY compression_count DESC"
            ),
        )
        .fetch_all(pool)
        .await?;

        let overall_stats: (Option<f64>, Option<f64>, i64) = sqlx::query_as(&format!(
            "SELECT
                   AVG(tokens_saved) as avg_tokens_saved,
                   AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent,
                   COUNT(*) as total_compressions
               FROM token_history
               {time_condition}"
        ))
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

    /// Returns compression analytics aggregated over a given `time_window`, scoped to `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_compression_analytics_scoped(
        pool: &DbPool,
        tenant_id: &str,
        time_window: Option<String>,
    ) -> Result<Value, sqlx::Error> {
        let time_condition = compression_time_filter_sql(time_window.as_deref(), "AND");

        let analytics = sqlx::query_as::<_, (String, String, i64, Option<f64>, Option<f64>)>(
            &format!(
                "SELECT
                       use_case,
                       mode,
                       COUNT(*) as compression_count,
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent
                   FROM token_history
                   WHERE tenant_id = $1
                   {time_condition}
                   GROUP BY use_case, mode
                   ORDER BY compression_count DESC"
            ),
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let overall_stats: (Option<f64>, Option<f64>, i64) = sqlx::query_as(&format!(
            "SELECT
                   AVG(tokens_saved) as avg_tokens_saved,
                   AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent,
                   COUNT(*) as total_compressions
               FROM token_history
               WHERE tenant_id = $1
               {time_condition}"
        ))
        .bind(tenant_id)
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

    /// Computes top active user statistics across all tenants.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_user_engagement_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
        let engagement_stats = sqlx::query_as::<_, EngagementRow>(
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

    /// Computes top active user statistics scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_user_engagement_stats_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let engagement_stats = sqlx::query_as::<_, EngagementRow>(
            "SELECT
                   user_id,
                   COUNT(*) as compression_count,
                   MAX(created_at) as last_activity,
                   MIN(created_at) as first_activity,
                   AVG(tokens_saved) as avg_tokens_saved
                FROM token_history
                WHERE tenant_id = $1
                GROUP BY user_id
                ORDER BY compression_count DESC
                LIMIT 20",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let total_active_users: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT user_id) FROM token_history WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        let avg_compressions_per_user: (Option<f64>,) = sqlx::query_as(
            "SELECT AVG(compression_count) FROM (
                   SELECT COUNT(*) as compression_count
                   FROM token_history
                   WHERE tenant_id = $1
                   GROUP BY user_id
               ) user_stats",
        )
        .bind(tenant_id)
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

    /// Aggregates explicit and implicit feedback signals across all tenants.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_feedback_analysis(pool: &DbPool) -> Result<Value, sqlx::Error> {
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

        let feedback_by_use_case = sqlx::query_as::<_, (String, String, i64, Option<f64>)>(
            "SELECT
                   COALESCE(NULLIF(th.use_case, ''), 'general') as use_case,
                   fs.signal_type,
                   COUNT(*) as signal_count,
                   AVG(fs.value) as avg_value
                FROM feedback_signals fs
                LEFT JOIN token_history th ON fs.history_id = th.id
                GROUP BY 1, fs.signal_type
                ORDER BY 1, signal_count DESC",
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

    /// Aggregates feedback signals scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_feedback_analysis_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let feedback_summary = sqlx::query_as::<_, (String, i64, Option<f64>)>(
            "SELECT
                   signal_type,
                   COUNT(*) as signal_count,
                   AVG(value) as avg_value
                FROM feedback_signals
                WHERE tenant_id = $1
                GROUP BY signal_type
                ORDER BY signal_count DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let feedback_by_use_case = sqlx::query_as::<_, (String, String, i64, Option<f64>)>(
            "SELECT
                   COALESCE(NULLIF(th.use_case, ''), 'general') as use_case,
                   fs.signal_type,
                   COUNT(*) as signal_count,
                   AVG(fs.value) as avg_value
                FROM feedback_signals fs
                LEFT JOIN token_history th ON fs.history_id = th.id AND th.tenant_id = $1
                WHERE fs.tenant_id = $1
                GROUP BY 1, fs.signal_type
                ORDER BY 1, signal_count DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let total_feedback: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM feedback_signals WHERE tenant_id = $1")
                .bind(tenant_id)
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

    /// Evaluates error metrics and anomalous compression runs over the last 24 hours.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::cast_precision_loss)] // Aggregate 24h counts fit well within f64 53-bit mantissa.
    pub async fn get_error_metrics(pool: &DbPool) -> Result<Value, sqlx::Error> {
        // FIX: COALESCE + Option<f64> prevents ColumnDecode NULL crash when 0 rows exist in the 24h window.
        let recent_issues: (i64, i64, Option<f64>) = sqlx::query_as(
            "SELECT
                   COUNT(*) as total_recent,
                   COUNT(CASE WHEN warnings != '[]' THEN 1 END) as warnings_count,
                   COALESCE(CAST(AVG(CASE WHEN tokens_saved < 0 THEN 1.0 ELSE 0.0 END) * 100.0 AS REAL), 0.0) as negative_savings_pct
                FROM token_history
                WHERE created_at > datetime('now', '-24 hours')",
        )
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "monitoring_period": "last_24_hours",
            "total_compressions": recent_issues.0,
            "compressions_with_warnings": recent_issues.1,
            "percentage_with_warnings": if recent_issues.0 > 0 { recent_issues.1 as f64 / recent_issues.0 as f64 * 100.0 } else { 0.0 },
            "negative_savings_percentage": recent_issues.2.unwrap_or(0.0)
        }))
    }

    /// Evaluates error metrics and anomalous compression runs scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::cast_precision_loss)] // Aggregate 24h counts fit well within f64 53-bit mantissa.
    pub async fn get_error_metrics_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        // FIX: COALESCE + Option<f64> prevents ColumnDecode NULL crash when a tenant has 0 rows in the 24h window.
        let recent_issues: (i64, i64, Option<f64>) = sqlx::query_as(
            "SELECT
                   COUNT(*) as total_recent,
                   COUNT(CASE WHEN warnings != '[]' THEN 1 END) as warnings_count,
                   COALESCE(CAST(AVG(CASE WHEN tokens_saved < 0 THEN 1.0 ELSE 0.0 END) * 100.0 AS REAL), 0.0) as negative_savings_pct
                FROM token_history
                WHERE tenant_id = $1 AND created_at > datetime('now', '-24 hours')",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

        Ok(json!({
            "monitoring_period": "last_24_hours",
            "total_compressions": recent_issues.0,
            "compressions_with_warnings": recent_issues.1,
            "percentage_with_warnings": if recent_issues.0 > 0 { recent_issues.1 as f64 / recent_issues.0 as f64 * 100.0 } else { 0.0 },
            "negative_savings_percentage": recent_issues.2.unwrap_or(0.0)
        }))
    }
}
