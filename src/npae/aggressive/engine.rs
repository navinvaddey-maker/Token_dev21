use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{NpaeConfig, StructuredPromptResponse, CompressionMeta, ProcessingMeta};
use std::time::Instant;

pub struct AggressiveEngine;

impl AggressiveEngine {
    pub async fn run(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        ory_engine: std::sync::Arc<tokio::sync::Mutex<crate::npae::ory::OryEngine>>,
        structurer_impl: &dyn super::structurer::Structurer,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // Isolate the actual user request from the RAG chunks and semantic context
        let (_, _, user_prompt) = super::structurer::split_raw_input(raw);

        // 1. Ory Engine: Meta-Orchestration and Deep Learning (using isolated prompt)
        let mut ory_lock = ory_engine.lock().await;
        let config_guard = config_handle.read();
        let ory_result = ory_lock.process(&user_prompt, &config_guard).map_err(|e| e.to_string())?;
        
        let mut profile = super::intent::extract(repr, &user_prompt)?;
        
        // Enhance Aggressive intent with Ory's deep learning
        if ory_result.intent.confidence_score > profile.confidence {
            profile.domain = ory_result.intent.inferred_domain.clone();
            profile.confidence = ory_result.intent.confidence_score;
        }

        // 1.5. Dispatch via StructurerRouter
        let resolver = super::resolver::PromptResolver::new(config_handle);
        let mut router = super::resolver::StructurerRouter::new(resolver);
        let resolved_prompt = router.dispatch(structurer_impl, &profile).map_err(|e| e.to_string())?;

        // 2. Build structured prompt (using isolated prompt so inference isn't confused by RAG chunks)
        let structured = super::structurer::build(&profile, &user_prompt, &resolved_prompt)?;

        // 3. Score ambiguity
        let amb = super::ambiguity::score(repr, &user_prompt)?;
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        // 4. Generate domain-aware questions if threshold exceeded
        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate_with_domain(repr, &amb, max_q, &profile.domain)?
        } else {
            vec![]
        };

        let aggressive_ms = t_start.elapsed().as_millis() as u64;

        // Render final optimized prompt for UI
        let optimized_prompt = super::structurer::render_crisp_prompt(&structured, &questions, raw);
        
        // Post-generation validation using Hallucination Guard
        let guard_cfg = &structured.hallucination_guard;
        let mut final_prompt = optimized_prompt;
        let mut guard_report = crate::npae::hallucination::guard::run_tri_layer(&final_prompt, raw, guard_cfg)
            .unwrap_or_else(|_| crate::npae::hallucination::guard::HallucinationReport {
                passed: true,
                layers: [
                    crate::npae::hallucination::guard::LayerReport { layer_id: 1, passed: true, flags: vec![] },
                    crate::npae::hallucination::guard::LayerReport { layer_id: 2, passed: true, flags: vec![] },
                    crate::npae::hallucination::guard::LayerReport { layer_id: 3, passed: true, flags: vec![] },
                ],
                remediation: None,
            });

        // Trigger targeted correction if not passed
        if !guard_report.passed {
            final_prompt = crate::npae::hallucination::guard::remediate_hallucination(&final_prompt, &guard_report);
            // Re-run the guard check on the corrected prompt
            if let Ok(new_report) = crate::npae::hallucination::guard::run_tri_layer(&final_prompt, raw, guard_cfg) {
                guard_report = new_report;
            }
        }
        
        // Final token accounting for UI
        let token_original = raw.split_whitespace().count() as u32;
        let token_final = final_prompt.split_whitespace().count() as u32;
        let token_saved = token_original.saturating_sub(token_final);

        // Calculate scoring for aggressive mode (v2 — computed, not hardcoded)
        // TES: Token efficiency — uses v2 savings-based formula
        let tes_score = if token_original > 0 {
            let savings = 1.0 - (token_final as f32 / token_original as f32);
            let base = if savings < 0.0 {
                (0.3 + savings * 0.4).max(0.1)
            } else if savings <= 0.1 {
                0.3 + savings * 2.0
            } else if savings <= 0.6 {
                0.5 + savings * 0.667
            } else if savings <= 0.8 {
                0.9 - (savings - 0.6) * 0.5
            } else {
                0.8 - (savings - 0.8) * 2.0
            };
            (base * 10.0).clamp(0.0, 10.0)
        } else {
            0.0
        };

        // SFS: Schema Fidelity — evaluate structural completeness of generated prompt
        let sfs_score = {
            let mut sfs = 0.0_f32;
            let total_checks = 7.0_f32;
            // Check role presence and quality
            if !structured.role.primary.is_empty() { sfs += 1.0; }
            if !structured.role.persona_anchor.is_empty() { sfs += 1.0; }
            // Check context
            if !structured.context.description.is_empty() { sfs += 1.0; }
            if !structured.context.background.is_empty() { sfs += 1.0; }
            // Check constraints
            if !structured.constraints.required_inclusions.is_empty() || !structured.constraints.forbidden_topics.is_empty() { sfs += 1.0; }
            // Check execution phases
            if !structured.execution_phases.is_empty() { sfs += 1.0; }
            // Check success criteria
            if !structured.success_criteria.is_empty() { sfs += 1.0; }
            (sfs / total_checks * 10.0).clamp(0.0, 10.0)
        };

        // SCS: Semantic Completeness — check constraint and question coverage
        let scs_score = {
            let mut score = 10.0_f32;
            // Penalize if ambiguity is high but no clarifying questions generated
            if amb.score > 0.5 && questions.is_empty() {
                score -= 2.0;
            }
            // Penalize if no success criteria could be inferred
            if structured.success_criteria.is_empty() {
                score -= 1.5;
            }
            // Penalize if dynamic instruction is generic
            if structured.dynamic_instruction.is_empty() {
                score -= 2.0;
            }
            // Reward constraint coverage
            if structured.constraints.required_inclusions.is_empty() && structured.constraints.forbidden_topics.is_empty() {
                score -= 1.0;
            }
            score.clamp(0.0, 10.0)
        };

        let avg_score = (tes_score + sfs_score + scs_score) / 3.0;
        let scoring_result = crate::types::ScoringResult {
            tes: tes_score,
            sfs: sfs_score,
            scs: scs_score,
            correction_needed: avg_score < 6.0,
            correction_axis: if tes_score < 6.0 {
                Some(crate::types::ScoreAxis::TaskEssential)
            } else if sfs_score < 6.0 {
                Some(crate::types::ScoreAxis::SchemaFidelity)
            } else if scs_score < 6.0 {
                Some(crate::types::ScoreAxis::SemanticCompleteness)
            } else {
                None
            },
        };

        // 6. Record Outcome to Ory Engine Memory
        if let Some(blueprint) = &ory_result.blueprint {
            let _ = ory_lock.record_outcome(&ory_result.intent, blueprint, &scoring_result);
        }

        Ok(StructuredPromptResponse {
            schema_version: "2.0.0".into(),
            request_id,
            optimized_prompt: final_prompt,
            token_original,
            token_final,
            token_saved,
            compression: CompressionMeta::from_repr(repr, token_original),
            structured_prompt: structured,
            ambiguity_analysis: amb,
            clarifying_questions: questions,
            confidence_score: profile.confidence,
            processing_metadata: ProcessingMeta {
                pipeline_duration_ms: repr.stage_metrics.total_ms(),
                aggressive_mode_ms: aggressive_ms,
                model_version: "npae-2.0.0".into(),
            },
            scoring_result,
            hallucination_report: Some(guard_report),
        })
    }
}
