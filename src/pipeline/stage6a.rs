//! Stage 6A: Output Generation (`ARCHITECTURE.md` §1.2, §1.3).
//!
//! Renders the final compressed prompt from `AlgorithmOutput`, enforcing a budgeted
//! structured format for Balanced/Gentle modes and preserving all constraint locks
//! in both structured and compact fallback modes.

use crate::types::{AlgorithmOutput, Mode};
use crate::utils::tokens::estimate_tokens;

/// Handles Stage 6A: Output Generation.
#[derive(Debug)]
pub struct Stage6a;

impl Stage6a {
    /// Creates a new `Stage6a` instance.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Processes input through Stage 6A: Output Generation.
    ///
    /// Generates the final compressed prompt from the schema-filled data, respecting
    /// token budgets in Balanced/Gentle mode and guaranteeing constraint lock survival
    /// when `force_compact_generation` is active.
    ///
    /// # Errors
    /// Returns `Err` only if an internal formatting invariant fails (currently infallible).
    ///
    /// # Panics
    /// Never.
    pub fn run(
        &self,
        schema_filled: &AlgorithmOutput,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // ARCH: Balanced Mode Contract (A2) — compact generation must never drop constraint locks or negations.
        // FIX: Previously `force_compact_generation` joined `sparse_tokens` as a raw bag of words and
        // dropped `constraint_locks` / `resolved_schema.constraints`. Now missing locks are always appended.
        if schema_filled.force_compact_generation {
            let compact = Self::build_compact_with_constraints(schema_filled);
            if !compact.is_empty() {
                return Ok(compact);
            }
        }

        let is_budgeted_mode = matches!(
            schema_filled.mode,
            Some(Mode::Balanced) | Some(Mode::Gentle) | None
        );

        let sparse_joined = Self::join_sparse_or_clean(schema_filled);

        // ARCH: Balanced Mode Contract (A1, A4) — budgeted structured renderer for Balanced/Gentle.
        // FIX: Previously Stage 6A pasted the entire `user_prompt` into `**Task:**` plus verbose
        // boilerplate (`**Inferred Context:**`, `**Warnings:**`, `**Role:**`), always expanding tokens.
        if is_budgeted_mode {
            return Ok(Self::render_budgeted_structured(
                schema_filled,
                &sparse_joined,
            ));
        }

        // Non-budgeted fallback path (e.g., if Stage 6A is invoked directly for Aggressive/Ambiguous)
        let mut parts = Vec::new();

        if let Some(role) = &schema_filled.resolved_schema.role {
            parts.push(format!("**Role:** {}", role));
        }

        if let Some(norm) = &schema_filled.normalization {
            let (ctx_opt, rag_opt, user_prompt) =
                crate::npae::aggressive::structurer::split_raw_input(&norm.normalized_text);

            let task_text = if !sparse_joined.is_empty() && sparse_joined.len() < user_prompt.len()
            {
                sparse_joined.as_str()
            } else {
                user_prompt.as_str()
            };
            parts.push(format!("**Task:** {}", task_text));

            if let Some(ctx) = ctx_opt {
                parts.push(format!("**Semantic Context:** {}", ctx));
            }
            if let Some(rag) = rag_opt {
                let safe_rag = crate::npae::grounding::verifier::sanitize_untrusted_fact(&rag);
                parts.push(format!(
                    "**Source Knowledge:**\n<retrieved_context trust=\"untrusted\">\n{}\n</retrieved_context>",
                    safe_rag
                ));
            }

            if let Some(context) = &schema_filled.resolved_schema.context {
                if context.len() > 3 {
                    parts.push(format!("**Inferred Context:** {}", context));
                }
            }
        } else {
            let task_text = if !sparse_joined.is_empty() {
                Some(sparse_joined.as_str())
            } else {
                schema_filled.resolved_schema.task.as_deref()
            };
            if let Some(task) = task_text {
                parts.push(format!("**Task:** {}", task));
            }
            if let Some(context) = &schema_filled.resolved_schema.context {
                if context.len() > 3 {
                    parts.push(format!("**Context:** {}", context));
                }
            }
        }

        if let Some(rag_block) = Self::format_enrichment_rag(schema_filled, &parts) {
            parts.push(rag_block);
        }

        if !schema_filled.scope_injections.is_empty() {
            let scopes = schema_filled.scope_injections.join(", ");
            parts.push(format!("**Directives:** {}", scopes));
        }

        let all_constraints = Self::collect_missing_constraints(schema_filled, "");
        if !all_constraints.is_empty() {
            parts.push(format!("**Constraints:** {}", all_constraints.join(", ")));
        }

        if !schema_filled.resolved_schema.output.is_empty() {
            let deliverables = schema_filled
                .resolved_schema
                .output
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("**Deliverables:** {}", deliverables));
        }

