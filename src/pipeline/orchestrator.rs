use crate::{
    algorithms::{
        field_validator::FieldTypeValidator,
        predictive_coding::{PredictiveCoding, SessionTurn},
        schema_filling::SchemaFilling,
        sparse_coding::SparseCoding,
        working_memory::{ContextFrame, WorkingMemory},
    },
    correction::cycle, // for the impl of CorrectionCycle
    pipeline::{
        stage0_normalize::NormalizationPrePass, stage0b_topology::TopologyClassifier,
        stage1::Stage1, stage2::Stage2, stage3::Stage3, stage4::Stage4, stage5::Stage5,
        stage6a::Stage6a,
    },
    scoring::sfs::SemanticFidelityScorer,
    scoring::tes::TokenEfficiencyScorer,
    session::SessionHistory,
    types::{
        AlgorithmOutput, CompressionResponse, CorrectionCycle, DualScore, FieldValidationIssue,
        NormalizationResult, OrdinalSequence, PromptTopology,
    },
};

/// Orchestrates the token compression pipeline stages as specified in the refinements guide.
pub struct PipelineOrchestrator {
    normalization_pre_pass: NormalizationPrePass,
    topology_classifier: TopologyClassifier,
    stage1: Stage1,
    stage2: Stage2,
    stage3: Stage3,
    stage4: Stage4,
    stage5: Stage5,
    field_validator: FieldTypeValidator,
    token_efficiency_scorer: TokenEfficiencyScorer,
    semantic_fidelity_scorer: SemanticFidelityScorer,
    stage6a: Stage6a,
    correction_cycle: CorrectionCycle,
}

impl PipelineOrchestrator {
    /// Creates a new pipeline orchestrator with all required stages.
    pub fn new(
        normalization_pre_pass: NormalizationPrePass,
        topology_classifier: TopologyClassifier,
        stage1: Stage1,
        stage2: Stage2,
        stage3: Stage3,
        stage4: Stage4,
        stage5: Stage5,
        field_validator: FieldTypeValidator,
        token_efficiency_scorer: TokenEfficiencyScorer,
        semantic_fidelity_scorer: SemanticFidelityScorer,
        stage6a: Stage6a,
        correction_cycle: CorrectionCycle,
    ) -> Self {
        Self {
            normalization_pre_pass,
            topology_classifier,
            stage1,
            stage2,
            stage3,
            stage4,
            stage5,
            field_validator,
            token_efficiency_scorer,
            semantic_fidelity_scorer,
            stage6a,
            correction_cycle,
        }
    }

    /// Processes the input through all pipeline stages (0A through 6B) and returns a compression response.
    pub fn process(
        &self,
        input: &str,
        session: &SessionHistory,
    ) -> Result<CompressionResponse, Box<dyn std::error::Error>> {
        // Initialize AlgorithmOutput that will be passed through the pipeline
        let mut output = AlgorithmOutput::default();

        // Stage 0A: Normalization pre-pass
        let normalized = self.normalization_pre_pass.run(input, &mut output);

        // Stage 0B: Topology classification
        let topology = self.topology_classifier.classify(&normalized, &mut output);

        // Stage 1: Signal Reduction (Lexical Compression → Sparse Coding)
        self.stage1.run(&normalized, &mut output);

        // Stage 2: Boundary Detection (Predictive Coding → mode decision)
        self.stage2.run(session, &mut output);

        // Stage 3: Context Management
        self.stage3.run(&mut output);

        // Stage 4: Schema filling (NULL Resolution)
        self.stage4.run(&mut output);

        // Stage 5: Scope Injection
        self.stage5.run(&mut output);

        // Stage 4B: Field validation
        let field_issues = self.field_validator.validate(
            &output.resolved_task,
            &output.resolved_deliverable,
            &output.resolved_constraints,
            &output.resolved_context,
        );

        // Update output with field issues
        output.field_issues = field_issues.clone();

        // Scoring stages
        let tes = TokenEfficiencyScorer::score(&output, &field_issues);
        let sfs = SemanticFidelityScorer::score(&field_issues);
        let dual_score = DualScore {
            primary: tes,
            secondary: sfs,
            combined: (tes + sfs) / 2.0,
        };

        // Update output with scores
        output.dual_score = Some(dual_score.clone());

        // Stage 6A: Output generation
        let stage6a_output = self.stage6a.run(&output)?;

        // Stage 6B: Correction cycle
        let correction_cycle =
            self.correction_cycle
                .new_cycle(&stage6a_output, &field_issues, &dual_score);

        // Update output with correction cycle
        output.correction_cycle = Some(correction_cycle.clone());

        // Package the response with all required fields
        self.package_response(
            stage6a_output,
            output.topology.clone().unwrap_or_default(),
            output.normalization.clone().unwrap_or_default(),
            output.ordinal_sequence.clone(),
            field_issues,
            dual_score,
            correction_cycle,
            &output,
        )
    }

    /// Packages the pipeline output into a CompressionResponse with all new fields.
    fn package_response(
        &self,
        compressed_prompt: String,
        topology: PromptTopology,
        normalization: NormalizationResult,
        ordinal_sequence: Option<OrdinalSequence>,
        field_issues: Vec<FieldValidationIssue>,
        dual_score: DualScore,
        correction_cycle: CorrectionCycle,
        algorithm_output: &AlgorithmOutput,
    ) -> Result<CompressionResponse, Box<dyn std::error::Error>> {
        Ok(CompressionResponse {
            mode: algorithm_output
                .mode
                .as_ref()
                .map(|m| format!("{:?}", m))
                .unwrap_or_else(|| "unknown".to_string()),
            error_score: algorithm_output.error_score,
            fidelity: algorithm_output.fidelity_estimate,
            task: algorithm_output.resolved_task.clone(),
            deliverable: algorithm_output.resolved_deliverable.clone(),
            context: algorithm_output.resolved_context.clone(),
            constraints: algorithm_output.resolved_constraints.clone(),
            wm_slots_used: algorithm_output.wm_slots.len(),
            null_fields: algorithm_output.null_fields.clone(),
            response: compressed_prompt,
            normalization,
            field_issues,
            topology,
            scope_injections: algorithm_output.scope_injections.clone(),
            // Aggressive only
            clusters: if !algorithm_output.clusters.is_empty() {
                // This is a placeholder - in a real implementation we'd return the actual clusters
                Some(vec!["cluster1".to_string(), "cluster2".to_string()])
            } else {
                None
            },
            delta_tokens: if !algorithm_output.delta_tokens.is_empty() {
                // This is a placeholder - in a real implementation we'd return the actual delta tokens
                Some(vec!["delta1".to_string(), "delta2".to_string()])
            } else {
                None
            },
            reasoning_chain: None,
            dual_score,
            correction_cycle,
        })
    }
}
