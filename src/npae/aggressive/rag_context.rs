use crate::types::RagChunk;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct DerivedRagContext {
    pub facts: Vec<String>,
    pub covered_gaps: HashSet<String>,
    pub tokens_added: u32,
}

impl DerivedRagContext {
    /// Derive concise, non-repetitive facts from RAG chunks within a given token budget (default 250 tokens).
    /// Filters out prompt injection attempts, strips file/page citations, and maps covered gap zones.
    pub fn derive(chunks: &[RagChunk], user_prompt: &str, token_budget: usize) -> Self {
        if chunks.is_empty() {
            return Self::default();
        }

        let mut raw_sentences = Vec::new();
        let prompt_lower = user_prompt.to_lowercase();
        let prompt_tokens: HashSet<String> = prompt_lower
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // 1. Extract sentences across all chunks
        for chunk in chunks {
            // Split chunk into sentence-like clauses
            for line in chunk.content.lines() {
                let cleaned_line = line.trim();
                if cleaned_line.is_empty() || cleaned_line.starts_with("[Source:") {
                    continue;
                }

                for sentence in cleaned_line.split(&['.', '!', '?'][..]) {
                    let s = sentence.trim();
                    if s.len() >= 10 && s.split_whitespace().count() >= 3 {
                        raw_sentences.push(s.to_string());
                    }
                }
            }
        }

        let mut selected_facts = Vec::new();
        let mut seen_normalized = HashSet::new();
        let mut current_tokens = 0usize;
        let mut covered_gaps = HashSet::new();

        // Instruction/injection filter keywords
        let injection_keywords = [
            "ignore previous",
            "system prompt",
            "you must",
            "assistant:",
            "system:",
            "ignore instructions",
            "new instructions",
            "disregard",
        ];

        for sentence in raw_sentences {
            let sentence_lower = sentence.to_lowercase();

            // Injection safety check
            if injection_keywords
                .iter()
                .any(|&k| sentence_lower.contains(k))
            {
                continue;
            }

            // Deduplication (exact normalized string match to handle 64-token chunk overlaps)
            let norm_key: String = sentence_lower
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();

            if norm_key.is_empty() || seen_normalized.contains(&norm_key) {
                continue;
            }

            // Deduplication against existing prompt words (drop sentence if >= 80% overlap with prompt)
            let sent_words: Vec<String> = sentence_lower
                .split_whitespace()
                .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if !sent_words.is_empty() {
                let overlap_count = sent_words
                    .iter()
                    .filter(|w| prompt_tokens.contains(*w))
                    .count();
                let overlap_ratio = overlap_count as f32 / sent_words.len() as f32;
                if overlap_ratio >= 0.80 {
                    continue;
                }
            }

            // Token count estimate for this sentence
            let sent_tokens = crate::utils::tokens::estimate_tokens(&sentence) as usize;
            if current_tokens + sent_tokens > token_budget {
                break;
            }

            // Track covered gap zones based on keywords in facts
            if sentence_lower.contains('$')
                || sentence_lower.contains('₹')
                || sentence_lower.contains("budget")
                || sentence_lower.contains("cost")
                || sentence_lower.contains("price")
            {
                covered_gaps.insert("budget".to_string());
                covered_gaps.insert(
                    "structural_ambiguity: budget / financial targets not specified".to_string(),
                );
            }
            if sentence_lower.contains("timeline")
                || sentence_lower.contains("deadline")
                || sentence_lower.contains("month")
                || sentence_lower.contains("quarter")
                || sentence_lower.contains("date")
            {
                covered_gaps.insert("timeline".to_string());
                covered_gaps
                    .insert("structural_ambiguity: temporal scope / deadline missing".to_string());
            }
            if sentence_lower.contains("scope")
                || sentence_lower.contains("phase")
                || sentence_lower.contains("requirement")
            {
                covered_gaps.insert("scope".to_string());
            }
            if sentence_lower.contains("domain") || sentence_lower.contains("industry") {
                covered_gaps.insert("domain_context_missing".to_string());
            }

            seen_normalized.insert(norm_key);
            current_tokens += sent_tokens;
            selected_facts.push(sentence);
        }

        DerivedRagContext {
            facts: selected_facts,
            covered_gaps,
            tokens_added: current_tokens as u32,
        }
    }
}