        if !schema_filled.null_fields.is_empty() {
            let warnings = schema_filled.null_fields.join(", ");
            parts.push(format!("**Warnings:** Missing fields: {}", warnings));
        }

        let compressed_tokens = if !parts.is_empty() {
            parts.join("\n")
        } else if !sparse_joined.is_empty() {
            sparse_joined
        } else {
            "Please clarify your request.".to_string()
        };

        Ok(compressed_tokens)
    }

    /// Joins `sparse_tokens` if non-empty, otherwise falls back to `clean_tokens`.
    fn join_sparse_or_clean(schema_filled: &AlgorithmOutput) -> String {
        if !schema_filled.sparse_tokens.is_empty() {
            schema_filled
                .sparse_tokens
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            schema_filled.clean_tokens.join(" ")
        }
    }

    /// Collects all unique constraints from `constraint_locks` and `resolved_schema.constraints`
    /// that are not already present verbatim (case-insensitive) in `existing_text`.
    fn collect_missing_constraints(
        schema_filled: &AlgorithmOutput,
        existing_text: &str,
    ) -> Vec<String> {
        let lower_existing = existing_text.to_lowercase();
        let mut out: Vec<String> = Vec::new();

        let mut push_unique = |candidate: &str| {
            let trimmed = candidate.trim();
            if trimmed.is_empty() {
                return;
            }
            let low = trimmed.to_lowercase();
            if !lower_existing.is_empty() && Self::contains_constraint_phrase(&lower_existing, &low)
            {
                return;
            }
            if !out.iter().any(|c| c.eq_ignore_ascii_case(trimmed)) {
                out.push(trimmed.to_string());
            }
        };

        for lock in &schema_filled.constraint_locks {
            push_unique(&lock.text);
        }
        for c in &schema_filled.resolved_schema.constraints {
            if c.name.eq_ignore_ascii_case("preserve_hard_constraints") {
                continue;
            }
            push_unique(&c.name);
        }

        out
    }

    /// Checks whether `haystack_lower` contains all words of `phrase_lower` in sequence or verbatim.
    fn contains_constraint_phrase(haystack_lower: &str, phrase_lower: &str) -> bool {
        if haystack_lower.contains(phrase_lower) {
            return true;
        }
        let mut phrase_words: Vec<&str> = phrase_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if phrase_words.len() >= 2 && phrase_words[0] == "do" && phrase_words[1] == "not" {
            phrase_words.remove(0);
        }
        if phrase_words.is_empty() {
            return true;
        }
        let hay_words: Vec<&str> = haystack_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if phrase_words.len() == 1 {
            return hay_words.contains(&phrase_words[0]);
        }
        hay_words
            .windows(phrase_words.len())
            .any(|w| w == phrase_words.as_slice())
    }

    /// Builds the compact fallback string from `sparse_tokens` (or `clean_tokens`), ensuring every
    /// missing constraint lock and negation from the input is appended concisely.
    fn build_compact_with_constraints(schema_filled: &AlgorithmOutput) -> String {
        let mut base = Self::join_sparse_or_clean(schema_filled);
        if base.is_empty() {
            if let Some(task) = &schema_filled.resolved_schema.task {
                base = task.clone();
            }
        }

        // Also rescue critical negation clauses from normalized input if sparse tokens lost them
        let mut missing = Self::collect_missing_constraints(schema_filled, &base);
        if let Some(norm) = &schema_filled.normalization {
            for neg_span in Self::extract_negation_and_bound_spans(&norm.normalized_text) {
                let low = neg_span.to_lowercase();
                if !Self::contains_constraint_phrase(&base.to_lowercase(), &low)
                    && !missing
                        .iter()
                        .any(|m| Self::contains_constraint_phrase(&m.to_lowercase(), &low))
                {
                    missing.push(neg_span);
                }
            }
        }

        if !missing.is_empty() {
            if base.is_empty() {
                format!("Constraints: {}", missing.join(", "))
            } else {
                format!("{}; Constraints: {}", base, missing.join(", "))
            }
        } else {
            base
        }
    }

    /// Renders a budgeted structured prompt for `Mode::Balanced` and `Mode::Gentle`.
    ///
    /// Keeps output strictly below the input token count (targeting 25-55% savings on inputs
    /// with >= 15 tokens) while preserving the compressed core task, all constraints/negations,
    /// and deliverables that fit within the token budget.
    fn render_budgeted_structured(schema_filled: &AlgorithmOutput, sparse_joined: &str) -> String {
        let (ctx_opt, rag_opt, user_prompt) = schema_filled
            .normalization
            .as_ref()
            .map(|n| crate::npae::aggressive::structurer::split_raw_input(&n.normalized_text))
            .unwrap_or_else(|| (None, None, String::new()));

        let input_tokens = if !user_prompt.is_empty() {
            estimate_tokens(&user_prompt)
        } else if schema_filled.input_token_count > 0 {
            ((schema_filled.input_token_count as f64) * 1.3).ceil() as u32
        } else {
            estimate_tokens(sparse_joined)
        };

        // Determine core task text from sparse tokens when available and shorter than raw user_prompt
        let core_task = if !sparse_joined.is_empty()
            && (user_prompt.is_empty() || sparse_joined.len() < user_prompt.len())
        {
            sparse_joined.to_string()
        } else if !user_prompt.is_empty() {
            user_prompt.clone()
        } else if let Some(t) = &schema_filled.resolved_schema.task {
            t.clone()
        } else {
            String::new()
        };

        if core_task.is_empty() {
            return "Please clarify your request.".to_string();
        }

        // Gather constraints missing from `core_task` (plus any explicit negation/bound spans from user_prompt)
        let mut missing_constraints = Self::collect_missing_constraints(schema_filled, &core_task);
        if !user_prompt.is_empty() {
            for span in Self::extract_negation_and_bound_spans(&user_prompt) {
                let low = span.to_lowercase();
                if !Self::contains_constraint_phrase(&core_task.to_lowercase(), &low)
                    && !missing_constraints
                        .iter()
                        .any(|m| Self::contains_constraint_phrase(&m.to_lowercase(), &low))
                {
                    missing_constraints.push(span);
                }
            }
        }

        let mut lines = Vec::new();
        lines.push(format!("**Task:** {}", core_task));

        if !missing_constraints.is_empty() {
            lines.push(format!(
                "**Constraints:** {}",
                missing_constraints.join(", ")
            ));
        }

        // Strict token ceiling (`<= input_tokens`) and target structured budget:
        // Balanced aims for <= 75% of `input_tokens` when `input_tokens >= 15` (so TES >= 6.0 and savings
        // stay in the 30-55% target band); Gentle allows up to 90% to preserve lighter-polish context;
        // shorter prompts use `input_tokens.saturating_sub(1).max(1)` so we never expand.
        let budget_ratio = if matches!(schema_filled.mode, Some(Mode::Gentle)) {
            0.90
        } else {
            0.75
        };
        let strict_budget = if input_tokens >= 15 {
            ((input_tokens as f32) * budget_ratio).floor() as u32
        } else {
            input_tokens.saturating_sub(1).max(1)
        };

        // Include `**Role:**` first when a domain-specific role is resolved and fits within `strict_budget`.
        if let Some(role) = &schema_filled.resolved_schema.role {
            let trimmed_role = role.trim();
            if !trimmed_role.is_empty() && !trimmed_role.eq_ignore_ascii_case("Domain Expert") {
                let role_line = format!("**Role:** {}", trimmed_role);
                let candidate = format!("{}\n{}", role_line, lines.join("\n"));
                let cand_tokens = estimate_tokens(&candidate);
                if cand_tokens <= strict_budget && cand_tokens <= input_tokens {
                    lines.insert(0, role_line);
                }
            }
        }

        // Add concise Deliverables (capped at 3) if non-empty, not already in Task, and within `strict_budget`
        if !schema_filled.resolved_schema.output.is_empty() {
            let core_lower = core_task.to_lowercase();
            let delivs: Vec<String> = schema_filled
                .resolved_schema
                .output
                .iter()
                .filter(|d| !Self::contains_constraint_phrase(&core_lower, &d.name.to_lowercase()))
                .take(3)
                .map(|d| d.name.clone())
                .collect();
            if !delivs.is_empty() {
                let deliv_line = format!("**Deliverables:** {}", delivs.join(", "));
                let candidate = format!("{}\n{}", lines.join("\n"), deliv_line);
                let cand_tokens = estimate_tokens(&candidate);
                if cand_tokens <= strict_budget && cand_tokens <= input_tokens {
                    lines.push(deliv_line);
                }
            }
        }

        // Add inline Semantic Context / RAG blocks if present (RAG is external grounding)
        if let Some(ctx) = ctx_opt {
            let ctx_line = format!("**Semantic Context:** {}", ctx);
            let candidate = format!("{}\n{}", lines.join("\n"), ctx_line);
            let cand_tokens = estimate_tokens(&candidate);
            if cand_tokens <= strict_budget && cand_tokens <= input_tokens {
                lines.push(ctx_line);
            }
        }
        if let Some(rag) = rag_opt {
            let safe_rag = crate::npae::grounding::verifier::sanitize_untrusted_fact(&rag);
            lines.push(format!(
                "**Source Knowledge:**\n<retrieved_context trust=\"untrusted\">\n{}\n</retrieved_context>",
                safe_rag
            ));
        }
        if let Some(rag_block) = Self::format_enrichment_rag(schema_filled, &lines) {
            lines.push(rag_block);
        }

        let rendered = lines.join("\n");

        // Final safety check: if structured headers caused expansion (`> input_tokens`) on a very short prompt,
        // fall back to compact representation with constraints preserved.
        if input_tokens > 0
            && estimate_tokens(&rendered) > input_tokens.saturating_sub(1).max(1)
            && schema_filled.enrichment.is_none()
        {
            let compact = Self::build_compact_with_constraints(schema_filled);
            if !compact.is_empty() && estimate_tokens(&compact) <= input_tokens {
                return compact;
            }
        }

        rendered
    }

    /// Extracts short negation phrases (`no X`, `not X`, `do not X`, `must not X`, `without X`, `never X`)
    /// and bound phrases (`under N unit`, `handle missing values`) from raw text so that critical
    /// semantic constraints survive even if upstream slot clustering missed them.
    fn extract_negation_and_bound_spans(text: &str) -> Vec<String> {
        let mut spans = Vec::new();
        let stop_words = [
            "and",
            "or",
            "but",
            "so",
            "because",
            "while",
            "thanks",
            "thank",
            "please",
            "include",
            "highlight",
        ];

        let words: Vec<&str> = text.split_whitespace().collect();
        let n = words.len();
        let mut i = 0;

        while i < n {
            let clean = words[i]
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
                .to_lowercase();

            let is_neg_start = matches!(
                clean.as_str(),
                "no" | "not"
                    | "never"
                    | "without"
                    | "avoid"
                    | "exclude"
                    | "dont"
                    | "don't"
                    | "cannot"
                    | "can't"
                    | "under"
            );

            if is_neg_start {
                let mut phrase = Vec::new();
                // Include preceding modal ("must", "do", "should", "budget") if present
                if i > 0 {
                    let prev = words[i - 1]
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase();
                    if matches!(
                        prev.as_str(),
                        "must" | "do" | "should" | "could" | "would" | "budget" | "keep"
                    ) {
                        phrase.push(prev);
                    }
                }
                phrase.push(clean.clone());

                let mut j = i + 1;
                while j < n && phrase.len() < 5 {
                    let raw_w = words[j];
                    let w_clean = raw_w
                        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '$')
                        .to_lowercase();
                    if w_clean.is_empty() || stop_words.contains(&w_clean.as_str()) {
                        break;
                    }
                    phrase.push(w_clean);
                    if raw_w.ends_with([',', '.', ';', '!', '?']) {
                        break;
                    }
                    j += 1;
                }

                if phrase.len() >= 2 {
                    let candidate = phrase.join(" ");
                    if !spans.contains(&candidate) {
                        spans.push(candidate);
                    }
                }
                i = j;
                continue;
            }

            i += 1;
        }

        spans
    }

    /// Formats RAG chunks from `schema_filled.enrichment` if not already included in `parts`.
    ///
    /// Sanitizes chunk content and source file metadata and wraps retrieved knowledge inside
    /// `<retrieved_context trust="untrusted">` to defend against indirect prompt injection (ARCHITECTURE.md §7.2 Layer 4, Rules §9).
    fn format_enrichment_rag(schema_filled: &AlgorithmOutput, parts: &[String]) -> Option<String> {
        let enrichment = schema_filled.enrichment.as_ref()?;
        let rag_chunks = enrichment.rag_chunks.as_ref()?;
        if parts.iter().any(|p| {
            p.contains("Source Knowledge:")
                || p.contains("Retrieved Knowledge:")
                || p.contains("<retrieved_context")
        }) {
            return None;
        }
        let mut text = String::new();
        for chunk in rag_chunks {
            let page_str = chunk
                .metadata
                .as_ref()
                .and_then(|m| m.page_number)
                .map(|p| format!(", Page {}", p))
                .unwrap_or_default();
            let source_str = chunk
                .metadata
                .as_ref()
                .map(|m| {
                    let safe_source =
                        crate::npae::grounding::verifier::sanitize_untrusted_fact(&m.source_file);
                    format!("[Source: {}{}]\n", safe_source, page_str)
                })
                .unwrap_or_default();
            let safe_content =
                crate::npae::grounding::verifier::sanitize_untrusted_fact(&chunk.content);
            text.push_str(&format!("{}{}\n\n", source_str, safe_content));
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(format!(
                "**Retrieved Knowledge:**\n<retrieved_context trust=\"untrusted\">\n{}\n</retrieved_context>",
                trimmed
            ))
        }
    }
}

