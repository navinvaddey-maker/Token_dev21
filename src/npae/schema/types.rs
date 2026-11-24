use crate::npae::compression::types::CompressedRepr;
use serde::{Deserialize, Serialize};

/// Targeted repair context supplied when retrying or correcting failed/missing verification items.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct RepairContext {
    /// Claims or verification assertions that failed grounding/constraint checks.
    #[serde(default)]
    pub failed_items: Vec<String>,
    /// Requested items missing from the candidate response.
    #[serde(default)]
    pub missing_items: Vec<String>,
    /// Additional diagnostic details explaining why repair is needed.
    #[serde(default)]
    pub details: Vec<String>,
}

/// Request payload for `POST /api/v1/aggressive`.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AggressiveRequest {
    pub prompt: String,
    pub config: Option<NpaeConfig>,
    pub rag_enabled: Option<bool>,
    pub rag_document_ids: Option<Vec<String>>,
    pub rag_top_k: Option<usize>,
    /// Optional parent ledger ID linking this turn to an earlier prompt state.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Optional constraint IDs (`"C1"`) or target terms to revoke on this turn.
    #[serde(default)]
    pub revoked_constraints: Option<Vec<String>>,
    /// Optional 1-based execution phase number to focus on this turn.
    #[serde(default)]
    pub active_phase: Option<u32>,
    /// Optional slot name -> value overrides resolving open ambiguity gaps.
    #[serde(default)]
    pub resolved_slots: Option<std::collections::BTreeMap<String, String>>,
    /// Optional verification-guided repair context for targeted repair synthesis.
    #[serde(default)]
    pub repair_context: Option<RepairContext>,
}

/// Controls how [`StructuredPrompt`] is rendered into `optimized_prompt` in Aggressive mode.
///
/// // ARCH: Grounding Harness (GAP-42) — opt-in harness rendering keeps `Crisp` as the default.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RenderStyle {
    /// Compact markdown sections (`# ROLE`, `# CONTEXT`, `# CONSTRAINTS`, …). Default.
    #[default]
    #[serde(alias = "Crisp", alias = "CRISP")]
    Crisp,
    /// XML-fenced grounding harness with numbered `[F#]` facts inside `<facts trust="untrusted">`
    /// and a strict `FOUND` / `PARTIAL_MATCH` / `NOT_FOUND` output contract.
    #[serde(alias = "Harness", alias = "HARNESS")]
    Harness,
    /// Stateful multi-turn continuation rendering mode (Phase 1 routes to Crisp baseline until
    /// Phase 2 delta renderer is attached).
    #[serde(alias = "cached_multi_turn", alias = "CachedMultiTurn")]
    CachedMultiTurn,
}

