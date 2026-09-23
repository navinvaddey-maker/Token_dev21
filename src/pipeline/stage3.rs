use crate::{
    algorithms::semantic_clustering::SemanticClustering,
    algorithms::working_memory::{ContextFrame, WorkingMemory},
    types::{AlgorithmOutput, Mode, SlotSource, WmSlot},
};

/// Handles Stage 3: Context Management
pub struct Stage3 {
    // clustering: SemanticClustering,
}

impl Stage3 {
    /// Creates a new Stage3 instance
    pub fn new() -> Self {
        Self {
        }
    }

    /// Processes input through Stage 3: Context Management
    /// Mode-aware: Seeding priority changes based on mode.
    pub fn run(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        // Pre-seed WM using a strict priority hierarchy: Locks -> Clusters -> Sparse Tokens
        let mut wm = WorkingMemory::new(mode);
        let mut items = Vec::new();

        // 1. HIGHEST PRIORITY: Constraint Locks (Non-negotiable structural anchors)
        for lock in &out.constraint_locks {
            items.push(WmSlot {
                content: lock.text.clone(),
                salience: 1.0,
                source: SlotSource::Cluster,
                is_protected: true,
                last_accessed: std::time::Instant::now(),
            });
        }

        // 2. SECONDARY PRIORITY: Structural Clusters (from Stage -1)
        // This fixes GAP-23 by using the cluster map instead of just raw tokens
        for (label, members) in out.cluster_labels.iter().zip(&out.clusters) {
            if let Some(top_token) = members.first() {
                items.push(WmSlot {
                    content: format!("{}: {}", label, top_token),
                    salience: 0.85,
                    source: SlotSource::Cluster,
                    is_protected: false,
                    last_accessed: std::time::Instant::now(),
                });
            }
        }

        // 3. TERTIARY PRIORITY: Sparse Tokens (Fill remaining slots)
        for t in &out.sparse_tokens {
            // Avoid duplicating tokens already added via locks or clusters
            if !items.iter().any(|item| item.content.contains(&t.text)) {
                items.push(WmSlot {
                    content: t.text.clone(),
                    salience: t.salience,
                    source: SlotSource::Delta,
                    is_protected: false,
                    last_accessed: std::time::Instant::now(),
                });
            }
        }

        wm.load(items);
        let frame = wm.get_context_frame();
        self.write_wm_frame(frame, out);
    }

    fn write_wm_frame(&self, frame: ContextFrame, out: &mut AlgorithmOutput) {
        out.wm_slots = frame.items;
        out.wm_utilisation = frame.utilisation;
        out.fidelity_estimate = frame.fidelity;
    }
}
