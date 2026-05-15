use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

// ============================================================================
// Phase 1: Semantic Learning Types
// ============================================================================

/// Classified token from NLP-lite analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedToken {
    pub text: String,
    pub token_class: TokenClass,
    pub confidence: f32,
}

/// Classification categories for prompt tokens
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TokenClass {
    Action,      // verbs: build, explain, debug, create
    Subject,     // nouns: system, plan, architecture
    Constraint,  // qualifiers: within 2 weeks, under $10k, no dependencies
    Modifier,    // adjectives/adverbs: scalable, fast, comprehensive
    Temporal,    // time signals: weekly, by Q3, in 6 months
    Audience,    // audience markers: for beginners, senior engineers
    Meta,        // meta-instructions: first principles, contrarian, unconventional
    Domain,      // domain markers: medical, legal, financial
    Noise,       // stop words, filler
}

/// Result of semantic tokenization and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAnalysis {
    pub tokens: Vec<ClassifiedToken>,
    pub clusters: HashMap<TokenClass, Vec<String>>,
    pub coverage_ratio: f32,   // % of tokens successfully classified (non-noise)
    pub action_objects: Vec<ActionObject>,  // verb-noun pairs as intent cores
    pub complexity_score: f32, // 1.0-10.0 request complexity
}

/// An action-object pair extracted from the prompt (verb + noun target)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionObject {
    pub action: String,     // "build", "explain", "debug"
    pub object: String,     // "system", "architecture", "bug"
    pub confidence: f32,
}

// ============================================================================
// Core Ory Types (Enhanced from v1)
// ============================================================================

/// Deep semantic analysis of a user prompt — the Ory learner's output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnedIntent {
    pub raw_prompt: String,
    pub core_objective: String,
    pub inferred_domain: String,
    pub novel_signals: Vec<String>,
    pub hidden_dependencies: Vec<String>,
    pub confidence_score: f32,
    // v2 additions
    pub audience_level: Option<String>,
    pub temporal_markers: Vec<String>,
    pub constraint_phrases: Vec<String>,
    pub meta_instructions: Vec<String>,
    pub complexity_score: f32,
    pub intent_fingerprint: String,  // hash for pattern matching
}

// ============================================================================
// Phase 2: Deep Audit Types
// ============================================================================

/// Comprehensive flow audit — multi-level assessment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowAudit {
    pub existing_domain_match: Option<String>,
    pub existing_template_match: Option<String>,
    pub coverage_score: f32,
    pub gaps_identified: Vec<String>,
    pub recommendation: AuditRecommendation,
    // v2 additions
    pub domain_audit: DomainAuditResult,
    pub template_audit: TemplateAuditResult,
    pub constraint_audit: ConstraintAuditResult,
    pub gap_details: Vec<AuditGapDetail>,
}

/// Result of domain-level auditing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainAuditResult {
    pub matched_domain: Option<String>,    // domain from unified.json
    pub match_method: DomainMatchMethod,
    pub match_confidence: f32,             // 0.0-1.0
}

/// How the domain was matched
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainMatchMethod {
    ExactMatch,          // direct name match
    SynonymMatch,        // via domain_mapper synonym table
    HierarchyMatch,      // parent/child domain relationship
    FuzzyMatch,          // edit-distance based
    NoMatch,
}

/// Result of template-level auditing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateAuditResult {
    pub has_execution_phases: bool,
    pub has_validation_steps: bool,
    pub has_success_criteria: bool,
    pub has_role_template: bool,
    pub has_constraint_rules: bool,
    pub template_coverage: f32,     // 0.0-1.0
}

/// Result of constraint auditing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConstraintAuditResult {
    pub total_constraints_detected: u32,
    pub constraints_covered: u32,
    pub uncovered_constraints: Vec<String>,
    pub constraint_coverage: f32,   // 0.0-1.0
}

