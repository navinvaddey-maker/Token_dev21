use serde::Serialize;
use tracing::info;
use super::{
    sparse_coding,
    chunking,
    selective_attention,
    predictive_coding,
    working_memory,
    hebbian_binding,
};

#[derive(Debug, Clone)]
pub struct PipelineInput {
    pub raw_text:           String,
    pub task:               String,
    pub use_case:           String,
    pub mode:               String,
    pub max_tokens:         usize,
    pub engine_version:     String,
    pub protected_entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineOutput {
    pub optimized_prompt: String,
    pub token_original:   usize,
    pub token_final:      usize,
    pub token_saved:      usize,
    pub use_case:         String,
    pub mode:             String,
    pub engine_version:   String,
    pub principle_logs:   Vec<PrincipleLog>,
    pub warnings:         Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrincipleLog {
    pub principle:     String,
    pub items_removed: usize,
    pub detail:        String,
    pub duration_ms:   u64,
}

/// Simple word-based token estimation (word_count * 1.3)
fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f64 * 1.3).ceil() as usize
}

pub fn run(input: &PipelineInput) -> Result<PipelineOutput, String> {
    let mut logs: Vec<PrincipleLog> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Validate input
    if input.raw_text.trim().is_empty() {
        return Err("raw_text must not be empty".into());
    }
    if input.task.trim().is_empty() {
        return Err("task must not be empty".into());
    }

    let word_count = input.raw_text.split_whitespace().count();
    if word_count > 8000 {
        return Err("Input exceeds 8000 words".into());
    }

    let token_original = estimate_tokens(&input.raw_text);

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
    info!(principle = "chunking", chunks = s2.chunks.len(), ms = s2.duration_ms);
    logs.push(PrincipleLog {
        principle: "chunking".into(),
        items_removed: s2.items_removed,
        detail: s2.detail.clone(),
        duration_ms: s2.duration_ms,
    });

    // ── Stage 3: Selective attention — keep relevant chunks ───────────────
    let s3 = selective_attention::run(&s2.chunks, &input.task, &input.mode, &input.protected_entities);
    info!(principle = "selective_attention", kept = s3.chunks.len(), dropped = s3.items_removed, ms = s3.duration_ms);
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
    let s4 = predictive_coding::run(&s3.chunks, &input.task, &input.use_case);
    info!(principle = "predictive_coding", ms = s4.duration_ms);
    logs.push(PrincipleLog {
        principle: "predictive_coding".into(),
        items_removed: s4.items_removed,
        detail: s4.detail.clone(),
        duration_ms: s4.duration_ms,
    });

    // ── Stage 5: Working memory — dedup + budget ──────────────────────────
    let s5 = working_memory::run(&s4.text, input.max_tokens);
    info!(principle = "working_memory", deduped = s5.items_removed, ms = s5.duration_ms);
    logs.push(PrincipleLog {
        principle: "working_memory".into(),
        items_removed: s5.items_removed,
        detail: s5.detail.clone(),
        duration_ms: s5.duration_ms,
    });

    // ── Stage 6: Hebbian binding — CRISP reconstruction ───────────────────
    let s6 = hebbian_binding::run(&s5.text, &input.task, &input.use_case);
    info!(principle = "hebbian_binding", ms = s6.duration_ms, detail = %s6.detail);
    logs.push(PrincipleLog {
        principle: "hebbian_binding".into(),
        items_removed: s6.items_removed,
        detail: s6.detail.clone(),
        duration_ms: s6.duration_ms,
    });

    let token_final = estimate_tokens(&s6.text);
    let token_saved = token_original.saturating_sub(token_final);

    info!(
        token_original = token_original,
        token_final    = token_final,
        token_saved    = token_saved,
        message        = "compress.done"
    );

    Ok(PipelineOutput {
        optimized_prompt: s6.text,
        token_original,
        token_final,
        token_saved,
        use_case:       input.use_case.clone(),
        mode:           input.mode.clone(),
        engine_version: input.engine_version.clone(),
        principle_logs: logs,
        warnings,
    })
}
