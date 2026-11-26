//! Token compression history persistence and tenant-scoped queries.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — `token_history` persistence & isolation.

use std::time::Instant;

use db::DbPool;

use super::Repository;
use crate::{
    models::{
        token_history::TokenHistory,
        verbose::{SqlxEvent, SqlxVerbose},
    },
    types::CompressionResponse,
};

impl Repository {
    /// Inserts a 13-column `token_history` record scoped to `(tenant_id, user_id)`.
    ///
    /// Used by Aggressive Mode and Scenario Mode callers that construct `principle_logs` and
    /// `warnings` JSON payloads directly.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::too_many_arguments)] // Maps 1:1 to the 13 persisted `token_history` columns.
    pub async fn insert_token_history_record_scoped(
        pool: &DbPool,
        id: &str,
        user_id: &str,
        tenant_id: &str,
        original_prompt: &str,
        optimized_prompt: &str,
        tokens_saved: i64,
        token_original: i64,
        token_final: i64,
        use_case: &str,
        mode: &str,
        engine_version: &str,
        principle_logs: serde_json::Value,
        warnings: serde_json::Value,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(original_prompt)
        .bind(optimized_prompt)
        .bind(tokens_saved)
        .bind(token_original)
        .bind(token_final)
        .bind(use_case)
        .bind(mode)
        .bind(engine_version)
        .bind(principle_logs)
        .bind(warnings)
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Inserts compression history from a 7-stage [`CompressionResponse`] defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::too_many_arguments)] // Preserves existing public Repository signature.
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

    /// Inserts compression history from a 7-stage [`CompressionResponse`] with explicit `tenant_id` scoping.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    #[allow(clippy::too_many_arguments)] // Preserves existing public Repository signature.
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
            .bind(i64::try_from(token_saved).unwrap_or(i64::MAX))
            .bind(i64::try_from(token_original).unwrap_or(i64::MAX))
            .bind(i64::try_from(token_final).unwrap_or(i64::MAX))
            .bind("auto") // use_case — new pipeline auto-detects
            .bind(&response.mode)
            .bind(engine_version)
            .bind(pipeline_diagnostics)
            .bind(serde_json::json!(warnings))
            .execute(pool)
            .await?;
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        verbose.push(SqlxEvent {
            operation: "insert_history_compression".into(),
            duration_ms,
            row_count: Some(i64::try_from(result.rows_affected()).unwrap_or(i64::MAX)),
            sql: Some("token_history.insert_compression".into()),
        });

        Ok(())
    }

    /// Fetches a specific history record defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_history(
        pool: &DbPool,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        Self::get_history_scoped(pool, user_id, "default", id).await
    }

    /// Fetches a specific history record filtered by `tenant_id`, `user_id`, and `id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn get_history_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE id = $1 AND user_id = $2 AND tenant_id = $3",
        )
        .bind(id)
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
    }

    /// Lists history records defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_history(
        pool: &DbPool,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        Self::list_history_scoped(pool, user_id, "default", limit, offset).await
    }

    /// Lists history records filtered by `tenant_id` and `user_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_history_scoped(
        pool: &DbPool,
        user_id: &str,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE user_id = $1 AND tenant_id = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Lists all history records across all users and tenants for administrative audit.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_all_history(
        pool: &DbPool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// Lists history records scoped to a specific `tenant_id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on query execution failure.
    ///
    /// # Panics
    /// Never.
    pub async fn list_history_by_tenant(
        pool: &DbPool,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TokenHistory>, sqlx::Error> {
        sqlx::query_as::<_, TokenHistory>(
            "SELECT id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings, created_at
             FROM token_history WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}
