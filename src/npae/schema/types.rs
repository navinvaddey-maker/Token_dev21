use serde::{Deserialize, Serialize};
use crate::npae::compression::types::CompressedRepr;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AggressiveRequest {
    pub prompt: String,
    pub config: Option<NpaeConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NpaeConfig {
    pub ambiguity_threshold:  Option<f32>,
    pub max_questions:        Option<u8>,
    pub confidence_threshold: Option<f32>,
    pub skip_stage:           Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructuredPromptResponse {
    pub schema_version:      String,
    pub request_id:          String,
    pub optimized_prompt:    String,      // Added for UI compatibility (app.js)
    pub token_original:      u32,         // Added for UI compatibility
    pub token_final:         u32,         // Added for UI compatibility
    pub token_saved:         u32,         // Added for UI compatibility
    pub compression:         CompressionMeta,
    pub structured_prompt:   StructuredPrompt,
    pub ambiguity_analysis:  AmbiguityAnalysis,
    pub clarifying_questions: Vec<ClarifyingQuestion>,
    pub confidence_score:    f32,
    pub processing_metadata: ProcessingMeta,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompressionMeta {
    pub original_tokens: u32,
    pub compressed_tokens: u32,
    pub compression_ratio: f32,
    pub stage_metrics: crate::npae::compression::types::StageMetrics,
}

impl From<&CompressedRepr> for CompressionMeta {
    fn from(repr: &CompressedRepr) -> Self {
        Self {
            original_tokens: 0, // placeholder, compute actual in repr if possible
            compressed_tokens: repr.token_ids.len() as u32,
            compression_ratio: repr.compression_ratio,
            stage_metrics: repr.stage_metrics.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StructuredPrompt {
    pub role:               PromptRole,
    pub context:            PromptContext,
    pub constraints:        PromptConstraints,
    pub hallucination_guard: HallucinationGuardConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptRole {
    pub primary: String,
    pub expertise_domains: Vec<String>,
    pub persona_constraints: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptContext {
    pub domain: String,
    pub description: String,
    pub background: String,
    pub user_knowledge_level: String,
    pub temporal_scope: String,
    pub intent_vector: Vec<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptConstraints {
    pub output_format: String,
    pub length_bound: LengthBound,
    pub forbidden_topics: Vec<String>,
    pub required_inclusions: Vec<String>,
    pub tone: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LengthBound {
    pub min: u32,
    pub max: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HallucinationGuardConfig {
    pub self_critique_enabled:    bool,
    pub confidence_threshold:     f32,
    pub contradiction_check:      bool,
    pub claim_verification_rules: Vec<String>,
    pub uncertainty_markers:      Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AmbiguityAnalysis {
    pub score:     f32,
    pub threshold: f32,
    pub triggered: bool,
    pub gap_zones: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClarifyingQuestion {
    pub id:               u8,
    pub question:         String,
    pub information_gain: f32,
    pub gap_addressed:    String,
    pub priority:         Priority,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Priority { High, Medium, Low }

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
