use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnedIntent {
    pub raw_prompt: String,
    pub core_objective: String,
    pub inferred_domain: String,
    pub novel_signals: Vec<String>,
    pub hidden_dependencies: Vec<String>,
    pub confidence_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowAudit {
    pub existing_domain_match: Option<String>,
    pub existing_template_match: Option<String>,
    pub coverage_score: f32,
    pub gaps_identified: Vec<String>,
    pub recommendation: AuditRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditRecommendation {
    UseExistingFlow,
    AugmentExistingFlow,
    BuildDynamicFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicBlueprint {
    pub architecture_id: String,
    pub rationale: String,
    pub phases: Vec<BlueprintPhase>,
    pub validation_checklist: Vec<String>,
    pub developer_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlueprintPhase {
    pub name: String,
    pub description: String,
    pub expected_deliverables: Vec<String>,
}
