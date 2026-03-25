use sqlx::SqlitePool;
use tracing::info;

/// Adjust per-user entity weights based on detected signals.
/// Negative signals (repetition, forgot, fast_reprompt) → raise weight (protect entity)
/// Positive signals (long_engagement, thumbs_up) → lower weight (safe to compress)
pub async fn adjust(
    pool:        &SqlitePool,
    user_id:     &str,
    use_case:    &str,
    entities:    &[String],
    signal_type: &str,
) -> Result<(), sqlx::Error> {
    let delta: f64 = match signal_type {
        "repetition"      => 0.15,
        "forgot"          => 0.20,
        "missing_entity"  => 0.25,
        "fast_reprompt"   => 0.10,
        "long_engagement" => -0.05,
        "thumbs_up"       => -0.10,
        "thumbs_down"     => 0.20,
        _                 => 0.0,
    };

    if delta == 0.0 { return Ok(()); }

    for entity in entities {
        // Use MIN(MAX(...)) instead of CLAMP (not available in SQLite)
        sqlx::query(
            r#"INSERT INTO compression_weights (id, user_id, entity, weight, use_case)
               VALUES (lower(hex(randomblob(16))), ?1, ?2, MIN(MAX(1.0 + ?3, 0.1), 3.0), ?4)
               ON CONFLICT(user_id, entity, use_case)
               DO UPDATE SET
                   weight     = MIN(MAX(weight + ?3, 0.1), 3.0),
                   updated_at = datetime('now')"#
        )
        .bind(user_id)
        .bind(entity)
        .bind(delta)
        .bind(use_case)
        .execute(pool)
        .await?;
        info!(user_id = user_id, entity = entity.as_str(), delta = delta, "weight.adjusted");
    }
    Ok(())
}

/// Fetch protected entities for a user — weight > 1.2 = protect
pub async fn get_protected_entities(
    pool:     &SqlitePool,
    user_id:  &str,
    use_case: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT entity FROM compression_weights WHERE user_id = ?1 AND use_case = ?2 AND weight >= 1.2"
    )
    .bind(user_id)
    .bind(use_case)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
