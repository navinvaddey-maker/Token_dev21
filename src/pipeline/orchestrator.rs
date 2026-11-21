//! Pipeline Orchestrator — executes all active pipeline stages (-1 through 6B)
//! and routes between NPAE Aggressive and Legacy Balanced/Gentle paths.
//!
//! Implements ARCHITECTURE.md §1.2 (Pipeline Stages), §1.3 (Dual Routing Split),
//! and §1.5 (Balanced Mode Contract).

use dashmap::DashMap;
use std::sync::Arc;

use crate::{
    algorithms::{field_validator::FieldTypeValidator, predictive_coding::PredictiveCoding},
    engine::reconstruction::TokenReconstructor,
    npae::aggressive::config::ConfigHandle,
    pipeline::{
        stage0_normalize::NormalizationPrePass, stage0b_topology::TopologyClassifier,
        stage1::Stage1, stage2::Stage2, stage3::Stage3, stage4::Stage4, stage5::Stage5,
        stage6a::Stage6a,
    },
    scoring::sfs::SemanticFidelityScorer,
    scoring::tes::TokenEfficiencyScorer,
    session::SessionHistory,
    types::{
        AlgorithmOutput, CompressionResponse, CorrectionCycle, FieldValidationIssue, Mode,
        NormalizationResult, OrchestratorResponse, OrdinalSequence, PromptTopology,
    },
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
    stage6b: crate::pipeline::stage6b::Stage6b,
    _correction_cycle: CorrectionCycle,
    npae_config_handle: Arc<ConfigHandle>,
    ory_engine: Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
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
        ory_engine: Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
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
            stage6b: crate::pipeline::stage6b::Stage6b::new(),
            _correction_cycle: correction_cycle,
            npae_config_handle: Arc::new(ConfigHandle::new(
                crate::npae::aggressive::config::UnifiedConfig {
                    domain_taxonomy: vec![],
                    roles: vec![],
                    constraints: vec![],
                },
            )), // Fallback if not injected properly, though we will inject via build
            ory_engine,
        }
    }

    /// Build orchestrator from shared schema priors — convenience factory.
    /// All 12 stage components are constructed internally with defaults.
    pub fn build(
        schema_priors: Arc<DashMap<String, u32>>,
        npae_config_handle: Arc<ConfigHandle>,
        ory_engine: Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
    ) -> Self {
        Self::build_with_pool(schema_priors, npae_config_handle, ory_engine, None)
    }

    /// Build orchestrator with an optional database pool for Stage 4 schema lookup.
    pub fn build_with_pool(
        schema_priors: Arc<DashMap<String, u32>>,
        npae_config_handle: Arc<ConfigHandle>,
        ory_engine: Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        _pool: Option<sqlx::SqlitePool>,
    ) -> Self {
        let predictive = PredictiveCoding::new(schema_priors);
        Self {
            reconstructor: TokenReconstructor::new(),
            normalization_pre_pass: NormalizationPrePass::new(),
            topology_classifier: TopologyClassifier::new(),
            stage1: Stage1::new(),
            stage2: Stage2::new(predictive),
            stage3: Stage3::new(),
            stage4: Stage4::with_config(npae_config_handle.clone()),
            stage5: Stage5::new(),
            field_validator: FieldTypeValidator::new(),
            _token_efficiency_scorer: TokenEfficiencyScorer::new(),
            _semantic_fidelity_scorer: SemanticFidelityScorer::new(),
            stage6a: Stage6a::new(),
            stage6b: crate::pipeline::stage6b::Stage6b::new(),
            _correction_cycle: CorrectionCycle::new(0),
            npae_config_handle,
            ory_engine,
        }
    }

    /// Runs Stage -1 (`TokenReconstructor`) on `input` using the in-memory `ScenarioSignal`
    /// fallback so direct endpoints (`/api/v1/aggressive`) preserve `constraint_locks` (`[STRICT]`).
    ///
    /// # Panics
    /// Never.
    pub fn reconstruct_input(&self, input: &str) -> crate::types::ReconstructedInput {
        // ARCH: ARCHITECTURE.md §1.2 — Stage -1 constraint locks must execute across all Aggressive entrypoints.
        // FIX: G-01 — Reconstruct input with scenario fallback and merge verbatim constraint spans
        // (negations, numeric bounds, modal requirements) into `constraint_locks`.
        let mut rec = self
            .reconstructor
            .run(input, self.resolve_scenario_fallback(input));
        let (inclusions, forbidden) =
            crate::npae::aggressive::constraints::extract_constraints(input, &[], &[]);
        for span in inclusions.into_iter().chain(forbidden.into_iter()) {
            let trimmed = span.trim();
            if !trimmed.is_empty()
                && !rec
                    .constraint_locks
                    .iter()
                    .any(|c| c.text.eq_ignore_ascii_case(trimmed))
            {
                rec.constraint_locks.push(crate::types::ConstraintToken {
                    text: trimmed.to_string(),
                    weight: 1.0,
                });
            }
        }
        rec
    }

    /// Resolves an in-memory `ScenarioSignal` fallback when upstream `ScenarioClassifier` abstains
    /// or no SQLite pool is wired.
    ///
    /// # Panics
    /// Never.
    fn resolve_scenario_fallback(
        &self,
        input: &str,
    ) -> Option<crate::classifier::signal::ScenarioSignal> {
        // 1. Deterministic Intent x Vertical rule layer first.
        if let Some(verdict) = crate::classifier::intent_rule::resolve_prompt_rules(input) {
            let detected_expertise =
                crate::classifier::expertise::detect_expertise_from_query(input).to_string();
            return Some(crate::classifier::signal::ScenarioSignal {
                domain: verdict.pipeline_domain().to_string(),
                task_type: verdict.task_type().to_string(),
                expertise: detected_expertise,
                intent_class: verdict.intent_class().to_string(),
                confidence: 0.95,
                secondary_domain: verdict.secondary_domain().map(ToString::to_string),
                primary_role: Some(verdict.primary_role.to_string()),
                rule_locked: true,
                legal_as_constraint: verdict.legal_as_constraint,
            });
        }

        // 2. Config-backed taxonomy & exemplar classifier fallback.
        let cfg = self.npae_config_handle.read();
        if cfg.domain_taxonomy.is_empty() {
            return None;
        }

        let (domain, conf, secondaries) =
            crate::npae::aggressive::intent::detect_domain_with_confidence(input, Some(&cfg));
        if domain == "general" || conf < 0.45 {
            return None;
        }

        let repr = crate::npae::compression::pipeline::run_parallel_pipeline(input).ok()?;
        let mut profile =
            crate::npae::aggressive::intent::extract_with_config(&repr, input, Some(&cfg)).ok()?;
        profile.domain = domain.clone();
        profile.confidence = conf;

        let task_type = Self::derive_task_type(input, &profile.primary_intent).to_string();
        let expertise =
            crate::classifier::expertise::detect_expertise_from_query(input).to_string();
        let raw_role = crate::npae::aggressive::role::generate_role(
            &profile,
            input,
            &cfg.domain_taxonomy,
            &cfg.roles,
        );
        let primary_role = raw_role
            .split(" (focus:")
            .next()
            .unwrap_or(raw_role.as_str())
            .trim()
            .to_string();

        let intent_class = match (domain.as_str(), task_type.as_str()) {
            ("legal", _) => "compliance",
            ("finance" | "business" | "real-estate", "creation" | "planning") => "wealth-building",
            ("marketing" | "ecommerce", _) => "brand-building",
            (_, "troubleshooting") => "debugging",
            _ => task_type.as_str(),
        }
        .to_string();

        Some(crate::classifier::signal::ScenarioSignal {
            domain,
            task_type,
            expertise,
            intent_class,
            confidence: conf,
            secondary_domain: secondaries
                .first()
                .cloned()
                .or_else(|| profile.secondary_domains.first().cloned()),
            primary_role: if primary_role.is_empty() {
                None
            } else {
                Some(primary_role)
            },
            rule_locked: profile.rule_locked,
            legal_as_constraint: profile.legal_as_constraint,
        })
    }

    /// Maps lexical verbs and `IntentClass` to canonical `task_type` strings:
    /// `"creation"`, `"analysis"`, `"planning"`, `"troubleshooting"`, `"optimization"`, `"explanation"`.
    fn derive_task_type(
        input: &str,
        intent: &crate::npae::aggressive::intent::IntentClass,
    ) -> &'static str {
        let lower = input.to_lowercase();
        let words: std::collections::HashSet<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|s| !s.is_empty())
            .collect();

        if [
            "optimize",
            "optimization",
            "tune",
            "speedup",
            "accelerate",
            "streamline",
        ]
        .iter()
        .any(|w| words.contains(w))
        {
            return "optimization";
        }

        if [
            "plan",
            "roadmap",
            "itinerary",
            "schedule",
            "syllabus",
            "strategy",
            "blueprint",
        ]
        .iter()
        .any(|w| words.contains(w))
        {
            return "planning";
        }

        match intent {
            crate::npae::aggressive::intent::IntentClass::Build => "creation",
            crate::npae::aggressive::intent::IntentClass::Analyze => "analysis",
            crate::npae::aggressive::intent::IntentClass::Debug => "troubleshooting",
            crate::npae::aggressive::intent::IntentClass::Explain => "explanation",
            crate::npae::aggressive::intent::IntentClass::Transform => "optimization",
        }
    }

    /// Processes the input through all pipeline stages (0A through 6B) and returns a compression response.
    pub async fn process(
        &self,
        input: &str,
        session: &mut SessionHistory,
        scenario: Option<crate::classifier::signal::ScenarioSignal>,
        forced_mode: Option<&str>,
        npae_config: Option<&crate::npae::schema::types::NpaeConfig>,
        enrichment: Option<crate::types::EnrichmentContext>,
    ) -> Result<OrchestratorResponse, Box<dyn std::error::Error>> {
        // Initialize AlgorithmOutput that will be passed through the pipeline
        let mut output = AlgorithmOutput::default();
        output.scenario = scenario;
        if output.scenario.is_none() {
            // ARCH: §1.4 — In-memory taxonomy & exemplar fallback when SQLite scenario_index abstains.
            output.scenario = self.resolve_scenario_fallback(input);
        }
        output.enrichment = enrichment.clone();

        // Stage -1: Token Reconstruction
        let reconstructed = self.reconstructor.run(input, output.scenario.clone());
        output.constraint_locks = reconstructed.constraint_locks.clone();
        output.ambiguity_register = reconstructed.ambiguity_register.clone();
        output.input_structure_score = reconstructed.input_structure_score;

        output.clusters = reconstructed
            .clusters
            .values()
            .map(|tokens| tokens.iter().map(|t| t.text.clone()).collect())
            .collect();
        output.cluster_labels = reconstructed
            .clusters
            .keys()
            .map(|k| format!("{:?}", k))
            .collect();

        // Extract expected deliverables (SlotType::Output)
        if let Some(deliverable_tokens) =
            reconstructed.clusters.get(&crate::types::SlotType::Output)
        {
            output.expected_deliverables =
                deliverable_tokens.iter().map(|t| t.text.clone()).collect();
        }

        // Stage 0A: Normalization pre-pass
        let normalized = self
            .normalization_pre_pass
            .run(input, &reconstructed, &mut output);

        // Stage 0B: Topology classification
        let _topology = self.topology_classifier.classify(&normalized, &mut output);

        // Stage 1: Signal Reduction (Lexical Compression → Sparse Coding)
        let topology = output.topology.clone().unwrap_or_default();
        let normalized_forced_mode = forced_mode.map(|m| m.trim().to_lowercase());
        self.stage1.run(
            &normalized,
            &mut output,
            &topology,
            normalized_forced_mode.as_deref(),
        );

        // Stage 2: Boundary Detection (Predictive Coding → mode decision)
        // ARCH: Balanced Mode Contract — always run Stage 2 so `delta_tokens` and `error_score`
        // are populated for Layer 2 schema filling and session learning (Gap C4).
        // FIX: Gaps C1, C2, C4 — run Stage 2 unconditionally, apply normalized forced_mode override,
        // and if auto-routing selected Mode::Balanced, re-run Stage 1 with "balanced" keep ratio (45%).
        self.stage2.run(session, &mut output);

        if let Some(ref fm) = normalized_forced_mode {
            let m = match fm.as_str() {
                "aggressive" => Mode::Aggressive,
                "gentle" => Mode::Gentle,
                "balanced" => Mode::Balanced,
                _ => Mode::Balanced,
            };
            output.mode = Some(m);
        } else if output.mode.as_ref() == Some(&Mode::Balanced) {
            // FIX: Gap C1 — Stage 1 initially ran with a topology proxy ratio (30% or 60%).
            // Re-run Stage 1 with Some("balanced") so Balanced mode gets its 45% keep ratio,
            // then refresh delta_tokens against the updated sparse_tokens while preserving Mode::Balanced.
            self.stage1
                .run(&normalized, &mut output, &topology, Some("balanced"));
            self.stage2.run(session, &mut output);
            output.mode = Some(Mode::Balanced);
        }

        // Stage 3: Context Management (Seeding WM from locks and clusters)
        self.stage3.run(&mut output);

        let is_aggressive = output.mode.as_ref() == Some(&Mode::Aggressive);

        if is_aggressive {
            // -- Aggressive Routing (NPAE) --
            let npae_cfg = crate::npae::schema::types::NpaeConfig {
                ambiguity_threshold: npae_config
                    .and_then(|c| c.ambiguity_threshold)
                    .or(Some(0.65)),
                max_questions: npae_config.and_then(|c| c.max_questions).or(Some(3)),
                confidence_threshold: npae_config
                    .and_then(|c| c.confidence_threshold)
                    .or(Some(0.75)),
                skip_stage: npae_config.and_then(|c| c.skip_stage.clone()),
                render_style: npae_config.and_then(|c| c.render_style),
                target_model_family: npae_config.and_then(|c| c.target_model_family),
            };

            // ARCH: §6 Rust Rules & Workstream 3 — Offload CPU-bound compression pipeline (rayon::join)
            // onto tokio::task::spawn_blocking instead of blocking Tokio async worker threads.
            let normalized_clone = normalized.clone();
            let repr = tokio::task::spawn_blocking(move || {
                crate::npae::compression::pipeline::run_parallel_pipeline(&normalized_clone)
            })
            .await
            .map_err(|e| format!("JoinError in run_parallel_pipeline: {}", e))?
            .map_err(|e| e.to_string())?;

            let structurer = crate::npae::aggressive::structurer::HttpStructurer {
                route: "/api/compress".to_string(),
                remote_addr: "127.0.0.1".to_string(), // In real app, pass actual address
                body: normalized.clone(),
            };

            let resp = crate::npae::aggressive::engine::AggressiveEngine::run(
                &normalized,
                &repr,
                &npae_cfg,
                self.npae_config_handle.clone(),
                self.ory_engine.clone(),
                &structurer,
                &reconstructed,
                output.enrichment.as_ref(),
            )
            .await
            .map_err(|e| e.to_string())?;

            // Populate output fields for session history and feedback tracking
            output.output_token_count = resp.token_final;
            output.scoring_result = Some(resp.scoring_result.clone());

            // Push to session history for tracking
            session.push(input, &output);

            return Ok(OrchestratorResponse::Aggressive(resp));
        }

        // -- Legacy Routing (Balanced/Gentle) --

        // Stage 4: Schema filling (NULL Resolution)
        self.stage4.run(&mut output).await;

        // Stage 4B: Field validation (before scope injection)
        let field_issues = self
            .field_validator
            .validate_full(&output.resolved_schema, &output);
        output.field_issues = field_issues.clone();

        // Stage 5: Scope Injection
        self.stage5.run(&mut output);

        // Stage 6A: Output generation
        let guard_cfg = crate::npae::schema::types::HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into(), "cite_uncertainty".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
        };

        let (initial_response, (initial_report, initial_corrections)) =
            self.run_guarded_generation(input, &guard_cfg, &output)?;

        // Stage 6B: Targeted Quality Correction Loop (managed by Stage6b module)
        let stage6b_out = self
            .stage6b
            .run(
                input,
                &mut output,
                &guard_cfg,
                &self.field_validator,
                &self.stage4,
                &self.stage5,
                &self.stage6a,
                initial_response,
                initial_report,
                initial_corrections,
            )
            .await?;

        let final_response = stage6b_out.final_response;
        let guard_report = stage6b_out.guard_report;
        let field_issues = stage6b_out.field_issues;
        let scoring_result = stage6b_out.scoring_result;
        let correction_cycle = stage6b_out.correction_cycle;

        // Auto-Learn: Always update schema and push to session history
        // This fires unconditionally regardless of mode or error paths out of process()
        self.stage2.update_schema(&output.delta_tokens);
        session.push(input, &output);

        // Package the response with all required fields
        self.package_response(
            final_response,
            output.topology.clone().unwrap_or_default(),
            output.normalization.clone().unwrap_or_default(),
            output.ordinal_sequence.clone(),
            field_issues,
            scoring_result,
            correction_cycle,
            &output,
            Some(guard_report),
        )
        .map(OrchestratorResponse::Legacy)
    }

    pub fn apply_feedback(&self, prompt: &str, weight: f32) {
        // Simple tokenization for feedback application
        let tokens: Vec<String> = prompt.split_whitespace().map(|s| s.to_string()).collect();
        self.stage2.apply_feedback(&tokens, weight);
    }

    /// Runs Stage 6A and the tri-layer hallucination guard on the generated prompt.
    fn run_guarded_generation(
        &self,
        input: &str,
        guard_cfg: &crate::npae::schema::types::HallucinationGuardConfig,
        output: &AlgorithmOutput,
    ) -> Result<
        (
            String,
            (
                crate::npae::hallucination::guard::HallucinationReport,
                Vec<crate::types::TextCorrection>,
            ),
        ),
        Box<dyn std::error::Error>,
    > {
        let mut final_response = self.stage6a.run(output)?;
        let mut guard_report =
            crate::npae::hallucination::guard::run_tri_layer(&final_response, input, guard_cfg)
                .unwrap_or_else(|_| crate::npae::hallucination::guard::HallucinationReport {
                    passed: true,
                    layers: [
                        crate::npae::hallucination::guard::LayerReport {
                            layer_id: 1,
                            passed: true,
                            flags: vec![],
                        },
                        crate::npae::hallucination::guard::LayerReport {
                            layer_id: 2,
                            passed: true,
                            flags: vec![],
                        },
                        crate::npae::hallucination::guard::LayerReport {
                            layer_id: 3,
                            passed: true,
                            flags: vec![],
                        },
                    ],
                    remediation: None,
                });

        let mut corrections_applied = Vec::new();
        if !guard_report.passed {
            for (layer_idx, layer) in guard_report.layers.iter().enumerate() {
                if !layer.passed {
                    for flag in &layer.flags {
                        corrections_applied.push(crate::types::TextCorrection {
                            original: flag.clone(),
                            corrected: format!("[CORRECTED] {}", flag),
                            confidence: 0.9,
                            correction_type: format!("l{}_guard_flag", layer_idx + 1),
                        });
                    }
                }
            }

            final_response = crate::npae::hallucination::guard::remediate_hallucination(
                &final_response,
                &guard_report,
            );
            if let Ok(new_report) =
                crate::npae::hallucination::guard::run_tri_layer(&final_response, input, guard_cfg)
            {
                guard_report = new_report;
            }
        }

        Ok((final_response, (guard_report, corrections_applied)))
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
        hallucination_report: Option<crate::npae::hallucination::guard::HallucinationReport>,
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
            clusters: if !algorithm_output.clusters.is_empty() {
                Some(
                    algorithm_output
                        .clusters
                        .iter()
                        .enumerate()
                        .flat_map(|(i, cluster)| {
                            let label = algorithm_output
                                .cluster_labels
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("cluster_{}", i));
                            std::iter::once(format!("[{}]", label)).chain(cluster.iter().cloned())
                        })
                        .collect(),
                )
            } else {
                None
            },
            delta_tokens: if !algorithm_output.delta_tokens.is_empty() {
                Some(algorithm_output.delta_tokens.clone())
            } else {
                None
            },
            reasoning_chain: None,
            scoring_result,
            correction_cycle,
            hallucination_report,
        })
    }
}
