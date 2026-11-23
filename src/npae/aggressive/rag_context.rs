//! RAG Fact Derivation & Provenance Preservation (`DerivedRagContext`).
//!
//! // ARCH: ARCHITECTURE.md §2.1, §3 & §5 GAP-35 / GAP-42 — derives concise, deduplicated,
//! // injection-screened facts from RAG chunks while preserving exact document/page provenance
//! // (`DerivedFact`) and decimal numeric integrity (`split_sentences_preserving_decimals`).

use crate::types::RagChunk;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A single derived fact statement bundled with its originating chunk provenance and `[F#]` ID.
///
/// // ARCH: ARCHITECTURE.md §5 (G-03) — preserves exact source file and page number per derived
/// // fact so downstream scenario/harness renderers never misattribute facts across chunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedFact {
    /// Stable 1-based fact identifier (`"F1"`, `"F2"`, ...).
    pub id: String,
    /// Extracted declarative fact sentence.
    pub text: String,
    /// Originating file name from `RagChunk.metadata`, if available.
    pub source_file: String,
    /// Originating page number from `RagChunk.metadata`, if available.
    pub page_number: Option<u32>,
    /// Normalized lowercase domain tag of the originating chunk.
    pub domain_tag: String,
}

/// Concise, deduplicated RAG context derived within a token budget.
#[derive(Debug, Clone, Default)]
pub struct DerivedRagContext {
    /// Plain fact strings retained for 100% backward compatibility.
    pub facts: Vec<String>,
    /// Provenance-preserving fact records corresponding 1-to-1 with `facts`.
    pub detailed_facts: Vec<DerivedFact>,
    /// Ambiguity gap zones satisfied by the derived facts.
    pub covered_gaps: HashSet<String>,
    /// Estimated token count of all selected facts.
    pub tokens_added: u32,
}

struct RawCandidateSentence {
    text: String,
    domain_tag: String,
    source_file: String,
    page_number: Option<u32>,
}

struct ScoredFactCandidate {
    score: usize,
    text: String,
    lower: String,
    source_file: String,
    page_number: Option<u32>,
    domain_tag: String,
}

impl DerivedRagContext {
    /// Derives concise, non-repetitive facts from RAG chunks within a given token budget (default 250 tokens).
    /// Filters out prompt injection attempts, strips file/page citations, preserves decimal numbers
    /// and chunk provenance, and maps covered gap zones.
    ///
    /// # Panics
    /// Never.
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
        // FIX: G-04 — compare candidate sentence words against a HashSet of prompt words
        // (`len >= 3`) instead of substring `prompt_lower.contains(w.as_str())`.
        let prompt_word_set: HashSet<String> = prompt_tokens
            .iter()
            .filter(|w| w.len() >= 3)
            .cloned()
            .collect();

