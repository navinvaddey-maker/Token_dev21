use super::{
    chunking,
    evaluation::{self, EvaluationMetrics},
    predictive_coding, selective_attention, sparse_coding, working_memory,
};
use crate::utils::tokens::estimate_tokens;
use serde::Serialize;
use tracing::info;

#[derive(Debug, Clone)]
pub struct PipelineInput {
    pub raw_text: String,
    pub task: String,

    pub model: String,
    pub use_case: String,
    pub mode: String,
    pub max_tokens: usize,
    pub engine_version: String,
    pub protected_entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineOutput {
    pub optimized_prompt: String,
    pub token_original: usize,
    pub token_final: usize,
    pub token_saved: usize,
    pub use_case: String,
    pub mode: String,
    pub engine_version: String,
    pub principle_logs: Vec<PrincipleLog>,
    pub warnings: Vec<String>,
    pub evaluation: EvaluationMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrincipleLog {
    pub principle: String,
    pub items_removed: usize,
    pub detail: String,
    pub duration_ms: u64,
}

pub fn run(input: &PipelineInput) -> Result<PipelineOutput, String> {
    let mut logs: Vec<PrincipleLog> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Validate input
    if input.raw_text.trim().is_empty() {
        return Err("raw_text must not be empty".into());
    }
    // Task is optional from the API, so we don't return an error if it's empty.
    let task_safe = if input.task.trim().is_empty() {
        "Auto-optimize prompt".to_string()
    } else {
        input.task.clone()
    };

    let word_count = input.raw_text.split_whitespace().count();
    if word_count > 8000 {
        return Err("Input exceeds 8000 words".into());
    }

    let token_original = estimate_tokens(&input.raw_text) as usize;

    // ── Stage 1: Sparse coding — noise removal ────────────────────────────
    let s1 = sparse_coding::run(&input.raw_text, &input.mode);
    info!(principle = "sparse_coding", removed = s1.items_removed, ms = s1.duration_ms, use_case = %input.use_case);
    logs.push(PrincipleLog {
        principle: "sparse_coding".into(),
        items_removed: s1.items_removed,
        detail: s1.detail.clone(),
        duration_ms: s1.duration_ms,
    });

    // ── Stage 2: Chunking — break into semantic units ─────────────────────
    let s2 = chunking::run(&s1.text, &input.use_case);
    info!(
        principle = "chunking",
        chunks = s2.chunks.len(),
        ms = s2.duration_ms
    );
    logs.push(PrincipleLog {
        principle: "chunking".into(),
        items_removed: s2.items_removed,
        detail: s2.detail.clone(),
        duration_ms: s2.duration_ms,
    });

    // ── Stage 3: Selective attention — keep relevant chunks ───────────────
    let s3 = selective_attention::run(
        &s2.chunks,
        &task_safe,
        &input.mode,
        &input.protected_entities,
    );
    info!(
        principle = "selective_attention",
        kept = s3.chunks.len(),
        dropped = s3.items_removed,
        ms = s3.duration_ms
    );
    logs.push(PrincipleLog {
        principle: "selective_attention".into(),
        items_removed: s3.items_removed,
        detail: s3.detail.clone(),
        duration_ms: s3.duration_ms,
    });

    if s3.chunks.is_empty() {
        warnings.push("All chunks were filtered — using fallback".into());
    }

    // ── Stage 4: Predictive coding — prepend role frame ───────────────────
    let s4 = predictive_coding::run(
        &s3.chunks,
        &task_safe,
        &input.use_case,
        &input.model,
    );
    info!(principle = "predictive_coding", ms = s4.duration_ms);
    logs.push(PrincipleLog {
        principle: "predictive_coding".into(),
        items_removed: s4.items_removed,
        detail: s4.detail.clone(),
        duration_ms: s4.duration_ms,
    });

    // ── Stage 5: Working memory — dedup + budget ──────────────────────────
    let s5 = working_memory::run(&s4.text, input.max_tokens);
    info!(
        principle = "working_memory",
        deduped = s5.items_removed,
        ms = s5.duration_ms
    );
    logs.push(PrincipleLog {
        principle: "working_memory".into(),
        items_removed: s5.items_removed,
        detail: s5.detail.clone(),
        duration_ms: s5.duration_ms,
    });

    let mut token_final = estimate_tokens(&s5.text) as usize;
    let mut optimized_prompt = s5.text;

    if token_final > token_original && input.model != "Claude" {
        warnings
            .push("Optimization increased token count — falling back to original prompt".into());
        optimized_prompt = input.raw_text.clone();
        token_final = token_original;
    } else if token_final > token_original {
        warnings.push("Note: Role assignment increased token count beyond original input".into());
    }

    let token_saved = token_original.saturating_sub(token_final);

    info!(
        token_original = token_original,
        token_final = token_final,
        token_saved = token_saved,
        message = "compress.done"
    );

    let eval = evaluation::evaluate(&input.raw_text, &optimized_prompt);

    Ok(PipelineOutput {
        optimized_prompt,
        token_original,
        token_final,
        token_saved,
        use_case: input.use_case.clone(),
        mode: input.mode.clone(),
        engine_version: input.engine_version.clone(),
        principle_logs: logs,
        warnings,
        evaluation: eval,
    })
}
