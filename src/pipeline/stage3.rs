use crate::{
    algorithms::semantic_clustering::SemanticClustering,
    algorithms::sparse_coding::SparseCoding,
    algorithms::working_memory::{ContextFrame, WorkingMemory},
    types::{AlgorithmOutput, Mode, SlotSource, WmSlot},
};

/// Handles Stage 3: Context Management
pub struct Stage3 {
    clustering: SemanticClustering,
    sparse: SparseCoding,
}

impl Stage3 {
    /// Creates a new Stage3 instance
    pub fn new() -> Self {
        Self {
            clustering: SemanticClustering::default(),
            sparse: SparseCoding::default(),
        }
    }

    /// Processes input through Stage 3: Context Management
    /// Mode-aware: Gentle skips Semantic Clustering.
    pub fn run(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        match mode {
            Mode::Gentle | Mode::Ambiguous => {
                // Gentle: load sparse tokens directly into 3-slot WM
                let mut wm = WorkingMemory::new(mode);
                let items: Vec<WmSlot> = out
                    .sparse_tokens
                    .iter()
                    .map(|t| WmSlot {
                        content: t.text.clone(),
                        salience: t.salience,
                        source: SlotSource::Delta,
                    })
                    .collect();
                wm.load(items);
                let frame = wm.get_context_frame();
                self.write_wm_frame(frame, out);
            }

            Mode::Aggressive => {
                // Aggressive: Semantic Clustering on delta_tokens → load clusters as slots
                let cluster_result = self.clustering.cluster(&out.delta_tokens);
                out.clusters = cluster_result.groups.clone();
                out.cluster_labels = cluster_result.labels.clone();

                let mut wm = WorkingMemory::new(&Mode::Aggressive);
                let items: Vec<WmSlot> = cluster_result
                    .labels
                    .iter()
                    .enumerate()
                    .map(|(i, label): (usize, &String)| WmSlot {
                        content: label.clone(),
                        salience: 0.70 + (i as f32 * 0.03),
                        source: SlotSource::Cluster,
                    })
                    .collect();
                wm.load(items);
                let frame = wm.get_context_frame();
                self.write_wm_frame(frame, out);
            }
        }
    }

    fn write_wm_frame(&self, frame: ContextFrame, out: &mut AlgorithmOutput) {
        out.wm_slots = frame.items;
        out.wm_utilisation = frame.utilisation;
        out.fidelity_estimate = frame.fidelity;
    }
}
