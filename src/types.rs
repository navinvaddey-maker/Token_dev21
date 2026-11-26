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
    #[serde(skip, default = "std::time::Instant::now")]
    pub last_accessed: std::time::Instant,
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
    pub expected_deliverables: Vec<String>,
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

    // New fields from refinements guide
    pub topology: Option<PromptTopology>,
    pub normalization: Option<NormalizationResult>,
    pub ordinal_sequence: Option<OrdinalSequence>,
    pub field_issues: Vec<FieldValidationIssue>,
    pub scope_injections: Vec<String>,
    pub scoring_result: Option<ScoringResult>,
    pub correction_cycle: Option<CorrectionCycle>,
    pub scenario: Option<crate::classifier::signal::ScenarioSignal>,
    /// When true, Stage 6A emits a compact prompt (correction for low TES).
    #[serde(default)]
    pub force_compact_generation: bool,
    pub enrichment: Option<EnrichmentContext>,
    /// Final rendered output text produced by Stage 6A/6B, evaluated by honest scorers (SCS, SFS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunk {
    pub domain_tag: String,
    pub content: String,
    pub metadata: Option<ChunkMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub page_number: Option<u32>,
    pub source_file: String,
}

impl From<crate::rag::types::SearchResult> for RagChunk {
    /// Converts a retrieved RAG [`crate::rag::types::SearchResult`] into a pipeline [`RagChunk`].
    ///
    /// Clamps negative `page_number` values to `0` before narrowing `i64` to `u32`.
    ///
    /// # Panics
    /// Never.
    fn from(res: crate::rag::types::SearchResult) -> Self {
        Self {
            domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
            content: res.chunk.content,
            metadata: Some(ChunkMetadata {
                page_number: res
                    .chunk
                    .metadata
                    .as_ref()
                    .and_then(|m| m.page_number)
                    .map(|p| u32::try_from(p.max(0)).unwrap_or(0)),
                source_file: res.document_filename,
            }),
        }
    }
}

/// A structured key-value datum supplied via `EnrichmentContext` (e.g. CRM/DB field, user profile,
/// or API metric) that enters `EvidenceBundle` with `Provenance::StructuredRecord`.
///
/// // ARCH: ARCHITECTURE.md §6.6 — Phase 4 structured data ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredKeyValue {
    /// Origin system or dataset identifier (e.g. `"crm_db"`, `"user_profile"`).
    pub source_id: String,
    /// Stable record identifier within `source_id` (e.g. `"acct_42"`).
    #[serde(default)]
    pub record_key: String,
    /// Field or attribute name.
    pub key: String,
    /// Field value.
    pub value: String,
    /// Optional relevance score in `[0.0, 1.0]` (defaults to `0.90`).
    #[serde(default)]
    pub relevance_score: Option<f32>,
}

/// A structured table supplied via `EnrichmentContext` (e.g. SQL query result or CSV table)
/// whose rows enter `EvidenceBundle` as `EvidenceKind::TableRow` with `Provenance::StructuredRecord`.
///
/// // ARCH: ARCHITECTURE.md §6.6 — Phase 4 structured table ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredTable {
    /// Origin system or dataset identifier (e.g. `"analytics_warehouse"`).
    pub source_id: String,
    /// Logical table name (e.g. `"quarterly_metrics"`).
    pub table_name: String,
    /// Ordered column headers corresponding 1-to-1 with each row's cell values.
    pub columns: Vec<String>,
    /// Data rows; each row must match `columns.len()`.
    pub rows: Vec<Vec<String>>,
    /// Optional relevance score in `[0.0, 1.0]` (defaults to `0.88`).
    #[serde(default)]
    pub relevance_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnrichmentContext {
    pub cluster_vocab: Option<String>,
    pub rag_chunks: Option<Vec<RagChunk>>,
    /// Optional structured key-value records ingested into `EvidenceBundle` with `Provenance::StructuredRecord`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_records: Option<Vec<StructuredKeyValue>>,
    /// Optional structured tables ingested row-by-row into `EvidenceBundle` with `Provenance::StructuredRecord`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_tables: Option<Vec<StructuredTable>>,
}

/// Unified response enum for the PipelineOrchestrator
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrchestratorResponse {
    Legacy(CompressionResponse),
    Aggressive(crate::npae::schema::types::StructuredPromptResponse),
}

