pub mod chunking;
pub mod evaluation;
pub mod pipeline;
pub mod predictive_coding;
pub mod reconstruction;
pub mod selective_attention;
pub mod sparse_coding;
pub mod working_memory;
pub mod neuro;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PrincipleResult {
    pub text: String,
    pub chunks: Vec<String>,
    pub items_removed: usize,
    pub detail: String,
    pub duration_ms: u64,
}
