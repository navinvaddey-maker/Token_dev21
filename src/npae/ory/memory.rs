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
}

impl PatternMemory {
    /// Create empty in-memory store
    pub fn new() -> Self {
        Self { patterns: HashMap::new() }
    }

    /// Load patterns from SQLite (via JSON column for now)
    pub fn load_from_db(_pool: &sqlx::SqlitePool) -> Self {
        // For initial implementation, start with in-memory
        // SQLite integration will be added when db schema is migrated
        Self::new()
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
        self.patterns.retain(|_, p| {
            !(p.usage_count > 5 && p.success_rate < 0.3)
        });
    }
}