/// Target downstream LLM family for adaptive prompt formatting in Aggressive Crisp mode.
///
/// // ARCH: Phase C — Adapts rendering structure to model attention/instruction-following behavior
/// // (`StandardMarkdown` default, `ClaudeXml` for XML-native models, `ReasoningMinimal` for
/// // native chain-of-thought reasoning models).
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TargetModelFamily {
    /// High-Signal CRISP Markdown (default; works well across GPT-4o, Gemini, and general LLMs).
    #[default]
    #[serde(
        alias = "standard",
        alias = "markdown",
        alias = "gpt",
        alias = "gemini"
    )]
    StandardMarkdown,
    /// XML-fenced sections (`<role>`, `<task>`, `<context>`, `<constraints>`, `<output_blueprint>`)
    /// tuned for Claude and XML-native frontier models.
    #[serde(alias = "claude", alias = "xml")]
    ClaudeXml,
    /// Lean Goal + Hard Constraints + Evaluation Rubric + Output Blueprint without rigid
    /// waterfall phases, tuned for reasoning models (o1/o3/o4, Gemini Thinking, DeepSeek-R1).
    #[serde(alias = "reasoning", alias = "o1", alias = "o3", alias = "r1")]
    ReasoningMinimal,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct NpaeConfig {
    pub ambiguity_threshold: Option<f32>,
    pub max_questions: Option<u8>,
    pub confidence_threshold: Option<f32>,
    pub skip_stage: Option<String>,
    /// Optional rendering mode (`crisp` default, or `harness` for strict `[F#]` fact grounding).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_style: Option<RenderStyle>,
    /// Optional target LLM family for adaptive Crisp rendering (`standard_markdown` default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_model_family: Option<TargetModelFamily>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructuredPromptResponse {
    pub schema_version: String,
    pub request_id: String,
    pub optimized_prompt: String, // Added for UI compatibility (app.js)
    pub token_original: u32,      // Added for UI compatibility
    pub token_final: u32,         // Added for UI compatibility
    pub token_saved: u32,         // Added for UI compatibility
    pub compression: CompressionMeta,
    pub structured_prompt: StructuredPrompt,
    pub ambiguity_analysis: AmbiguityAnalysis,
    pub clarifying_questions: Vec<ClarifyingQuestion>,
    pub confidence_score: f32,
    pub processing_metadata: ProcessingMeta,
    pub scoring_result: crate::types::ScoringResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hallucination_report: Option<crate::npae::hallucination::guard::HallucinationReport>,
    /// Numbered `[F#]` facts emitted when `render_style == RenderStyle::Harness` (omitted when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_facts: Vec<crate::npae::grounding::verifier::NumberedFact>,
    /// Optional stateful continuation metadata emitted for multi-turn parent chains.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ContinuationMeta>,
}

/// Serialized summary view of a tracked constraint inside [`ContinuationMeta`].
///
/// # Invariants
/// `id` matches the stable `C#` identifier assigned by [`crate::npae::continuation::ledger::PromptStateLedger`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConstraintView {
    /// Stable constraint identifier (for example `"C1"`).
    pub id: String,
    /// Constraint category (`"inclusion"`, `"exclusion"`, `"format"`, `"length_bound"`, `"tone"`).
    pub kind: String,
    /// Human-readable constraint statement.
    pub text: String,
    /// Lifecycle status (`"active"`, `"superseded"`, `"revoked"`).
    pub status: String,
}

/// Serialized summary view of an execution phase inside [`ContinuationMeta`].
///
/// # Invariants
/// `phase_number` is 1-based.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PhaseView {
    /// 1-based execution phase number.
    pub phase_number: u32,
    /// Concise phase title.
    pub name: String,
    /// Lifecycle status (`"pending"`, `"active"`, `"done"`, `"deferred"`).
    pub status: String,
}

/// Stateful multi-turn continuation metadata returned alongside [`StructuredPromptResponse`].
///
/// // ARCH: ARCHITECTURE.md §6.7 — surfaces parent-delta path, continuation route, active and
/// // superseded constraints, phase cursor, and remaining open slots.
///
/// # Invariants
/// `turn_number >= 1` for initialized ledger responses.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ContinuationMeta {
    /// Persistent ledger identifier for this prompt branch.
    pub ledger_id: String,
    /// 1-based turn index within the ledger.
    pub turn_number: u32,
    /// Continuation path selected by [`crate::npae::continuation::diff::ParentMatcher`].
    pub path: crate::npae::continuation::diff::ContinuationPath,
    /// Highest-priority continuation intent selected by [`crate::npae::continuation::router::ContinuationRouter`].
    pub primary_kind: crate::npae::continuation::router::ContinuationKind,
    /// All continuation intents detected on this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_kinds: Vec<crate::npae::continuation::router::ContinuationKind>,
    /// Concise single-line summary of the delta applied on this turn.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub delta_summary: String,
    /// Currently active constraints enforced by the ledger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_constraints: Vec<ConstraintView>,
    /// Constraint texts superseded on this or prior turns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_constraints: Vec<String>,
    /// Execution phases tracked by the ledger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<PhaseView>,
    /// Active phase number, if any phase is currently active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_phase: Option<u32>,
    /// Remaining unresolved ambiguity slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_slots: Vec<String>,
    /// Compact turn-delta prompt emitted alongside the full rehydrated prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_prompt: Option<String>,
    /// Slot values resolved on this or prior turns.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub resolved_slots: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompressionMeta {
    pub original_tokens: u32,
    pub compressed_tokens: u32,
    pub compression_ratio: f32,
    pub stage_metrics: crate::npae::compression::types::StageMetrics,
}

