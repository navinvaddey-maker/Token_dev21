use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub resolved_task: Option<String>,
    pub resolved_deliverable: Option<String>,
    pub resolved_context: Vec<String>,
    pub resolved_constraints: Option<String>,
    pub null_fields: Vec<String>,
    pub task_inferred: bool,
    pub deliverable_inferred: bool,

    // New fields from refinements guide
    pub topology: Option<PromptTopology>,
    pub normalization: Option<NormalizationResult>,
    pub ordinal_sequence: Option<OrdinalSequence>,
    pub field_issues: Vec<FieldValidationIssue>,
    pub scope_injections: Vec<String>,
    pub dual_score: Option<DualScore>,
    pub correction_cycle: Option<CorrectionCycle>,
}

/// Final packaged response sent to the caller / UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompressionResponse {
    pub mode: String,
    pub error_score: f32,
    pub fidelity: f32,
    pub task: Option<String>,
    pub deliverable: Option<String>,
    pub context: Vec<String>,
    pub constraints: Option<String>,
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
    pub dual_score: DualScore,
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

/// Dual scoring mechanism (e.g., salience and novelty).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DualScore {
    pub primary: f32,
    pub secondary: f32,
    pub combined: f32,
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
        dual_score: &DualScore,
    ) -> Self {
        let corrections = Self::analyze_corrections(compressed_output, field_issues);
        let improvement = Self::calculate_improvement(dual_score);

        Self {
            cycle_number: self.cycle_number + 1,
            corrections_applied: corrections,
            improvement,
        }
    }

    /// Analyze the compressed output and field issues to determine needed corrections.
    fn analyze_corrections(
        compressed_output: &str,
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

    /// Calculate improvement based on dual score (placeholder logic).
    fn calculate_improvement(dual_score: &DualScore) -> f32 {
        dual_score.combined * 0.1 // Placeholder
    }
}
