//! Database query routines for prompts, neural state, and user domain profiles.
//!
//! ARCH: §7 sqlx Rules, §8 Vector DB Rules, §2 Component Map — Multi-tenant persistence.

use competitive_core::centroid::CentroidSnapshot;
use hebbian_core::WeightDelta;
use schema_engine::types::PromptSchema;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// Saves a user prompt and its parsed schema to the `user_prompts` table, scoped by tenant.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn save_prompt(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    raw: &str,
    schema: &PromptSchema,
) -> Result<Uuid, sqlx::Error> {
    save_prompt_scoped(pool, tenant_id, user_id, raw, schema).await
}

/// Saves a user prompt and its parsed schema with explicit tenant scoping.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn save_prompt_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    raw: &str,
    schema: &PromptSchema,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let schema_json = serde_json::to_value(schema).unwrap_or_default();

    sqlx::query(
        r"
        INSERT INTO user_prompts (id, user_id, tenant_id, raw_text, schema_json, token_count)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(tenant_id)
    .bind(raw)
    .bind(schema_json)
    .bind(i32::try_from(schema.token_count).unwrap_or(i32::MAX))
    .execute(pool)
    .await?;

    Ok(id)
}

/// Backward-compatible wrapper for saving prompts defaulting to 'default' tenant.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn save_prompt_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
    raw: &str,
    schema: &PromptSchema,
) -> Result<Uuid, sqlx::Error> {
    save_prompt_scoped(pool, "default", user_id, raw, schema).await
}

/// Upserts top co-activation weight deltas into `learned_context`, scoped by tenant.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn upsert_learned_context(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    deltas: &[WeightDelta],
) -> Result<(), sqlx::Error> {
    upsert_learned_context_scoped(pool, tenant_id, user_id, deltas).await
}

/// Upserts top co-activation weight deltas with explicit tenant scoping.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn upsert_learned_context_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    deltas: &[WeightDelta],
) -> Result<(), sqlx::Error> {
    let pairs = serde_json::to_value(deltas).unwrap_or_default();

    sqlx::query(
        r"
        INSERT INTO learned_context (id, user_id, tenant_id, top_pairs)
        VALUES (lower(hex(randomblob(16))), $1, $2, $3)
        ON CONFLICT (user_id) DO UPDATE SET
            top_pairs  = EXCLUDED.top_pairs,
            tenant_id  = EXCLUDED.tenant_id,
            updated_at = CURRENT_TIMESTAMP
        ",
    )
    .bind(user_id.to_string())
    .bind(tenant_id)
    .bind(pairs)
    .execute(pool)
    .await?;

    Ok(())
}

/// Loads learned context weight deltas with explicit tenant scoping.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_learned_context_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
) -> Result<Vec<WeightDelta>, sqlx::Error> {
    let row = sqlx::query(
        r"
        SELECT top_pairs
        FROM   learned_context
        WHERE  user_id = $1 AND tenant_id = $2
        ",
    )
    .bind(user_id.to_string())
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let val: serde_json::Value = r.try_get("top_pairs")?;
        Ok(serde_json::from_value(val).unwrap_or_default())
    } else {
        Ok(vec![])
    }
}

/// Backward-compatible wrapper for loading learned context defaulting to 'default' tenant.
///
/// Wraps [`load_learned_context_scoped`] using `"default"` as the tenant identifier for legacy callers.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_learned_context_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Vec<WeightDelta>, sqlx::Error> {
    load_learned_context_scoped(pool, "default", user_id).await
}

/// Backward-compatible wrapper for upserting learned context defaulting to 'default' tenant.
///
/// Wraps [`upsert_learned_context_scoped`] using `"default"` as the tenant identifier for legacy callers.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn upsert_learned_context_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
    deltas: &[WeightDelta],
) -> Result<(), sqlx::Error> {
    upsert_learned_context_scoped(pool, "default", user_id, deltas).await
}

/// Upserts domain centroid profiles into `user_domain_profile`, scoped by tenant.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn upsert_domain_profile(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    snapshots: &[CentroidSnapshot],
) -> Result<(), sqlx::Error> {
    upsert_domain_profile_scoped(pool, tenant_id, user_id, snapshots).await
}

