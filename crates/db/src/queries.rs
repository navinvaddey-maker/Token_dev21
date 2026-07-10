use competitive_core::centroid::CentroidSnapshot;
use hebbian_core::WeightDelta;
use schema_engine::types::PromptSchema;
use sqlx::{SqlitePool, Row};
use uuid::Uuid;

pub async fn save_prompt(
    pool: &SqlitePool,
    user_id: Uuid,
    raw: &str,
    schema: &PromptSchema,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let schema_json = serde_json::to_value(schema).unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO user_prompts (id, user_id, raw_text, schema_json, token_count)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(raw)
    .bind(schema_json)
    .bind(schema.token_count as i32)
    .execute(pool)
    .await?;

    Ok(id)
}

pub async fn upsert_learned_context(
    pool: &SqlitePool,
    user_id: Uuid,
    deltas: &[WeightDelta],
) -> Result<(), sqlx::Error> {
    let pairs = serde_json::to_value(deltas).unwrap_or_default();

    sqlx::query(
        r#"
        INSERT INTO learned_context (id, user_id, top_pairs)
        VALUES (lower(hex(randomblob(16))), $1, $2)
        ON CONFLICT (user_id) DO UPDATE SET
            top_pairs  = EXCLUDED.top_pairs,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(user_id.to_string())
    .bind(pairs)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn upsert_domain_profile(
    pool: &SqlitePool,
    user_id: Uuid,
    snapshots: &[CentroidSnapshot],
) -> Result<(), sqlx::Error> {
    for snap in snapshots {
        let top_tokens = serde_json::to_value(&snap.top_tokens).unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO user_domain_profile
                (id, user_id, cluster_id, cluster_label, top_tokens, hits)
            VALUES (lower(hex(randomblob(16))), $1, $2, $3, $4, $5)
            ON CONFLICT (user_id, cluster_id) DO UPDATE SET
                cluster_label = EXCLUDED.cluster_label,
                top_tokens    = EXCLUDED.top_tokens,
                hits          = EXCLUDED.hits,
                updated_at    = CURRENT_TIMESTAMP
            "#,
        )
        .bind(user_id.to_string())
        .bind(snap.id as i32)
        .bind(&snap.label)
        .bind(top_tokens)
        .bind(snap.hits as i32)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn load_cluster_vocab(
    pool: &SqlitePool,
    user_id: Uuid,
    cluster_id: i32,
) -> Result<Vec<(String, f32)>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT top_tokens
        FROM   user_domain_profile
        WHERE  user_id = $1 AND cluster_id = $2
        "#,
    )
    .bind(user_id.to_string())
    .bind(cluster_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let val: serde_json::Value = r.try_get("top_tokens")?;
        Ok(serde_json::from_value(val).unwrap_or_default())
    } else {
        Ok(vec![])
    }
}