        // 1. Extract sentences across all chunks while preserving chunk provenance (G-03)
        // and decimal numbers such as `$4.2M` or `99.95%` (G-04).
        for chunk in chunks {
            let domain_tag_lower = chunk.domain_tag.to_lowercase();
            let source_file = chunk
                .metadata
                .as_ref()
                .map(|m| m.source_file.clone())
                .unwrap_or_default();
            let page_number = chunk.metadata.as_ref().and_then(|m| m.page_number);

            for line in chunk.content.lines() {
                let cleaned_line = line.trim();
                if cleaned_line.is_empty()
                    || cleaned_line.starts_with("[Source:")
                    || cleaned_line.starts_with("legal_docs_india |")
                    || cleaned_line.contains("| Reference corpus for retrieval")
                {
                    continue;
                }

                // FIX: G-04 — use `split_sentences_preserving_decimals` so `$4.2M` and `99.95%`
                // remain intact instead of splitting on every `.` character.
                for sentence in
                    crate::npae::grounding::verifier::split_sentences_preserving_decimals(
                        cleaned_line,
                    )
                {
                    let s = sentence.trim();
                    if s.ends_with('?') {
                        continue;
                    }
                    let s = s.trim_end_matches(&['.', '!', '?'][..]).trim();
                    if s.len() >= 10 && s.split_whitespace().count() >= 3 {
                        raw_sentences.push(RawCandidateSentence {
                            text: s.to_string(),
                            domain_tag: domain_tag_lower.clone(),
                            source_file: source_file.clone(),
                            page_number,
                        });
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
        let mut scored_candidates: Vec<ScoredFactCandidate> = Vec::new();

        for candidate in raw_sentences {
            let sentence = candidate.text;
            let sentence_lower = sentence.to_lowercase();

            // Injection safety check (local keywords + canonical verifier injection detector)
            if injection_keywords
                .iter()
                .any(|&k| sentence_lower.contains(k))
                || crate::npae::grounding::verifier::contains_prompt_injection(&sentence)
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
                .filter(|s| s.len() >= 3)
                .collect();

            if !sent_words.is_empty() {
                let overlap_count = sent_words
                    .iter()
                    .filter(|w| prompt_word_set.contains(w.as_str()))
                    .count();
                let overlap_ratio = overlap_count as f32 / sent_words.len() as f32;
                let has_new_quant = sentence_lower.split_whitespace().any(|raw_w| {
                    let has_digit = raw_w.chars().any(|c| c.is_ascii_digit());
                    if !has_digit {
                        return false;
                    }
                    let clean = raw_w.trim_matches(|c: char| !c.is_alphanumeric());
                    !clean.is_empty() && !prompt_word_set.contains(clean)
                });
                // AUD-13: Keep high-overlap RAG sentences when they add numeric/currency/percentage
                // evidence not already present in the prompt.
                if overlap_ratio >= 0.80 && !has_new_quant {
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
                if !candidate.domain_tag.is_empty() && prompt_lower.contains(&candidate.domain_tag)
                {
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

            seen_normalized.insert(norm_key);
            scored_candidates.push(ScoredFactCandidate {
                score,
                text: sentence,
                lower: sentence_lower,
                source_file: candidate.source_file,
                page_number: candidate.page_number,
                domain_tag: candidate.domain_tag,
            });
        }

        // Sort descending by relevance score so the highest-signal facts fill the token budget first
        scored_candidates.sort_by_key(|b| std::cmp::Reverse(b.score));

        let mut selected_facts = Vec::new();
        let mut detailed_facts = Vec::new();
        let mut current_tokens = 0usize;
        let mut covered_gaps = HashSet::new();

        for candidate in scored_candidates {
            let sent_tokens = crate::utils::tokens::estimate_tokens(&candidate.text) as usize;
            if current_tokens + sent_tokens > token_budget {
                if selected_facts.is_empty() {
                    // Always include at least one top-scoring fact even if close to budget
                    let fact_id = format!("F{}", detailed_facts.len() + 1);
                    selected_facts.push(candidate.text.clone());
                    detailed_facts.push(DerivedFact {
                        id: fact_id,
                        text: candidate.text,
                        source_file: candidate.source_file,
                        page_number: candidate.page_number,
                        domain_tag: candidate.domain_tag,
                    });
                    current_tokens += sent_tokens;
                }
                break;
            }

            // Track covered gap zones based on keywords in facts
            if candidate.lower.contains('$')
                || candidate.lower.contains('₹')
                || candidate.lower.contains("budget")
                || candidate.lower.contains("cost")
                || candidate.lower.contains("price")
            {
                covered_gaps.insert("budget".to_string());
                covered_gaps.insert(
                    "structural_ambiguity: budget / financial targets not specified".to_string(),
                );
            }
            if candidate.lower.contains("timeline")
                || candidate.lower.contains("deadline")
                || candidate.lower.contains("month")
                || candidate.lower.contains("quarter")
                || candidate.lower.contains("date")
            {
                covered_gaps.insert("timeline".to_string());
                covered_gaps
                    .insert("structural_ambiguity: temporal scope / deadline missing".to_string());
            }
            if candidate.lower.contains("scope")
                || candidate.lower.contains("phase")
                || candidate.lower.contains("requirement")
            {
                covered_gaps.insert("scope".to_string());
            }
            if candidate.lower.contains("domain") || candidate.lower.contains("industry") {
                covered_gaps.insert("domain_context_missing".to_string());
            }

            current_tokens += sent_tokens;
            let fact_id = format!("F{}", detailed_facts.len() + 1);
            selected_facts.push(candidate.text.clone());
            detailed_facts.push(DerivedFact {
                id: fact_id,
                text: candidate.text,
                source_file: candidate.source_file,
                page_number: candidate.page_number,
                domain_tag: candidate.domain_tag,
            });
        }

        DerivedRagContext {
            facts: selected_facts,
            detailed_facts,
            covered_gaps,
            tokens_added: current_tokens as u32,
        }
    }
}
