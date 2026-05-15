//! Ory Pattern Memory — SQLite-backed persistent learning
//!
//! Stores successful blueprint patterns for reuse. When Ory encounters a prompt
//! with a matching fingerprint, it can retrieve the cached blueprint instead of
//! regenerating from scratch.

use crate::npae::ory::types::{LearnedIntent, DynamicBlueprint, LearnedPattern, PatternOutcome};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

pub struct PatternMemory {
    patterns: HashMap<String, LearnedPattern>,
    dirty: bool,
}

impl PatternMemory {
    /// Create empty in-memory store
    pub fn new() -> Self {
        Self { 
            patterns: HashMap::new(),
            dirty: false,
        }
    }

    /// Load patterns from SQLite
    pub async fn load_from_db(pool: &sqlx::SqlitePool) -> Result<Self> {
        let rows = sqlx::query_as::<_, LearnedPattern>(
            "SELECT * FROM learned_patterns"
        )
        .fetch_all(pool)
        .await?;

        let mut patterns = HashMap::new();
        for p in rows {
            patterns.insert(p.intent_fingerprint.clone(), p);
        }

        Ok(Self {
            patterns,
            dirty: false,
        })
    }

    /// Persist patterns to SQLite if dirty
    pub async fn persist(&mut self, pool: &sqlx::SqlitePool) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        for pattern in self.patterns.values() {
            sqlx::query(
                r#"
                INSERT INTO learned_patterns 
                (pattern_id, domain_fingerprint, intent_fingerprint, blueprint_json, usage_count, success_rate, last_used, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT(pattern_id) DO UPDATE SET
                    usage_count = EXCLUDED.usage_count,
                    success_rate = EXCLUDED.success_rate,
                    last_used = EXCLUDED.last_used,
                    blueprint_json = EXCLUDED.blueprint_json
                "#
            )
            .bind(&pattern.pattern_id)
            .bind(&pattern.domain_fingerprint)
            .bind(&pattern.intent_fingerprint)
            .bind(&pattern.blueprint_json)
            .bind(pattern.usage_count as i64)
            .bind(pattern.success_rate as f64)
            .bind(pattern.last_used)
            .bind(pattern.created_at)
            .execute(pool)
            .await?;
        }

        self.dirty = false;
        Ok(())
    }

    /// Check if memory needs persistence
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Find a matching pattern by intent fingerprint
    pub fn find_match(&self, intent: &LearnedIntent) -> Option<&LearnedPattern> {
        // Primary: exact fingerprint match
        if let Some(pattern) = self.patterns.get(&intent.intent_fingerprint) {
            if pattern.success_rate >= 0.6 && pattern.usage_count >= 2 {
                return Some(pattern);
            }
        }

        // Secondary: domain + objective similarity match
        for pattern in self.patterns.values() {
            if pattern.domain_fingerprint == intent.inferred_domain
                && pattern.success_rate >= 0.7
                && pattern.usage_count >= 3
            {
                return Some(pattern);
            }
        }

        None
    }

    /// Record a blueprint and its outcome into memory
    pub fn record(
        &mut self,
        intent: &LearnedIntent,
        blueprint: &DynamicBlueprint,
        outcome: &PatternOutcome,
    ) -> Result<()> {
        let bp_json = serde_json::to_string(blueprint)?;
        let now = Utc::now();

        let entry = self.patterns
            .entry(intent.intent_fingerprint.clone())
            .or_insert_with(|| LearnedPattern {
                pattern_id: format!("pat-{}", uuid::Uuid::new_v4()),
                domain_fingerprint: intent.inferred_domain.clone(),
                intent_fingerprint: intent.intent_fingerprint.clone(),
                blueprint_json: bp_json.clone(),
                usage_count: 0,
                success_rate: 0.0,
                last_used: now,
                created_at: now,
            });

        entry.usage_count += 1;
        entry.last_used = now;
        entry.blueprint_json = bp_json;

        // Rolling average of success rate
        let n = entry.usage_count as f32;
        let success_val = if outcome.success { 1.0 } else { 0.0 };
        entry.success_rate = ((entry.success_rate * (n - 1.0)) + success_val) / n;

        self.dirty = true;
        Ok(())
    }

    /// Get all patterns (for persistence)
    pub fn all_patterns(&self) -> &HashMap<String, LearnedPattern> {
        &self.patterns
    }

    /// Number of stored patterns
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Prune low-quality patterns (success_rate < 0.3 and usage > 5)
    pub fn prune(&mut self) {
        let old_len = self.patterns.len();
        self.patterns.retain(|_, p| {
            !(p.usage_count > 5 && p.success_rate < 0.3)
        });
        if self.patterns.len() < old_len {
            self.dirty = true;
        }
    }
}