/// Final packaged response sent to the caller / UI (Legacy Mode).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hallucination_report: Option<crate::npae::hallucination::guard::HallucinationReport>,
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
    pub tes: f32, // 0.0–10.0
    pub sfs: f32, // 0.0–10.0
    pub scs: f32, // 0.0–10.0 (NEW)
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
        compressed_output: &str,
        field_issues: &[FieldValidationIssue],
    ) -> Vec<TextCorrection> {
        let mut corrections = Vec::new();

        for issue in field_issues {
            if issue.severity == "error" || issue.severity == "warning" {
                // Extract the problematic content from the compressed output if identifiable
                let original = if !issue.field_name.is_empty() {
                    // Try to find the field content in the output
                    compressed_output
                        .lines()
                        .find(|line| {
                            line.to_lowercase()
                                .contains(&issue.field_name.to_lowercase())
                        })
                        .unwrap_or("")
                        .to_string()
                } else {
                    String::new()
                };

                let corrected = match issue.issue_type.as_str() {
                    "missing_required" => format!("[NEEDS: {}]", issue.field_name),
                    "type_mismatch" => {
                        format!("[FIX TYPE: {} — {}]", issue.field_name, issue.description)
                    }
                    "empty_field" => format!("[FILL: {}]", issue.field_name),
                    _ => format!("[REVIEW: {} — {}]", issue.field_name, issue.description),
                };

                let confidence = match issue.severity.as_str() {
                    "error" => 0.9,
                    "warning" => 0.7,
                    _ => 0.5,
                };

                corrections.push(TextCorrection {
                    original,
                    corrected,
                    confidence,
                    correction_type: issue.issue_type.clone(),
                });
            }
        }

        corrections
    }

    /// Create a new correction cycle with actual empirical score improvement calculation.
    pub fn new_cycle_with_delta(
        &self,
        compressed_output: &str,
        field_issues: &[FieldValidationIssue],
        before_scoring: &ScoringResult,
        after_scoring: &ScoringResult,
    ) -> Self {
        let corrections = Self::analyze_corrections(compressed_output, field_issues);
        let improvement = Self::calculate_improvement_delta(before_scoring, after_scoring);

        Self {
            cycle_number: self.cycle_number + 1,
            corrections_applied: corrections,
            improvement,
        }
    }

    /// Calculate actual improvement between pre-correction and post-correction scoring results.
    pub fn calculate_improvement_delta(before: &ScoringResult, after: &ScoringResult) -> f32 {
        let before_avg = (before.tes + before.sfs + before.scs) / 3.0;
        let after_avg = (after.tes + after.sfs + after.scs) / 3.0;
        (after_avg - before_avg).max(0.0)
    }

    /// Calculate expected improvement per correction cycle.
    /// Uses the gap between current scores and the target (10.0) to estimate
    /// how much improvement each correction should provide.
    fn calculate_improvement(scoring: &ScoringResult) -> f32 {
        let overall = (scoring.tes + scoring.sfs + scoring.scs) / 3.0;
        let gap = (10.0 - overall).max(0.0);
        gap * 0.30
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::types::{
        ChunkMetadata as StorageChunkMetadata, RagChunk as StorageRagChunk, SearchResult,
    };

    #[test]
    fn test_rag_chunk_from_search_result() {
        let sr = SearchResult {
            chunk: StorageRagChunk {
                id: "c1".to_string(),
                document_id: "d1".to_string(),
                chunk_index: 0,
                content: "Revenue rose 15%.".to_string(),
                embedding: None,
                metadata: Some(StorageChunkMetadata {
                    page_number: Some(4),
                    section_title: None,
                    char_offset_start: None,
                    char_offset_end: None,
                }),
                token_count: Some(4),
                created_at: chrono::Utc::now(),
                embedding_model: "all-MiniLM-L6-v2".to_string(),
                model_version: "1.0".to_string(),
            },
            similarity_score: 0.91,
            document_filename: "annual_report.pdf".to_string(),
            document_domain: None,
        };
        let converted = RagChunk::from(sr);
        assert_eq!(converted.domain_tag, "general");
        assert_eq!(converted.content, "Revenue rose 15%.");
        let meta = converted.metadata.as_ref();
        assert_eq!(meta.and_then(|m| m.page_number), Some(4));
        assert_eq!(
            meta.map(|m| m.source_file.as_str()),
            Some("annual_report.pdf")
        );

        // Negative page_number clamps safely to 0
        let sr_neg = SearchResult {
            chunk: StorageRagChunk {
                id: "c2".to_string(),
                document_id: "d2".to_string(),
                chunk_index: 1,
                content: "Statute §12".to_string(),
                embedding: None,
                metadata: Some(StorageChunkMetadata {
                    page_number: Some(-5),
                    section_title: None,
                    char_offset_start: None,
                    char_offset_end: None,
                }),
                token_count: Some(3),
                created_at: chrono::Utc::now(),
                embedding_model: "all-MiniLM-L6-v2".to_string(),
                model_version: "1.0".to_string(),
            },
            similarity_score: 0.85,
            document_filename: "act.pdf".to_string(),
            document_domain: Some("legal".to_string()),
        };
        let converted_neg = RagChunk::from(sr_neg);
        assert_eq!(converted_neg.domain_tag, "legal");
        assert_eq!(converted_neg.metadata.and_then(|m| m.page_number), Some(0));
    }
}
