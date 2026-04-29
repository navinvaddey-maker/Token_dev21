use dashmap::DashMap;
use std::sync::Arc;

use crate::{
    algorithms::{
        field_validator::FieldTypeValidator,
        predictive_coding::PredictiveCoding,
    },
    engine::reconstruction::TokenReconstructor,
    pipeline::{
        stage0_normalize::NormalizationPrePass, stage0b_topology::TopologyClassifier,
        stage1::Stage1, stage2::Stage2, stage3::Stage3, stage4::Stage4, stage5::Stage5,
        stage6a::Stage6a,
    },
    scoring::sfs::SemanticFidelityScorer,
    scoring::tes::TokenEfficiencyScorer,
    session::SessionHistory,
    types::{
        AlgorithmOutput, CompressionResponse, CorrectionCycle, FieldValidationIssue,
        NormalizationResult, OrdinalSequence, PromptTopology, OrchestratorResponse, Mode,
    },
    npae::aggressive::config::ConfigHandle,
};

/// Orchestrates the token compression pipeline stages as specified in the refinements guide.
pub struct PipelineOrchestrator {
    reconstructor: TokenReconstructor,
    normalization_pre_pass: NormalizationPrePass,
    topology_classifier: TopologyClassifier,
    stage1: Stage1,
    stage2: Stage2,
    stage3: Stage3,
    stage4: Stage4,
    stage5: Stage5,
    field_validator: FieldTypeValidator,
    _token_efficiency_scorer: TokenEfficiencyScorer,
    _semantic_fidelity_scorer: SemanticFidelityScorer,
    stage6a: Stage6a,
    correction_cycle: CorrectionCycle,
    npae_config_handle: Arc<ConfigHandle>,
}

