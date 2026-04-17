use dashmap::DashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1Stats {
    pub original_tokens: u32,
    pub final_tokens: u32,
    pub cache_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1Out {
    pub clean_tokens: Vec<String>,
    pub sparse_mask:  Vec<bool>,
    pub stats:        Stage1Stats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage2Out {
    pub repr_vec:   Vec<f32>,   // semantic centroid vector (ndarray → Vec)
    pub intent_vec: Vec<f32>,   // probability dist over IntentClass variants
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompressedRepr {
    pub token_ids:         Vec<u32>,
    pub attention_weights: Vec<f32>,
    pub concept_graph:     DashMap<String, Vec<String>>,
    pub intent_vec:        Vec<f32>,
    pub compression_ratio: f32,
    pub stage_metrics:     StageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMetrics {
    pub stage1_ms: u64,
    pub stage2_ms: u64,
    pub stage3_ms: u64,
}

impl StageMetrics {
    pub fn total_ms(&self) -> u64 {
        // Technically these ran in parallel? No, the spec says "run_parallel_pipeline() re-invokes them in a 3-stage parallel pattern". Inside each stage, join handles parallelism. But the stages themselves (P1, P2, P3) run sequentially.
        self.stage1_ms + self.stage2_ms + self.stage3_ms
    }
}
