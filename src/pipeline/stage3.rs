use crate::{
    algorithms::semantic_clustering::SemanticClustering,
    algorithms::sparse_coding::SparseCoding,
    algorithms::working_memory::{ContextFrame, WorkingMemory},
    types::{AlgorithmOutput, Mode, SlotSource, WmSlot},
};

/// Handles Stage 3: Context Management
pub struct Stage3 {
    clustering: SemanticClustering,
    _sparse: SparseCoding,
}

impl Stage3 {
    /// Creates a new Stage3 instance
    pub fn new() -> Self {
        Self {
            clustering: SemanticClustering::default(),
            _sparse: SparseCoding::default(),
        }
    }

    /// Processes input through Stage 3: Context Management
    /// Mode-aware: Gentle skips Semantic Clustering.
    pub fn run(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        // Pre-seed WM using Constraint Locks and Clusters
        let mut wm = WorkingMemory::new(mode);
        let mut items = Vec::new();

        // 1. Add all CONSTRAINT_LOCK tokens with highest priority
        for lock in &out.constraint_locks {
            items.push(WmSlot {
                content: lock.text.clone(),
                salience: 1.0,
                source: SlotSource::Cluster,
                is_protected: true,
            });
        }

        // 2. Add clusters or sparse tokens based on mode
        match mode {
            Mode::Gentle | Mode::Ambiguous | Mode::Balanced => {
                // Use sparse tokens for remaining slots
                for t in &out.sparse_tokens {
                    items.push(WmSlot {
                        content: t.text.clone(),
                        salience: t.salience,
                        source: SlotSource::Delta,
                        is_protected: false,
                    });
                }
            }
            Mode::Aggressive => {
                // Semantic Clustering on delta_tokens → load clusters as slots
                let cluster_result = self.clustering.cluster(&out.delta_tokens);
                out.clusters = cluster_result.groups.clone();
                out.cluster_labels = cluster_result.labels.clone();

                for (i, label) in cluster_result.labels.iter().enumerate() {
                    items.push(WmSlot {
                        content: label.clone(),
                        salience: 0.70 + (i as f32 * 0.03),
                        source: SlotSource::Cluster,
                        is_protected: false,
                    });
                }
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