impl CompressionMeta {
    pub fn from_repr(repr: &CompressedRepr, original_token_count: u32) -> Self {
        Self {
            original_tokens: original_token_count,
            compressed_tokens: repr.token_ids.len() as u32,
            compression_ratio: repr.compression_ratio,
            stage_metrics: repr.stage_metrics.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructuredPrompt {
    pub role: PromptRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<PromptObjective>,
    pub context: PromptContext,
    pub constraints: PromptConstraints,
    pub hallucination_guard: HallucinationGuardConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_spec: Option<OutputSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_phases: Vec<ExecutionPhase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_steps: Vec<ValidationStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub success_criteria: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_meta: Option<ConstraintsMeta>,
    #[serde(default)]
    pub dynamic_instruction: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptRole {
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    pub expertise_domains: Vec<String>,
    pub persona_constraints: Vec<String>,
    #[serde(default)]
    pub persona_anchor: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptContext {
    pub domain: String,
    pub description: String,
    pub background: String,
    pub user_knowledge_level: String,
    #[serde(default)]
    pub audience: String,
    pub temporal_scope: String,
    pub intent_vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptConstraints {
    pub output_format: String,
    pub length_bound: LengthBound,
    pub forbidden_topics: Vec<String>,
    pub required_inclusions: Vec<String>,
    pub tone: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LengthBound {
    pub min: u32,
    pub max: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HallucinationGuardConfig {
    pub self_critique_enabled: bool,
    pub confidence_threshold: f32,
    pub contradiction_check: bool,
    pub claim_verification_rules: Vec<String>,
    pub uncertainty_markers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AmbiguityAnalysis {
    pub score: f32,
    pub threshold: f32,
    pub triggered: bool,
    pub gap_zones: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClarifyingQuestion {
    pub id: u8,
    pub question: String,
    pub information_gain: f32,
    pub gap_addressed: String,
    pub priority: Priority,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    Medium,
    Low,
}

// --- 6-Pillar Prompt Spec Types ---

/// Pillar 2: Objective — one primary task only
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptObjective {
    pub primary_task: String,
    pub deliverable_type: DeliverableType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_boundary: Vec<String>,
}

/// Classifies what the user wants delivered — prevents intent mixing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableType {
    Content,  // email, article, code, essay — needs Requirements, NOT roadmaps
    Strategy, // business plan, roadmap, analysis — needs Execution Phases
    Hybrid,   // comprehensive guide + implementation — needs both
    Artifact, // code, config, schema — needs Output Format + Validation
    Analysis, // report, comparison, review — needs Requirements
}

impl Default for DeliverableType {
    fn default() -> Self {
        DeliverableType::Content
    }
}

/// Pillar 5: Output Format — structured specification
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputSpec {
    pub format: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    pub word_count: LengthBound,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mandatory_elements: Vec<String>,
}

// --- Execution Roadmap Types ---

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutionPhase {
    pub phase_number: u8,
    pub name: String,
    pub description: String,
    pub estimated_duration: String,
    pub deliverables: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValidationStep {
    pub step_number: u8,
    pub name: String,
    pub criteria: String,
    pub validation_method: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConstraintsMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geography: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_tolerance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revenue_expectations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_composition: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessingMeta {
    pub pipeline_duration_ms: u64,
    pub aggressive_mode_ms: u64,
    pub model_version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HallucinationCheckRequest {
    pub output: String,
    pub original_prompt: String,
    pub guard_config: Option<HallucinationGuardConfig>,
}
