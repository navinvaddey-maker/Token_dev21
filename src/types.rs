use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReconstructedInput {
    pub clusters: HashMap<SlotType, Vec<WeightedToken>>,
    pub constraint_locks: Vec<ConstraintToken>,
    pub ambiguity_register: Vec<AmbiguityFlag>,
    pub input_structure_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SlotType {
    Role,
    Context,
    Task,
    Constraint,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressionSchema {
    pub role: Option<String>,
    pub context: Option<String>,
    pub task: Option<String>,
    pub constraints: Vec<Constraint>,
    pub output: Vec<Deliverable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedToken {
    pub text: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintToken {
    pub text: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbiguityFlag {
    pub text: String,
    pub reason: String,
    pub resolved_as: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub name: String,
}

/// Every token carries its text and salience score through the pipeline.
/// Zero-copy: we store indices into the original prompt string, not clones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredToken {
    pub text: String,
    pub salience: f32, // 0.0 – 1.0
    pub source: TokenSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TokenSource {
    Lexical, // survived lexical compression
    Sparse,  // survived sparse coding filter
    Delta,   // novel — not in schema priors (from predictive coding)
    Cluster, // a cluster label loaded into WM
}

/// Mode decision from Predictive Coding.
/// This is the boundary signal that routes the entire pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Mode {
    Gentle,
    Balanced,
    Aggressive,
    Ambiguous, // 0.45–0.55 hysteresis band — resolved by session depth
}

/// Working memory slot — stack allocated, fixed capacity.
/// Gentle = 3 slots, Aggressive = 7 slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WmSlot {
    pub content: String,
    pub salience: f32,
    pub source: SlotSource,
    pub is_protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlotSource {
    Task,
    Deliverable,
    Context,
    Cluster,
    Delta,
}

/// Maximum WM capacities — compile-time constants.
/// ArrayVec enforces these at the type level; no runtime bounds check needed.
pub const GENTLE_WM_CAPACITY: usize = 3;
pub const AGGRESSIVE_WM_CAPACITY: usize = 7;

pub type GentleWm = ArrayVec<WmSlot, GENTLE_WM_CAPACITY>;
pub type AggressiveWm = ArrayVec<WmSlot, AGGRESSIVE_WM_CAPACITY>;

/// The shared output struct written to by each stage.
/// Passed by &mut through the pipeline — no cloning between stages.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AlgorithmOutput {
    // Stage -1 — Token Reconstruction
    pub constraint_locks: Vec<ConstraintToken>,
    pub ambiguity_register: Vec<AmbiguityFlag>,
    pub input_structure_score: f32,

    // Stage 1 — Signal Reduction
    pub clean_tokens: Vec<String>,
    pub compression_ratio: f32,
    pub salience_map: HashMap<String, f32>,
    pub sparse_tokens: Vec<ScoredToken>,
    pub input_token_count: u32,
    pub output_token_count: u32,

    // Stage 2 — Boundary
    pub error_score: f32,
    pub delta_tokens: Vec<String>,
    pub mode: Option<Mode>,
    pub is_ambiguous: bool,

    // Stage 3 — Context
    pub clusters: Vec<Vec<String>>,
    pub cluster_labels: Vec<String>,
    pub wm_slots: Vec<WmSlot>,
    pub wm_utilisation: f32,
    pub fidelity_estimate: f32,

    // Stage 4 — Schema
    pub resolved_schema: CompressionSchema,
    pub null_fields: Vec<String>,
    pub task_inferred: bool,
    pub deliverable_inferred: bool,

    // New fields from refinements guide
    pub topology: Option<PromptTopology>,
    pub normalization: Option<NormalizationResult>,
    pub ordinal_sequence: Option<OrdinalSequence>,
    pub field_issues: Vec<FieldValidationIssue>,
    pub scope_injections: Vec<String>,
    pub scoring_result: Option<ScoringResult>,
    pub correction_cycle: Option<CorrectionCycle>,
}

/// Final packaged response sent to the caller / UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompressionResponse {
    pub mode: String,
    pub error_score: f32,
    pub fidelity: f32,
    pub schema: CompressionSchema,
    pub wm_slots_used: usize,
    pub null_fields: Vec<String>,
    pub response: String,
    pub normalization: NormalizationResult,
    pub field_issues: Vec<FieldValidationIssue>,
    pub topology: PromptTopology,
    pub scope_injections: Vec<String>,
    // Aggressive only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clusters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_tokens: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_chain: Option<Vec<String>>,
    pub scoring_result: ScoringResult,
    pub correction_cycle: CorrectionCycle,
}

/// Topology of the prompt structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PromptTopology {
    Linear,
    Hierarchical,
    Network,
    Flat,
}

impl Default for PromptTopology {
    fn default() -> Self {
        PromptTopology::Linear
    }
}

/// Types of degeneracy in token representations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DegeneracyType {
    None,
    Exact,
    Partial,
    High,
}

