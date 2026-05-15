//! Ory Engine v2 — Meta-Orchestrator for learning and dynamic flow design
//!
//! The Ory Engine is the self-evolving intelligence layer of NPAE. It:
//! 1. Performs deep semantic analysis of raw prompts
//! 2. Audits existing engine capabilities against the prompt's needs
//! 3. Dynamically architects new processing flows when gaps are found
//! 4. Remembers successful patterns for reuse
//! 5. Evaluates outcomes to continuously improve

pub mod types;
pub mod semantic;
pub mod learner;
pub mod domain_mapper;
pub mod registry;
pub mod architect;
pub mod memory;
pub mod evaluator;
pub mod math;
pub mod embeddings;

use crate::npae::ory::types::{
    OryResult, LearnedIntent, DynamicBlueprint, AuditRecommendation,
};
use crate::npae::aggressive::config::UnifiedConfig;
use crate::types::ScoringResult;
use anyhow::Result;

/// Ory Engine: The Meta-Orchestrator for learning and dynamic flow design.
pub struct OryEngine {
    memory: memory::PatternMemory,
}

impl OryEngine {
    /// Create a new Ory engine instance with empty memory
    pub fn new() -> Self {
        Self {
            memory: memory::PatternMemory::new(),
        }
    }

    /// Load Ory engine with patterns from database
    pub async fn load_from_db(pool: &sqlx::SqlitePool) -> Result<Self> {
        let memory = memory::PatternMemory::load_from_db(pool).await?;
        Ok(Self { memory })
    }

    /// Persist Ory engine memory to database
    pub async fn save_to_db(&mut self, pool: &sqlx::SqlitePool) -> Result<()> {
        self.memory.persist(pool).await
    }

    /// Check if Ory engine memory needs persistence
    pub fn is_dirty(&self) -> bool {
        self.memory.is_dirty()
    }

    /// Process a raw prompt through the Ory intelligence cycle.
    ///
    /// Returns an `OryResult` containing:
    /// - `LearnedIntent`: Deep semantic analysis
    /// - `FlowAudit`: Assessment of existing capabilities
    /// - `Option<DynamicBlueprint>`: Generated architecture if needed
    pub fn process(&self, raw: &str, config: &UnifiedConfig) -> Result<OryResult> {
        // 1. Deep semantic learning
        let intent = learner::OryLearner::learn(raw)?;

        // 2. Check pattern memory — have we seen this before?
        if let Some(cached) = self.memory.find_match(&intent) {
            let blueprint: Option<DynamicBlueprint> = serde_json::from_str(&cached.blueprint_json).ok();
            let audit = registry::FlowRegistry::audit(&intent, config)?;
            return Ok(OryResult {
                intent,
                audit,
                blueprint,
                from_cache: true,
                cached_pattern_id: Some(cached.pattern_id.clone()),
            });
        }

        // 3. Deep audit against existing flows
        let audit = registry::FlowRegistry::audit(&intent, config)?;

        // 4. Design blueprint if needed
        let blueprint = if audit.recommendation != AuditRecommendation::UseExistingFlow {
            Some(architect::OryArchitect::design(&intent, &audit)?)
        } else {
            None
        };

        Ok(OryResult {
            intent,
            audit,
            blueprint,
            from_cache: false,
            cached_pattern_id: None,
        })
    }

    /// Record outcome after NPAE generates output — closes the learning loop
    pub fn record_outcome(
        &mut self,
        intent: &LearnedIntent,
        blueprint: &DynamicBlueprint,
        scores: &ScoringResult,
    ) -> Result<()> {
        let outcome = evaluator::OutcomeEvaluator::evaluate(intent, blueprint, scores);
        self.memory.record(intent, blueprint, &outcome)?;
        Ok(())
    }

    /// Get the number of learned patterns
    pub fn pattern_count(&self) -> usize {
        self.memory.len()
    }

    /// Prune low-quality patterns from memory
    pub fn prune_memory(&mut self) {
        self.memory.prune();
    }
}

/// Static convenience method for backward compatibility
/// (processes without memory — stateless mode)
pub fn process_stateless(raw: &str, config: &UnifiedConfig) -> Result<OryResult> {
    let engine = OryEngine::new();
    engine.process(raw, config)
}