impl Default for Stage6a {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        CompressionSchema, Constraint, ConstraintToken, Deliverable, NormalizationResult,
        ScoredToken, TokenSource,
    };

    #[test]
    fn test_stage6a_balanced_budgeted_structured_compresses_below_input() {
        let stage6a = Stage6a::new();
        let raw_prompt = "Hi there, I was basically wondering if you could please help me out. I really need you to write a Python function that parses a CSV file and returns the average of the price column. It must not use pandas and it should handle missing values gracefully. Thanks so much in advance!";

        let out = AlgorithmOutput {
            mode: Some(Mode::Balanced),
            normalization: Some(NormalizationResult {
                normalized_text: raw_prompt.to_string(),
                normalization_score: 1.0,
                applied_rules: vec![],
            }),
            sparse_tokens: vec![
                ScoredToken {
                    text: "write".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "python".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "function".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "parses".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "csv".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "returns".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "average".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "price".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "handle".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "missing".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "values".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
            ],
            constraint_locks: vec![ConstraintToken {
                text: "must not use pandas".into(),
                weight: 1.0,
            }],
            resolved_schema: CompressionSchema {
                role: Some("Expert Software Engineer".into()),
                task: Some("Perform write action".into()),
                context: Some("Some verbose inferred context".into()),
                constraints: vec![Constraint {
                    name: "must not use pandas".into(),
                }],
                output: vec![Deliverable {
                    name: "python_function".into(),
                }],
            },
            null_fields: vec!["context".into()],
            ..Default::default()
        };

        let rendered = stage6a.run(&out).expect("Stage6a should succeed");
        let in_tokens = estimate_tokens(raw_prompt);
        let out_tokens = estimate_tokens(&rendered);

        assert!(
            out_tokens < in_tokens,
            "Balanced output ({out_tokens} tokens) must be shorter than input ({in_tokens} tokens): {rendered}"
        );
        assert!(
            rendered.to_lowercase().contains("not use pandas"),
            "Constraint lock must be preserved in rendered output: {rendered}"
        );
        assert!(
            !rendered.contains("**Warnings:**"),
            "Balanced mode must omit verbose Warnings boilerplate"
        );
    }

    #[test]
    fn test_stage6a_compact_fallback_never_drops_constraints() {
        let stage6a = Stage6a::new();
        let out = AlgorithmOutput {
            mode: Some(Mode::Balanced),
            force_compact_generation: true,
            sparse_tokens: vec![
                ScoredToken {
                    text: "create".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "7".into(),
                    salience: 1.0,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "day".into(),
                    salience: 0.8,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "meal".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
                ScoredToken {
                    text: "plan".into(),
                    salience: 0.9,
                    source: TokenSource::Sparse,
                },
            ],
            constraint_locks: vec![
                ConstraintToken {
                    text: "no dairy".into(),
                    weight: 1.0,
                },
                ConstraintToken {
                    text: "no nightshades".into(),
                    weight: 1.0,
                },
                ConstraintToken {
                    text: "under 80 usd".into(),
                    weight: 1.0,
                },
            ],
            ..Default::default()
        };

        let compact = stage6a
            .run(&out)
            .expect("Compact generation should succeed");
        let lower = compact.to_lowercase();
        assert!(
            lower.contains("no dairy"),
            "Compact output must retain 'no dairy': {compact}"
        );
        assert!(
            lower.contains("no nightshades"),
            "Compact output must retain 'no nightshades': {compact}"
        );
        assert!(
            lower.contains("under 80 usd"),
            "Compact output must retain 'under 80 usd': {compact}"
        );
    }
}