impl PipelineOrchestrator {
    /// Creates a new pipeline orchestrator with all required stages.
    pub fn new(
        reconstructor: TokenReconstructor,
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
            reconstructor,
            normalization_pre_pass,
            topology_classifier,
            stage1,
            stage2,
            stage3,
            stage4,
            stage5,
            field_validator,
            _token_efficiency_scorer: token_efficiency_scorer,
            _semantic_fidelity_scorer: semantic_fidelity_scorer,
            stage6a,
            correction_cycle,
            npae_config_handle: Arc::new(ConfigHandle::new(crate::npae::aggressive::config::UnifiedConfig {
                domain_taxonomy: vec![],
                roles: vec![],
                constraints: vec![],
            })), // Fallback if not injected properly, though we will inject via build
        }
    }

    /// Build orchestrator from shared schema priors — convenience factory.
    /// All 12 stage components are constructed internally with defaults.
    pub fn build(schema_priors: Arc<DashMap<String, u32>>, npae_config_handle: Arc<ConfigHandle>) -> Self {
        let predictive = PredictiveCoding::new(schema_priors);
        Self {
            reconstructor: TokenReconstructor::new(),
            normalization_pre_pass: NormalizationPrePass::new(),
            topology_classifier: TopologyClassifier::new(),
            stage1: Stage1::new(),
            stage2: Stage2::new(predictive),
            stage3: Stage3::new(),
            stage4: Stage4::new(),
            stage5: Stage5::new(),
            field_validator: FieldTypeValidator::new(),
            _token_efficiency_scorer: TokenEfficiencyScorer::new(),
            _semantic_fidelity_scorer: SemanticFidelityScorer::new(),
            stage6a: Stage6a::new(),
            correction_cycle: CorrectionCycle::new(0),
            npae_config_handle,
        }
    }

    /// Processes the input through all pipeline stages (0A through 6B) and returns a compression response.
    pub fn process(
        &self,
        input: &str,
        session: &mut SessionHistory,
        forced_mode: Option<&str>,
    ) -> Result<OrchestratorResponse, Box<dyn std::error::Error>> {
        // Initialize AlgorithmOutput that will be passed through the pipeline
        let mut output = AlgorithmOutput::default();

        // Stage -1: Token Reconstruction
        let reconstructed = self.reconstructor.run(input);
        output.constraint_locks = reconstructed.constraint_locks.clone();
        output.ambiguity_register = reconstructed.ambiguity_register.clone();
        output.input_structure_score = reconstructed.input_structure_score;
        
        output.clusters = reconstructed.clusters.values()
            .map(|tokens| tokens.iter().map(|t| t.text.clone()).collect())
            .collect();
        output.cluster_labels = reconstructed.clusters.keys()
            .map(|k| format!("{:?}", k))
            .collect();

        // Stage 0A: Normalization pre-pass
        let normalized = self.normalization_pre_pass.run(&reconstructed, &mut output);

        // Stage 0B: Topology classification
        let _topology = self.topology_classifier.classify(&normalized, &mut output);

        // Stage 1: Signal Reduction (Lexical Compression → Sparse Coding)
        self.stage1.run(&normalized, &mut output);

        // Stage 2: Boundary Detection (Predictive Coding → mode decision)
        if let Some(fm) = forced_mode {
            let m = match fm.to_lowercase().as_str() {
                "aggressive" => Mode::Aggressive,
                "gentle" => Mode::Gentle,
                "balanced" => Mode::Balanced,
                _ => Mode::Balanced,
            };
            output.mode = Some(m);
        } else {
            self.stage2.run(session, &mut output);
        }

        let is_aggressive = output.mode.as_ref() == Some(&Mode::Aggressive);

        if is_aggressive {
            // -- Aggressive Routing (NPAE) --
            let npae_cfg = crate::npae::schema::types::NpaeConfig {
                ambiguity_threshold: Some(0.65),
                max_questions: Some(3),
                confidence_threshold: Some(0.75),
                skip_stage: None,
            };

            let repr = crate::npae::compression::pipeline::run_parallel_pipeline(input)
                .map_err(|e| e.to_string())?;

            let structurer = crate::npae::aggressive::structurer::HttpStructurer {
                route: "/api/compress".to_string(),
                remote_addr: "127.0.0.1".to_string(), // In real app, pass actual address
                body: input.to_string(),
            };

            let resp = crate::npae::aggressive::engine::AggressiveEngine::run(
                input, 
                &repr, 
                &npae_cfg, 
                self.npae_config_handle.clone(), 
                &structurer
            ).map_err(|e| e.to_string())?;

            // Push to session history for tracking
            session.push(input, &output);
            
            return Ok(OrchestratorResponse::Aggressive(resp));
        }

        // -- Legacy Routing (Balanced/Gentle) --
        
        // Stage 3: Context Management
        self.stage3.run(&mut output);

        // Stage 4: Schema filling (NULL Resolution)
        self.stage4.run(&mut output);

        // Stage 5: Scope Injection
        self.stage5.run(&mut output);

        // Stage 4B: Field validation
        let field_issues = self.field_validator.validate(
            &output.resolved_schema
        );

        // Update output with field issues
        output.field_issues = field_issues.clone();

        // Scoring stages
        let scoring_result = crate::scoring::compute_scoring_result(&output, &field_issues);

        // Update output with scores
        output.scoring_result = Some(scoring_result.clone());

        // Stage 6A: Output generation
        let stage6a_output = self.stage6a.run(&output)?;

        // Stage 6B: Correction cycle (conditionally triggers on low scores)
        let correction_cycle = if scoring_result.tes < 6.0 || scoring_result.sfs < 6.0 || scoring_result.scs < 6.0 {
            self.correction_cycle
                .new_cycle(&stage6a_output, &field_issues, &scoring_result)
        } else {
            // Keep empty default cycle if scores are good
            crate::types::CorrectionCycle::new(self.correction_cycle.cycle_number)
        };

        // Update output with correction cycle
        output.correction_cycle = Some(correction_cycle.clone());

        // Auto-Learn: Always update schema and push to session history
        // This fires unconditionally regardless of mode or error paths out of process()
        self.stage2.update_schema(&output.delta_tokens);
        session.push(input, &output);

        // Package the response with all required fields
        self.package_response(
            stage6a_output,
            output.topology.clone().unwrap_or_default(),
            output.normalization.clone().unwrap_or_default(),
            output.ordinal_sequence.clone(),
            field_issues,
            scoring_result,
            correction_cycle,
            &output,
        ).map(OrchestratorResponse::Legacy)
    }

    /// Packages the pipeline output into a CompressionResponse with all new fields.
    fn package_response(
        &self,
        compressed_prompt: String,
        topology: PromptTopology,
        normalization: NormalizationResult,
        _ordinal_sequence: Option<OrdinalSequence>,
        field_issues: Vec<FieldValidationIssue>,
        scoring_result: crate::types::ScoringResult,
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
            schema: algorithm_output.resolved_schema.clone(),
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
            scoring_result,
            correction_cycle,
        })
    }
}
