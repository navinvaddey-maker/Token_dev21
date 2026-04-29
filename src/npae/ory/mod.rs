pub mod types;
pub mod learner;
pub mod registry;
pub mod architect;

use crate::npae::ory::types::{DynamicBlueprint, LearnedIntent, FlowAudit, AuditRecommendation};
use crate::npae::aggressive::config::UnifiedConfig;
use anyhow::Result;

/// Ory Engine: The Meta-Orchestrator for learning and dynamic flow design.
pub struct OryEngine;

impl OryEngine {
    /// Process a raw prompt through the Ory learning cycle.
    /// 
    /// Returns:
    /// - `LearnedIntent`: The deep semantic analysis of the prompt.
    /// - `FlowAudit`: Assessment of existing engine capabilities for this intent.
    /// - `Option<DynamicBlueprint>`: A generated architecture blueprint if a custom flow is needed.
    pub fn process(raw: &str, config: &UnifiedConfig) -> Result<(LearnedIntent, FlowAudit, Option<DynamicBlueprint>)> {
        // 1. Deep Learning: Extract intent, signals, and hidden dependencies
        let intent = learner::OryLearner::learn(raw)?;
        
        // 2. Audit: Check if existing flows (in unified.json) can handle this
        let audit = registry::FlowRegistry::audit(&intent, config)?;
        
        // 3. Design: If the audit reveals gaps or novel domains, architect a new flow
        let mut blueprint = None;
        if audit.recommendation != AuditRecommendation::UseExistingFlow {
            blueprint = Some(architect::OryArchitect::design(&intent, &audit)?);
        }
        
        Ok((intent, audit, blueprint))
    }
}