/// Upserts domain centroid profiles with explicit tenant scoping inside a single database transaction.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure or transaction rollback.
pub async fn upsert_domain_profile_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    snapshots: &[CentroidSnapshot],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for snap in snapshots {
        let top_tokens = serde_json::to_value(&snap.top_tokens).unwrap_or_default();

        sqlx::query(
            r"
            INSERT INTO user_domain_profile
                (id, user_id, tenant_id, cluster_id, cluster_label, top_tokens, hits)
            VALUES (lower(hex(randomblob(16))), $1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, cluster_id) DO UPDATE SET
                cluster_label = EXCLUDED.cluster_label,
                top_tokens    = EXCLUDED.top_tokens,
                hits          = EXCLUDED.hits,
                tenant_id     = EXCLUDED.tenant_id,
                updated_at    = CURRENT_TIMESTAMP
            ",
        )
        .bind(user_id.to_string())
        .bind(tenant_id)
        .bind(i32::try_from(snap.id).unwrap_or(i32::MAX))
        .bind(&snap.label)
        .bind(top_tokens)
        .bind(snap.hits.cast_signed())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Backward-compatible wrapper for upserting domain profiles defaulting to 'default' tenant.
///
/// Wraps [`upsert_domain_profile_scoped`] using `"default"` as the tenant identifier for legacy callers.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn upsert_domain_profile_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
    snapshots: &[CentroidSnapshot],
) -> Result<(), sqlx::Error> {
    upsert_domain_profile_scoped(pool, "default", user_id, snapshots).await
}

/// Loads domain centroid snapshots with explicit tenant scoping.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_domain_profiles_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
) -> Result<Vec<CentroidSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        r"
        SELECT cluster_id, cluster_label, top_tokens, hits
        FROM   user_domain_profile
        WHERE  user_id = $1 AND tenant_id = $2
        ORDER BY cluster_id ASC
        ",
    )
    .bind(user_id.to_string())
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    let mut snapshots = Vec::with_capacity(rows.len());
    for r in rows {
        let cluster_id: i32 = r.try_get("cluster_id")?;
        let cluster_label: Option<String> = r.try_get("cluster_label")?;
        let top_tokens_val: serde_json::Value = r.try_get("top_tokens")?;
        let hits: i32 = r.try_get("hits")?;

        let top_tokens: Vec<(String, f32)> =
            serde_json::from_value(top_tokens_val).unwrap_or_default();

        snapshots.push(CentroidSnapshot {
            id: usize::try_from(cluster_id.max(0)).unwrap_or(0),
            label: cluster_label,
            top_tokens,
            hits: hits.max(0).cast_unsigned(),
        });
    }

    Ok(snapshots)
}

/// Backward-compatible wrapper for loading domain centroid profiles defaulting to 'default' tenant.
///
/// Wraps [`load_domain_profiles_scoped`] using `"default"` as the tenant identifier for legacy callers.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_domain_profiles_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Vec<CentroidSnapshot>, sqlx::Error> {
    load_domain_profiles_scoped(pool, "default", user_id).await
}

/// Loads cluster vocabulary for a specific tenant and user.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_cluster_vocab(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    cluster_id: i32,
) -> Result<Vec<(String, f32)>, sqlx::Error> {
    load_cluster_vocab_scoped(pool, tenant_id, user_id, cluster_id).await
}

/// Loads cluster vocabulary with explicit tenant scoping.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_cluster_vocab_scoped(
    pool: &SqlitePool,
    tenant_id: &str,
    user_id: Uuid,
    cluster_id: i32,
) -> Result<Vec<(String, f32)>, sqlx::Error> {
    let row = sqlx::query(
        r"
        SELECT top_tokens
        FROM   user_domain_profile
        WHERE  user_id = $1 AND cluster_id = $2 AND tenant_id = $3
        ",
    )
    .bind(user_id.to_string())
    .bind(cluster_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let val: serde_json::Value = r.try_get("top_tokens")?;
        Ok(serde_json::from_value(val).unwrap_or_default())
    } else {
        Ok(vec![])
    }
}

/// Backward-compatible wrapper for loading cluster vocabulary defaulting to 'default' tenant.
///
/// # Errors
/// Returns `sqlx::Error` on query execution failure.
pub async fn load_cluster_vocab_legacy(
    pool: &SqlitePool,
    user_id: Uuid,
    cluster_id: i32,
) -> Result<Vec<(String, f32)>, sqlx::Error> {
    load_cluster_vocab_scoped(pool, "default", user_id, cluster_id).await
}
