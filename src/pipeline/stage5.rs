use crate::{
    algorithms::{semantic_clustering::SemanticClustering, sparse_coding::SparseCoding},
    types::{AlgorithmOutput, Mode},
};

/// Handles Stage 5: Scope Injection
pub struct Stage5 {
    clustering: SemanticClustering,
    _sparse: SparseCoding,
}

impl Stage5 {
    /// Creates a new Stage5 instance
    pub fn new() -> Self {
        Self {
            clustering: SemanticClustering::default(),
            _sparse: SparseCoding::default(),
        }
    }

    /// Processes input through Stage 5: Scope Injection
    /// Mode-aware: Gentle = no scope injection, Aggressive = scope injection from clustering
    pub fn run(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        match mode {
            Mode::Gentle | Mode::Ambiguous | Mode::Balanced => {
                // Gentle: no scope injection
                out.scope_injections = vec![];
            }
            Mode::Aggressive => {
                // Aggressive: generate scope injections from delta tokens using clustering
                if !out.delta_tokens.is_empty() {
                    let cluster_result = self.clustering.cluster(&out.delta_tokens);
                    out.scope_injections = cluster_result
                        .labels
                        .iter()
                        .map(|label| format!("Consider {}", label))
                        .collect();
                } else {
                    out.scope_injections = vec![];
                }
            }
        }
    }
}