/// Result of normalization process.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NormalizationResult {
    pub normalized_text: String,
    pub normalization_score: f32,
    pub applied_rules: Vec<String>,
}

/// Correction applied to text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextCorrection {
    pub original: String,
    pub corrected: String,
    pub confidence: f32,
    pub correction_type: String,
}

/// Repair action for a field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldRepair {
    pub field_name: String,
    pub repair_type: String,
    pub confidence: f32,
    /// Optional suggested value for the repair.
    pub suggested_value: Option<String>,
}

/// Sequence of ordinal positions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrdinalSequence {
    pub sequence: Vec<u32>,
    pub score: f32,
}

/// Semantic type of field content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldContentType {
    Text,
    Number,
    Date,
    Identifier,
    Code,
    Unknown,
}

/// Issue detected during field validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValidationIssue {
    pub field_name: String,
    pub issue_type: String,
    pub description: String,
    pub severity: String, // e.g., "error", "warning", "info"
}

/// ScoringResult: TES + SFS + SCS
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoringResult {
    pub tes: f32,  // 0.0–10.0
    pub sfs: f32,  // 0.0–10.0
    pub scs: f32,  // 0.0–10.0 (NEW)
    pub correction_needed: bool,
    pub correction_axis: Option<ScoreAxis>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScoreAxis {
    TaskEssential,
    SchemaFidelity,
    SemanticCompleteness,
}

/// Issue encountered during scoring process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScoringIssue {
    LowConfidence,
    Mismatch,
    MissingData,
    Timeout,
}

/// Record of a correction cycle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CorrectionCycle {
    pub cycle_number: u32,
    pub corrections_applied: Vec<TextCorrection>,
    pub improvement: f32,
}

impl CorrectionCycle {
    /// Create a new empty correction cycle.
    pub fn new(cycle_number: u32) -> Self {
        Self {
            cycle_number,
            corrections_applied: Vec::new(),
            improvement: 0.0,
        }
    }

    /// Create a new correction cycle based on analysis of compressed output and field issues.
    pub fn new_cycle(
        &self,
        compressed_output: &str,
        field_issues: &[FieldValidationIssue],
        scoring_result: &ScoringResult,
    ) -> Self {
        let corrections = Self::analyze_corrections(compressed_output, field_issues);
        let improvement = Self::calculate_improvement(scoring_result);

        Self {
            cycle_number: self.cycle_number + 1,
            corrections_applied: corrections,
            improvement,
        }
    }

    /// Analyze the compressed output and field issues to determine needed corrections.
    fn analyze_corrections(
        _compressed_output: &str,
        field_issues: &[FieldValidationIssue],
    ) -> Vec<TextCorrection> {
        let mut corrections = Vec::new();

        for issue in field_issues {
            if issue.severity == "error" {
                corrections.push(TextCorrection {
                    original: "".to_string(),  // Placeholder, would need more context
                    corrected: "".to_string(), // Placeholder
                    confidence: 0.8,
                    correction_type: issue.issue_type.clone(),
                });
            }
        }

        corrections
    }

    /// Calculate improvement based on scoring result.
    fn calculate_improvement(scoring: &ScoringResult) -> f32 {
        let overall = (scoring.tes + scoring.sfs + scoring.scs) / 3.0;
        overall * 0.1 // placeholder — proportional to overall quality
    }
}
