use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{NpaeConfig, StructuredPromptResponse, CompressionMeta, ProcessingMeta};
use std::time::Instant;

pub struct AggressiveEngine;

impl AggressiveEngine {
    pub fn run(
        raw: &str,
        repr: &CompressedRepr,
        cfg: &NpaeConfig,
    ) -> Result<StructuredPromptResponse, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let t_start = Instant::now();

        // 1. Extract intent
        let profile = super::intent::extract(repr, raw)?;

        // 2. Build structured prompt
        let structured = super::structurer::build(&profile, raw)?;

        // 3. Score ambiguity
        let amb = super::ambiguity::score(repr, raw)?;
        let threshold = cfg.ambiguity_threshold.unwrap_or(0.65);

        // 4. Generate questions if threshold exceeded
        let questions = if amb.score > threshold {
            let max_q = cfg.max_questions.unwrap_or(3).min(3);
            super::questions::generate(repr, &amb, max_q)?
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

        Ok(StructuredPromptResponse {
            schema_version: "1.0.0".into(),
            request_id,
            optimized_prompt,
            token_original,
            token_final,
            token_saved,
            compression: CompressionMeta::from(repr),
            structured_prompt: structured,
            ambiguity_analysis: amb,
            clarifying_questions: questions,
            confidence_score: profile.confidence,
            processing_metadata: ProcessingMeta {
                pipeline_duration_ms: repr.stage_metrics.total_ms(),
                aggressive_mode_ms: aggressive_ms,
                model_version: "npae-1.0.0".into(),
            },
        })
    }
}
