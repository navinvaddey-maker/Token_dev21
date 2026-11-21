//! Ory Pattern Memory — SQLite-backed persistent learning
//!
//! Stores successful blueprint patterns for reuse. When Ory encounters a prompt
//! with a matching fingerprint, it can retrieve the cached blueprint instead of
//! regenerating from scratch.

use crate::npae::ory::types::{DynamicBlueprint, LearnedIntent, LearnedPattern, PatternOutcome};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

/// Maximum number of in-memory learned patterns before bounded eviction.
///
/// ARCH: Workstream 3 & 1M-user concurrency — prevents unbounded RAM growth by capping pattern count.
pub const MAX_PATTERNS: usize = 10_000;

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
        let rows = sqlx::query_as::<_, LearnedPattern>("SELECT * FROM learned_patterns")
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

    /// Record a blueprint and its outcome into memory.
    ///
    /// Evicts the lowest-usage / oldest pattern when the in-memory cache reaches `MAX_PATTERNS`.
    ///
    /// # Errors
    /// Returns `Err` if serializing the blueprint JSON fails.
    ///
    /// # Panics
    /// Never.
    pub fn record(
        &mut self,
        intent: &LearnedIntent,
        blueprint: &DynamicBlueprint,
        outcome: &PatternOutcome,
    ) -> Result<()> {
        let bp_json = serde_json::to_string(blueprint)?;
        let now = Utc::now();

        // ARCH: Workstream 3 — Bounded capacity enforcement.
        // If at capacity and this is a novel pattern, evict the entry with the lowest usage_count,
        // breaking ties by oldest last_used.
        if self.patterns.len() >= MAX_PATTERNS
            && !self.patterns.contains_key(&intent.intent_fingerprint)
        {
            if let Some(evict_key) = self
                .patterns
                .iter()
                .min_by(|(_, a), (_, b)| {
                    a.usage_count
                        .cmp(&b.usage_count)
                        .then_with(|| a.last_used.cmp(&b.last_used))
                })
                .map(|(k, _)| k.clone())
            {
                self.patterns.remove(&evict_key);
            }
        }

        let entry = self
            .patterns
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
        self.patterns
            .retain(|_, p| !(p.usage_count > 5 && p.success_rate < 0.3));
        if self.patterns.len() < old_len {
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npae::ory::types::{
        BlueprintPhase, BlueprintStrategy, DynamicBlueprint, LearnedIntent, PatternOutcome,
        PhaseGraph, PhaseType, ResourceEstimate, ScopeClass,
    };

    #[tokio::test]
    async fn test_pattern_memory_dirty_and_persistence() -> Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS learned_patterns (
                pattern_id         TEXT PRIMARY KEY,
                domain_fingerprint TEXT NOT NULL,
                intent_fingerprint TEXT NOT NULL,
                blueprint_json     TEXT NOT NULL,
                usage_count        INTEGER NOT NULL,
                success_rate       REAL NOT NULL,
                last_used          TIMESTAMP NOT NULL,
                created_at         TIMESTAMP NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        let mut mem = PatternMemory::new();
        assert!(!mem.is_dirty());

        let intent = LearnedIntent {
            raw_prompt: "Test prompt".into(),
            core_objective: "Build test".into(),
            inferred_domain: "software".into(),
            novel_signals: vec![],
            hidden_dependencies: vec![],
            confidence_score: 0.9,
            audience_level: None,
            temporal_markers: vec![],
            constraint_phrases: vec![],
            meta_instructions: vec![],
            complexity_score: 3.0,
            intent_fingerprint: "test-fp-123".into(),
        };

        let bp = DynamicBlueprint {
            architecture_id: "ORY-TEST".into(),
            strategy: BlueprintStrategy::NovelDesign {
                reasoning: "Test".into(),
            },
            rationale: "Rationale".into(),
            phases: vec![BlueprintPhase {
                name: "Phase 1".into(),
                description: "Desc".into(),
                expected_deliverables: vec!["D1".into()],
                phase_type: PhaseType::Design,
                inputs: vec![],
                outputs: vec![],
                estimated_complexity: 2.0,
            }],
            phase_graph: PhaseGraph {
                edges: vec![],
                parallel_groups: vec![],
            },
            resource_estimate: ResourceEstimate {
                complexity_score: 2.0,
                estimated_token_budget: 100,
                suggested_phases: 1,
                scope_class: ScopeClass::Quick,
            },
            validation_checklist: vec!["Verify 1".into()],
            developer_notes: "Notes".into(),
            reuse_patterns: vec![],
        };

        let outcome = PatternOutcome {
            pattern_id: "pat-1".into(),
            quality_score: 8.5,
            success: true,
            improvement_notes: vec![],
        };

        mem.record(&intent, &bp, &outcome)?;
        assert!(mem.is_dirty());

        // Persist to database
        mem.persist(&pool).await?;
        assert!(!mem.is_dirty());

        // Reload from database
        let loaded = PatternMemory::load_from_db(&pool).await?;
        assert_eq!(loaded.len(), 1);
        let _found = loaded.find_match(&intent);
        assert!(loaded.patterns.contains_key(&intent.intent_fingerprint));

        Ok(())
    }

    #[test]
    fn test_pattern_memory_capacity_bounded_eviction() -> Result<()> {
        let mut mem = PatternMemory::new();

        let bp = DynamicBlueprint {
            architecture_id: "ORY-TEST".into(),
            strategy: BlueprintStrategy::NovelDesign {
                reasoning: "Test".into(),
            },
            rationale: "Rationale".into(),
            phases: vec![],
            phase_graph: PhaseGraph {
                edges: vec![],
                parallel_groups: vec![],
            },
            resource_estimate: ResourceEstimate {
                complexity_score: 1.0,
                estimated_token_budget: 10,
                suggested_phases: 1,
                scope_class: ScopeClass::Quick,
            },
            validation_checklist: vec![],
            developer_notes: "".into(),
            reuse_patterns: vec![],
        };

        let outcome = PatternOutcome {
            pattern_id: "pat-1".into(),
            quality_score: 8.5,
            success: true,
            improvement_notes: vec![],
        };

        // Populate mock patterns up to MAX_PATTERNS
        for i in 0..MAX_PATTERNS {
            let intent = LearnedIntent {
                raw_prompt: format!("Prompt {}", i),
                core_objective: format!("Objective {}", i),
                inferred_domain: "software".into(),
                novel_signals: vec![],
                hidden_dependencies: vec![],
                confidence_score: 0.9,
                audience_level: None,
                temporal_markers: vec![],
                constraint_phrases: vec![],
                meta_instructions: vec![],
                complexity_score: 1.0,
                intent_fingerprint: format!("fp-{}", i),
            };
            mem.record(&intent, &bp, &outcome)?;
        }

        assert_eq!(mem.len(), MAX_PATTERNS);

        // Boost fp-1 usage count
        let intent_1 = LearnedIntent {
            raw_prompt: "Prompt 1".into(),
            core_objective: "Objective 1".into(),
            inferred_domain: "software".into(),
            novel_signals: vec![],
            hidden_dependencies: vec![],
            confidence_score: 0.9,
            audience_level: None,
            temporal_markers: vec![],
            constraint_phrases: vec![],
            meta_instructions: vec![],
            complexity_score: 1.0,
            intent_fingerprint: "fp-1".into(),
        };
        mem.record(&intent_1, &bp, &outcome)?;
        assert_eq!(mem.len(), MAX_PATTERNS);
        assert_eq!(mem.patterns.get("fp-1").unwrap().usage_count, 2);

        // Add pattern exceeding MAX_PATTERNS
        let intent_overflow = LearnedIntent {
            raw_prompt: "Overflow".into(),
            core_objective: "Overflow".into(),
            inferred_domain: "software".into(),
            novel_signals: vec![],
            hidden_dependencies: vec![],
            confidence_score: 0.9,
            audience_level: None,
            temporal_markers: vec![],
            constraint_phrases: vec![],
            meta_instructions: vec![],
            complexity_score: 1.0,
            intent_fingerprint: "fp-overflow".into(),
        };
        mem.record(&intent_overflow, &bp, &outcome)?;

        // Size must still be bounded to MAX_PATTERNS
        assert_eq!(mem.len(), MAX_PATTERNS);
        assert!(mem.patterns.contains_key("fp-overflow"));
        // fp-1 with higher usage count should not have been evicted
        assert!(mem.patterns.contains_key("fp-1"));

        Ok(())
    }
}