/// Specific gap found during audit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditGapDetail {
    pub gap_type: GapType,
    pub severity: GapSeverity,
    pub description: String,
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GapType {
    DomainMissing,
    TemplateMissing,
    ConstraintUnsupported,
    NovelSignalUnhandled,
    AudienceMismatch,
    ComplexityBeyondTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GapSeverity {
    Critical,   // blueprint MUST be generated
    Major,      // augmentation strongly recommended
    Minor,      // existing flow works, with caveats
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditRecommendation {
    UseExistingFlow,
    AugmentExistingFlow,
    BuildDynamicFlow,
}

// ============================================================================
// Phase 3: Adaptive Blueprint Types
// ============================================================================

/// Dynamic flow architecture — the Ory architect's output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamicBlueprint {
    pub architecture_id: String,
    pub strategy: BlueprintStrategy,
    pub rationale: String,
    pub phases: Vec<BlueprintPhase>,
    pub phase_graph: PhaseGraph,
    pub resource_estimate: ResourceEstimate,
    pub validation_checklist: Vec<String>,
    pub developer_notes: String,
    pub reuse_patterns: Vec<String>,  // pattern IDs from memory
}

/// Strategy used to generate the blueprint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlueprintStrategy {
    TemplateExtension { base_template: String, additions: Vec<String> },
    DomainTransfer { source_domain: String, adaptations: Vec<String> },
    CompositeDesign { sources: Vec<String> },
    NovelDesign { reasoning: String },
}

/// A phase in the dynamic blueprint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlueprintPhase {
    pub name: String,
    pub description: String,
    pub expected_deliverables: Vec<String>,
    // v2 additions
    pub phase_type: PhaseType,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub estimated_complexity: f32,  // 1.0-10.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PhaseType {
    Analysis,
    Design,
    Implementation,
    Validation,
    Synthesis,
    Custom(String),
}

/// Dependency graph between phases
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhaseGraph {
    pub edges: Vec<(usize, usize)>,           // (prerequisite, dependent)
    pub parallel_groups: Vec<Vec<usize>>,      // phases that can run in parallel
}

/// Resource and complexity estimation for a blueprint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceEstimate {
    pub complexity_score: f32,       // 1.0-10.0
    pub estimated_token_budget: u32,
    pub suggested_phases: u8,
    pub scope_class: ScopeClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScopeClass {
    Quick,       // < 5 min, simple response
    Standard,    // 5-30 min, structured response
    Deep,        // 30+ min, multi-phase with validation
    Research,    // open-ended, iterative
}

// ============================================================================
// Phase 4: Pattern Memory Types (SQLite-backed)
// ============================================================================

/// A learned pattern stored in memory for reuse
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LearnedPattern {
    pub pattern_id: String,
    pub domain_fingerprint: String,
    pub intent_fingerprint: String,
    pub blueprint_json: String,    // serialized DynamicBlueprint
    pub usage_count: u32,
    pub success_rate: f32,
    pub last_used: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Outcome of evaluating a blueprint's effectiveness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternOutcome {
    pub pattern_id: String,
    pub quality_score: f32,       // 0.0-10.0 (average of TES, SFS, SCS)
    pub success: bool,            // quality_score >= 6.0
    pub improvement_notes: Vec<String>,
}

// ============================================================================
// Unified Return Type
// ============================================================================

/// The complete result of Ory processing — returned to the Aggressive engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OryResult {
    pub intent: LearnedIntent,
    pub audit: FlowAudit,
    pub blueprint: Option<DynamicBlueprint>,
    pub from_cache: bool,           // true if pattern was reused from memory
    pub cached_pattern_id: Option<String>,
}

impl OryResult {
    pub fn has_blueprint(&self) -> bool {
        self.blueprint.is_some()
    }

    pub fn needs_custom_flow(&self) -> bool {
        self.audit.recommendation != AuditRecommendation::UseExistingFlow
    }
}
