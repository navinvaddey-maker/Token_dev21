use crate::{
    algorithms::{
        field_validator::FieldTypeValidator,
        lexical::LexicalCompression,
        predictive_coding::{PredictiveCoding, SessionTurn},
        schema_filling::SchemaFilling,
        semantic_clustering::SemanticClustering,
        sparse_coding::SparseCoding,
        working_memory::{ContextFrame, WorkingMemory},
    },
    session::SessionHistory,
    types::{AlgorithmOutput, Mode, SlotSource, WmSlot},
};

// Declare submodules
pub mod orchestrator;
pub mod stage0_normalize;
pub mod stage0b_topology;
pub mod stage1;
pub mod stage2;
pub mod stage3;
pub mod stage4;
pub mod stage5;
pub mod stage6a;
pub mod tee_topology_classifier;

// Import specific items from submodules
use crate::pipeline::stage0b_topology::TopologyClassifier;
use crate::pipeline::stage5::Stage5;
use dashmap::DashMap;
use std::sync::Arc;

pub struct TokenCompressionPipeline {
    pub lexical: LexicalCompression,         // existing algorithm
    pub sparse: SparseCoding,                // new
    pub predictive: PredictiveCoding,        // new — THIS SOLVES THE BOUNDARY
    pub clustering: SemanticClustering,      // existing algorithm
    pub schema: SchemaFilling,               // existing algorithm (extended)
    pub field_validator: FieldTypeValidator, // new — field validation stage
    pub stage5: Stage5,                      // new — scope injection stage
}

impl TokenCompressionPipeline {
    pub fn new(schema_priors: Arc<DashMap<String, u32>>) -> Self {
        Self {
            lexical: LexicalCompression::default(),
            sparse: SparseCoding::default(),
            predictive: PredictiveCoding::new(schema_priors),
            clustering: SemanticClustering::default(),
            schema: SchemaFilling::default(),
            field_validator: FieldTypeValidator::new(),
            stage5: Stage5::new(),
        }
    }

    /// Stage 1 — Signal Reduction
    /// Lexical Compression → Sparse Coding
    /// Always runs, both modes. No mode awareness yet.
    pub fn stage1_reduction(&self, raw_prompt: &str, out: &mut AlgorithmOutput) {
        // Existing: Lexical Compression
        let lex_result = self.lexical.compress(raw_prompt, out);
        out.clean_tokens = lex_result.tokens;
        out.compression_ratio = lex_result.ratio;

        // New: Sparse Coding — receives clean_tokens
        // Use aggressive ratio at stage 1 (mode unknown; be generous)
        let scored = self.sparse.apply(&out.clean_tokens, 0.60);
        out.salience_map = scored
            .iter()
            .map(|t| (t.text.clone(), t.salience))
            .collect();
        out.sparse_tokens = scored;
    }

    /// Stage 2 — Boundary Detection
    /// Predictive Coding → mode decision
    /// Always runs, both modes. Output: out.mode is set here.
    pub fn stage2_boundary(&self, session: &SessionHistory, out: &mut AlgorithmOutput) {
        let result = self.predictive.compute_error(
            &out.sparse_tokens,
            &session.turns,
            out.topology.clone().unwrap_or_default(),
        );
        out.error_score = result.error_score;
        out.delta_tokens = result.delta_tokens;
        out.mode = Some(result.mode);
        out.is_ambiguous = result.is_ambiguous;
    }

    /// Stage 3 — Context Management
    /// Mode-aware: Gentle skips Semantic Clustering.
    pub fn stage3_context(&self, out: &mut AlgorithmOutput) {
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

    /// Stage 4 — NULL Resolution
    /// Mode-aware: Gentle = 1 layer, Aggressive = 3 layers.
    pub fn stage4_schema(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        let layers = match mode {
            Mode::Gentle | Mode::Ambiguous => 1,
            Mode::Aggressive => 3,
        };

        // Extract all needed values to avoid borrow conflicts
        let wm_slots = out.wm_slots.clone();
        let delta_tokens = out.delta_tokens.clone();
        let clusters = out.clusters.clone();

        let result = self
            .schema
            .fill(&wm_slots, layers, &delta_tokens, &clusters, Some(out));

        out.resolved_task = result.task.clone();
        out.resolved_deliverable = result.deliverable.clone();
        out.resolved_context = result.context.clone();
        out.null_fields = result.null_fields;
        out.task_inferred = result.task_inferred;
        out.deliverable_inferred = result.deliverable_inferred;
    }

    /// Stage 4b — Field Validation
    /// Validates the resolved fields (task, deliverable, constraints, context) for type and content issues.
    /// Always runs after schema filling, regardless of mode.
    pub fn stage4_field_validation(&self, out: &mut AlgorithmOutput) {
        // Extract resolved fields from AlgorithmOutput
        let task = &out.resolved_task;
        let deliverable = &out.resolved_deliverable;
        let constraints = if out.resolved_context.is_empty() {
            &None
        } else {
            &Some(out.resolved_context.join(", "))
        };
        let context = &out.resolved_context;

        // Run field validation
        let issues = self
            .field_validator
            .validate(task, deliverable, constraints, context);
        out.field_issues = issues;
    }

    fn write_wm_frame(&self, frame: ContextFrame, out: &mut AlgorithmOutput) {
        out.wm_slots = frame.items;
        out.wm_utilisation = frame.utilisation;
        out.fidelity_estimate = frame.fidelity;
    }
}
