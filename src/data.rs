//! Database repository and multi-tenant persistence layer.
//!
//! ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules, §8 Vector DB Rules.

use crate::engine::pipeline::PipelineOutput;
use crate::models::{
    feedback_signal::FeedbackSignal,
    token_history::TokenHistory,
    use_case::UseCase,
    user::User,
    verbose::{SqlxEvent, SqlxVerbose},
};
use crate::types::CompressionResponse;
use db::DbPool;
use serde_json::{json, Value};
use std::time::Instant;

/// Central data repository providing CRUD operations for users, token history,
/// feedback signals, and use cases, with multi-tenant isolation.
#[derive(Debug)]
pub struct Repository;

impl Repository {
    // ── Users ──────────────────────────────────────────────────────────────

    /// Creates a user defaulting to 'user' role and 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn create_user(
        pool: &DbPool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
    ) -> Result<User, sqlx::Error> {
        Self::create_user_scoped(
            pool,
            id,
            username,
            password_hash,
            email,
            business_type,
            "user",
            "default",
        )
        .await
    }

    /// Creates a user with explicit role and tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn create_user_scoped(
        pool: &DbPool,
        id: &str,
        username: &str,
        password_hash: &str,
        email: &str,
        business_type: &str,
        role: &str,
        tenant_id: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, username, password_hash, email, business_type, role, tenant_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at"
        )
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .bind(email)
        .bind(business_type)
        .bind(role)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
    }

    /// Finds a user by ID, selecting role and tenant_id.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn find_user_by_id(pool: &DbPool, id: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Finds a user by username, selecting role and tenant_id.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn find_user_by_username(
        pool: &DbPool,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE username = $1"
        )
        .bind(username)
        .fetch_optional(pool)
        .await
    }

    // ── Token History ──────────────────────────────────────────────────────

    /// Inserts compression history defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
    ) -> Result<(), sqlx::Error> {
        Self::insert_history_scoped(pool, id, user_id, "default", original_prompt, output).await
    }

    /// Inserts compression history with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history_scoped(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        tenant_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
    ) -> Result<(), sqlx::Error> {
        let principle_logs =
            serde_json::to_string(&output.principle_logs).unwrap_or_else(|_| "[]".into());
        let warnings = serde_json::to_string(&output.warnings).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
        )
        .bind(id)
        .bind(user_id)
        .bind(tenant_id)
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

    /// Inserts verbose compression history defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history_verbose(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
        verbose: &mut SqlxVerbose,
    ) -> Result<(), sqlx::Error> {
        Self::insert_history_verbose_scoped(
            pool,
            id,
            user_id,
            "default",
            original_prompt,
            output,
            verbose,
        )
        .await
    }

    /// Inserts verbose compression history with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history_verbose_scoped(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        tenant_id: &str,
        original_prompt: &str,
        output: &PipelineOutput,
        verbose: &mut SqlxVerbose,
    ) -> Result<(), sqlx::Error> {
        let sql = "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)";
        let start = Instant::now();
        let result = sqlx::query(sql)
            .bind(id)
            .bind(user_id)
            .bind(tenant_id)
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

    /// Insert compression history from the new 7-stage pipeline response defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history_from_compression(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        original_prompt: &str,
        response: &CompressionResponse,
        token_original: usize,
        token_final: usize,
        token_saved: usize,
        engine_version: &str,
        verbose: &mut SqlxVerbose,
    ) -> Result<(), sqlx::Error> {
        Self::insert_history_from_compression_scoped(
            pool,
            id,
            user_id,
            "default",
            original_prompt,
            response,
            token_original,
            token_final,
            token_saved,
            engine_version,
            verbose,
        )
        .await
    }

    /// Insert compression history from the new 7-stage pipeline response with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_history_from_compression_scoped(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        tenant_id: &str,
        original_prompt: &str,
        response: &CompressionResponse,
        token_original: usize,
        token_final: usize,
        token_saved: usize,
        engine_version: &str,
        verbose: &mut SqlxVerbose,
    ) -> Result<(), sqlx::Error> {
        let sql = "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)";

        // Store pipeline diagnostics as principle_logs JSON
        let pipeline_diagnostics = serde_json::json!({
            "pipeline_version": "7-stage",
            "topology": format!("{:?}", response.topology),
            "mode": &response.mode,
            "error_score": response.error_score,
            "fidelity": response.fidelity,
            "dual_score": {
                "tes": response.scoring_result.tes,
                "sfs": response.scoring_result.sfs,
                "overall": (response.scoring_result.tes + response.scoring_result.sfs + response.scoring_result.scs) / 3.0
            },
            "wm_slots_used": response.wm_slots_used,
            "null_fields": &response.null_fields,
            "field_issues_count": response.field_issues.len(),
            "scope_injections_count": response.scope_injections.len(),
            "correction_cycle": response.correction_cycle.cycle_number,
        });

        // Store field issues as warnings
        let warnings: Vec<String> = response
            .field_issues
            .iter()
            .map(|issue| format!("{}: {}", issue.field_name, issue.description))
            .collect();

        let start = Instant::now();
        let result = sqlx::query(sql)
            .bind(id)
            .bind(user_id)
            .bind(tenant_id)
            .bind(original_prompt)
            .bind(&response.response)
            .bind(token_saved as i64)
            .bind(token_original as i64)
            .bind(token_final as i64)
            .bind("auto") // use_case — new pipeline auto-detects
            .bind(&response.mode)
            .bind(engine_version)
            .bind(pipeline_diagnostics)
            .bind(serde_json::json!(warnings))
            .execute(pool)
            .await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        verbose.push(SqlxEvent {
            operation: "insert_history_compression".into(),
            duration_ms,
            row_count: Some(result.rows_affected() as i64),
            sql: Some("token_history.insert_compression".into()),
        });

        Ok(())
    }

    /// Fetches a specific history record defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_history(
        pool: &DbPool,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        Self::get_history_scoped(pool, user_id, "default", id).await
    }

    /// Fetches a specific history record filtered by tenant, user, and ID.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_history_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE id = $1 AND user_id = $2 AND tenant_id = $3"
        )
        .bind(id)
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
    }

    /// Lists history records defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_history(
        pool: &DbPool,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        Self::list_history_scoped(pool, user_id, "default", limit, offset).await
    }

    /// Lists history records filtered by tenant and user.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_history_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE user_id = $1 AND tenant_id = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4"
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    // ── Feedback ───────────────────────────────────────────────────────────

    /// Inserts a feedback signal defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_feedback(
        pool: &DbPool,
        signal: &FeedbackSignal,
    ) -> Result<(), sqlx::Error> {
        Self::insert_feedback_scoped(pool, signal, "default").await
    }

    /// Inserts a feedback signal with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn insert_feedback_scoped(
        pool: &DbPool,
        signal: &FeedbackSignal,
        tenant_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO feedback_signals (id, user_id, tenant_id, history_id, signal_type, signal_layer, value, meta, detected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(&signal.id)
        .bind(&signal.user_id)
        .bind(tenant_id)
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

    /// Lists active use case definitions.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_use_cases(pool: &DbPool) -> Result<Vec<UseCase>, sqlx::Error> {
        sqlx::query_as::<_, UseCase>(
            "SELECT id, key, version, role_frame, output_format, chunk_strategy, description, active, created_at, updated_at
             FROM use_cases WHERE active = 1"
        )
        .fetch_all(pool)
        .await
    }

    // ── Stats ──────────────────────────────────────────────────────────────

    /// Returns aggregated compression telemetry defaulting to 'default' tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` if any read query fails.
    pub async fn get_user_stats(
        pool: &DbPool,
        user_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
        Self::get_user_stats_scoped(pool, user_id, "default").await
    }

    /// Returns aggregated compression telemetry, mode/domain breakdowns, 14-day daily series,
    /// and top-saving runs for a given user, scoped strictly by tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` if any parameterized read query against `token_history` fails.
    pub async fn get_user_stats_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
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

        let top_rows: Vec<(String, String, String, i64, i64, i64, String, String)> =
            sqlx::query_as(
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

        Ok(serde_json::json!({
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
        }))
    }

    // ── Admin ──────────────────────────────────────────────────────────────

    /// Lists all users paginated, selecting role and tenant_id.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_all_users(
        pool: &DbPool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Lists users scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_users_by_tenant(
        pool: &DbPool,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
             FROM users WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Counts active administrative users based on `role = 'admin'`.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn count_admins(pool: &DbPool) -> Result<i64, sqlx::Error> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin'")
                .fetch_one(pool)
                .await?;
        Ok(count.0)
    }

    /// Counts active administrative users within a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn count_admins_scoped(pool: &DbPool, tenant_id: &str) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin' AND tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
        Ok(count.0)
    }

    /// Deletes a user by ID.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn delete_user(pool: &DbPool, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Deletes a user by ID scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn delete_user_scoped(
        pool: &DbPool,
        tenant_id: &str,
        id: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Updates a user's business type.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Updates a user's business type scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn update_user_business_type_scoped(
        pool: &DbPool,
        tenant_id: &str,
        id: &str,
        business_type: &str,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            "UPDATE users SET business_type = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2 AND tenant_id = $3",
        )
        .bind(business_type)
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Updates a user's password hash.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn update_user_password(
        pool: &DbPool,
        id: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE users SET password_hash = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
        )
        .bind(password_hash)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Returns global platform statistics across all tenants.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_global_stats(pool: &DbPool) -> Result<serde_json::Value, sqlx::Error> {
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

        Ok(serde_json::json!({
            "total_users":        users.0,
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
        }))
    }

    /// Returns tenant-scoped platform statistics.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_tenant_stats(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
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

        Ok(serde_json::json!({
            "total_users":        users.0,
            "total_compressions": total.0,
            "total_tokens_saved": saved.0,
        }))
    }

    /// Lists all history records across all users and tenants for administrative audit.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_all_history(
        pool: &DbPool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
             "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
              FROM token_history ORDER BY created_at DESC LIMIT $1 OFFSET $2"
         )
         .bind(limit)
         .bind(offset)
         .fetch_all(pool)
         .await
    }

    /// Lists history records scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn list_history_by_tenant(
        pool: &DbPool,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
             "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
              FROM token_history WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
         )
         .bind(tenant_id)
         .bind(limit)
         .bind(offset)
         .fetch_all(pool)
         .await
    }

    // ── Monitoring ───────────────────────────────────────────────────────────

    /// Computes system performance diagnostics.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Returns aggregate learning engine cluster distribution stats.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Returns compression analytics aggregated over a given time window.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_compression_analytics(
        pool: &DbPool,
        time_window: Option<String>,
    ) -> Result<Value, sqlx::Error> {
        let time_condition = match time_window.as_deref() {
            Some("24h") => "WHERE created_at > datetime('now', '-24 hours')",
            Some("7d") => "WHERE created_at > datetime('now', '-7 days')",
            Some("30d") => "WHERE created_at > datetime('now', '-30 days')",
            _ => "",
        };

        let analytics = sqlx::query_as::<_, (String, String, i64, Option<f64>, Option<f64>)>(
              &format!(
                  "SELECT 
                       use_case,
                       mode,
                       COUNT(*) as compression_count,
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent
                   FROM token_history 
                   {}
                   GROUP BY use_case, mode
                   ORDER BY compression_count DESC",
                  time_condition
              )
          )
          .fetch_all(pool)
          .await?;

        let overall_stats: (Option<f64>, Option<f64>, i64) = sqlx::query_as(
              &format!(
                  "SELECT 
                       AVG(tokens_saved) as avg_tokens_saved,
                       AVG((CAST(tokens_saved AS REAL) / NULLIF(token_original, 0)) * 100) as avg_savings_percent,
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

    /// Computes top active user statistics.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_user_engagement_stats(pool: &DbPool) -> Result<Value, sqlx::Error> {
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

    /// Aggregates explicit and implicit feedback signals.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Evaluates error metrics and anomalous compression runs over the last 24 hours.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_error_metrics(pool: &DbPool) -> Result<Value, sqlx::Error> {
        let recent_issues: (i64, i64, f64) = sqlx::query_as(
            "SELECT 
                   COUNT(*) as total_recent,
                   COUNT(CASE WHEN warnings != '[]' THEN 1 END) as warnings_count,
                   AVG(CASE WHEN tokens_saved < 0 THEN 1 ELSE 0 END) * 100 as negative_savings_pct
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
            "negative_savings_percentage": recent_issues.2
        }))
    }

    /// Computes system performance diagnostics scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Returns aggregate learning engine cluster distribution stats scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Returns compression analytics aggregated over a given time window, scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_compression_analytics_scoped(
        pool: &DbPool,
        tenant_id: &str,
        time_window: Option<String>,
    ) -> Result<Value, sqlx::Error> {
        let time_condition = match time_window.as_deref() {
            Some("24h") => "AND created_at > datetime('now', '-24 hours')",
            Some("7d") => "AND created_at > datetime('now', '-7 days')",
            Some("30d") => "AND created_at > datetime('now', '-30 days')",
            _ => "",
        };

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
                   {}
                   GROUP BY use_case, mode
                   ORDER BY compression_count DESC",
                time_condition
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
                   {}",
            time_condition
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

    /// Computes top active user statistics scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_user_engagement_stats_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
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

    /// Aggregates feedback signals scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
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

    /// Evaluates error metrics and anomalous compression runs scoped to a specific tenant.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on query execution failure.
    pub async fn get_error_metrics_scoped(
        pool: &DbPool,
        tenant_id: &str,
    ) -> Result<Value, sqlx::Error> {
        let recent_issues: (i64, i64, f64) = sqlx::query_as(
            "SELECT 
                   COUNT(*) as total_recent,
                   COUNT(CASE WHEN warnings != '[]' THEN 1 END) as warnings_count,
                   AVG(CASE WHEN tokens_saved < 0 THEN 1 ELSE 0 END) * 100 as negative_savings_pct
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
            "negative_savings_percentage": recent_issues.2
        }))
    }
}
