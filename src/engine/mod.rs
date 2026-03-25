pub mod chunking;
pub mod hebbian_binding;
pub mod predictive_coding;
pub mod selective_attention;
pub mod sparse_coding;
pub mod working_memory;
pub mod pipeline;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PrincipleResult {
    pub text:          String,
    pub chunks:        Vec<String>,
    pub items_removed: usize,
    pub detail:        String,
    pub duration_ms:   u64,
}
