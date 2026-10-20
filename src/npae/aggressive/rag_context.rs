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
            let domain_tag_lower = chunk.domain_tag.to_lowercase();
            // Split chunk into sentence-like clauses
            for line in chunk.content.lines() {
                let cleaned_line = line.trim();
                if cleaned_line.is_empty()
                    || cleaned_line.starts_with("[Source:")
                    || cleaned_line.starts_with("legal_docs_india |")
                    || cleaned_line.contains("| Reference corpus for retrieval")
                {
                    continue;
                }

                for sentence in cleaned_line.split(&['.', '!', '?'][..]) {
                    let s = sentence.trim();
                    if s.len() >= 10 && s.split_whitespace().count() >= 3 {
                        raw_sentences.push((s.to_string(), domain_tag_lower.clone()));
                    }
                }
            }
        }

        // Generic stop-words filter for extracting meaningful intent terms
        let prompt_terms: Vec<String> = prompt_tokens
            .into_iter()
            .filter(|w| {
                w.len() >= 4
                    && !matches!(
                        w.as_str(),
                        "this"
                            | "that"
                            | "with"
                            | "from"
                            | "they"
                            | "been"
                            | "have"
                            | "what"
                            | "which"
                            | "when"
                            | "where"
                            | "how"
                            | "who"
                            | "whom"
                            | "than"
                            | "then"
                            | "these"
                            | "those"
                            | "each"
                            | "every"
                            | "some"
                            | "such"
                            | "only"
                            | "into"
                            | "over"
                            | "after"
                            | "also"
                            | "would"
                            | "could"
                            | "should"
                            | "about"
                            | "there"
                            | "their"
                            | "please"
                            | "search"
                            | "need"
                            | "tell"
                            | "give"
                            | "details"
                    )
            })
            .collect();

        // 2. Score candidate sentences for topical relevance across any domain
        // Sentences with higher keyword/stem overlap or quantitative metrics are prioritized
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
        let mut seen_normalized = HashSet::new();
        let mut scored_candidates: Vec<(usize, String, String)> = Vec::new();

        for (sentence, chunk_domain) in raw_sentences {
            let sentence_lower = sentence.to_lowercase();

            // Injection safety check
            if injection_keywords
                .iter()
                .any(|&k| sentence_lower.contains(k))
            {
                continue;
            }

            // Generic document metadata, TOC, and corpus scaffolding filter
            // Prevents PDF metadata, chapter listings, and boilerplate from being treated as facts
            let is_boilerplate = {
                sentence_lower.contains("reference corpus")
                    || sentence_lower.contains("draft corpus")
                    || sentence_lower.contains("table of contents")
                    || sentence_lower.contains("all rights reserved")
                    || (sentence_lower.contains("scenarios") && sentence_lower.contains("chunks"))
                    || (sentence_lower.starts_with("page ") && sentence_lower.contains("version"))
                    || (sentence_lower.contains("compiled") && sentence_lower.contains("202"))
                    || sentence_lower.starts_with("forum magistrate related")
                    || sentence_lower.starts_with("part a -")
                    || sentence_lower.starts_with("part b -")
                    || sentence_lower.starts_with("part c -")
                    || sentence_lower.starts_with("part d -")
                    || sentence_lower.starts_with("part e -")
                    || sentence_lower.starts_with("part f -")
            };
            if is_boilerplate {
                continue;
            }

            // Deduplication (exact normalized string match to handle chunk overlaps)
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
                    .filter(|w| prompt_lower.contains(w.as_str()))
                    .count();
                let overlap_ratio = overlap_count as f32 / sent_words.len() as f32;
                if overlap_ratio >= 0.80 {
                    continue;
                }
            }

            // Generic relevance score: count matches against prompt intent terms or their stems
            let mut score = 0usize;
            if !prompt_terms.is_empty() {
                for term in &prompt_terms {
                    let stem = if term.chars().count() > 5 {
                        term.char_indices()
                            .last()
                            .map_or(term.as_str(), |(idx, _)| &term[..idx])
                    } else {
                        term.as_str()
                    };
                    if sentence_lower.contains(stem) {
                        score += 3;
                    }
                }

                // If chunk belongs to a domain explicitly mentioned in the user prompt, boost domain facts
                if !chunk_domain.is_empty() && prompt_lower.contains(&chunk_domain) {
                    score += 2;
                }

                // If prompt has clear search terms, drop sentences with 0 relevance to avoid cross-topic leaks
                if score == 0 {
                    continue;
                }
            } else {
                score = 1; // Fallback when prompt is generic
            }

            // Boost sentences carrying concrete factual values (numbers, currencies, percentages, codes)
            if sentence_lower.contains('$')
                || sentence_lower.contains('₹')
                || sentence_lower.contains('%')
                || sentence.chars().any(|c| c.is_ascii_digit())
            {
                score += 1;
            }

            seen_normalized.insert(norm_key.clone());
            scored_candidates.push((score, sentence, sentence_lower));
        }

        // Sort descending by relevance score so the highest-signal facts fill the token budget first
        scored_candidates.sort_by(|a, b| b.0.cmp(&a.0));

        let mut selected_facts = Vec::new();
        let mut current_tokens = 0usize;
        let mut covered_gaps = HashSet::new();

        for (_score, sentence, sentence_lower) in scored_candidates {
            let sent_tokens = crate::utils::tokens::estimate_tokens(&sentence) as usize;
            if current_tokens + sent_tokens > token_budget {
                if selected_facts.is_empty() {
                    // Always include at least one top-scoring fact even if close to budget
                    selected_facts.push(sentence);
                    current_tokens += sent_tokens;
                }
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
