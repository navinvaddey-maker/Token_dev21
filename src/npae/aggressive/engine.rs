use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{NpaeConfig, StructuredPromptResponse, CompressionMeta, ProcessingMeta};
use std::time::Instant;

pub struct AggressiveEngine;

impl AggressiveEngine {
    pub fn run(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
        config_handle: std::sync::Arc<super::config::ConfigHandle>,
        structurer_impl: &dyn super::structurer::Structurer,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // 1. Ory Engine: Meta-Orchestration and Deep Learning
        let mut ory_engine = crate::npae::ory::OryEngine::new();
        let config_guard = config_handle.read();
        let ory_result = ory_engine.process(raw, &config_guard).map_err(|e| e.to_string())?;
        drop(config_guard); // Release lock
        
        let mut profile = super::intent::extract(repr, raw)?;
        
        // Enhance Aggressive intent with Ory's deep learning
        if ory_result.intent.confidence_score > profile.confidence {
            profile.domain = ory_result.intent.inferred_domain.clone();
            profile.confidence = ory_result.intent.confidence_score;
        }

        // 1.5. Dispatch via StructurerRouter
        let resolver = super::resolver::PromptResolver::new(config_handle);
        let mut router = super::resolver::StructurerRouter::new(resolver);
        let resolved_prompt = router.dispatch(structurer_impl, &profile).map_err(|e| e.to_string())?;

        // 2. Build structured prompt (now with execution phases, validation, constraints_meta)
        let structured = super::structurer::build(&profile, raw, &resolved_prompt)?;

        // 3. Score ambiguity
        let amb = super::ambiguity::score(repr, raw)?;
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
        let optimized_prompt = super::structurer::render_crisp_prompt(&structured, &questions);
        
        // Final token accounting for UI
        let token_original = raw.split_whitespace().count() as u32;
        let token_final = optimized_prompt.split_whitespace().count() as u32;
        let token_saved = token_original.saturating_sub(token_final);

        // Calculate scoring for aggressive mode
        let tes_score = if token_original > 0 {
            let ratio = token_final as f32 / token_original as f32;
            (ratio * 10.0).clamp(0.0, 10.0)
        } else {
            0.0
        };
        let scoring_result = crate::types::ScoringResult {
            tes: tes_score,
            sfs: 10.0, // NPAE structurally enforces these fields
            scs: 10.0,
            correction_needed: false,
            correction_axis: None,
        };

        // 6. Record Outcome to Ory Engine Memory
        if let Some(blueprint) = &ory_result.blueprint {
            let _ = ory_engine.record_outcome(&ory_result.intent, blueprint, &scoring_result);
        }

        Ok(StructuredPromptResponse {
            schema_version: "2.0.0".into(),
            request_id,
            optimized_prompt,
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
        })
    }
}
