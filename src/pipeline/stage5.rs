use crate::{
    algorithms::sparse_coding::SparseCoding,
    types::{AlgorithmOutput, Mode},
};

/// Handles Stage 5: Scope Injection
pub struct Stage5 {
    _sparse: SparseCoding,
}

impl Stage5 {
    /// Creates a new Stage5 instance
    pub fn new() -> Self {
        Self {
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
                // Aggressive: generate scope injections from labels generated in Stage 3
                if !out.cluster_labels.is_empty() {
                    out.scope_injections = out.cluster_labels
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
