//! Grounding Fact Collection & Requested-Item Extraction.
//!
//! // ARCH: ARCHITECTURE.md §5 GAP-42 & §7.2 Layer 4 — extracts, sanitizes, and numbers
//! // `[F1]..[Fn]` grounding facts and `<requested_items>` from raw input and `DerivedRagContext`.

use std::collections::HashSet;

use super::ingest::split_raw_input;
use crate::npae::aggressive::rag_context::DerivedRagContext;
use crate::npae::grounding::verifier::{
    contains_prompt_injection, sanitize_untrusted_fact, split_sentences_preserving_decimals,
    NumberedFact,
};
use crate::npae::schema::types::StructuredPrompt;

/// Collects, sanitizes, and numbers `[F1]..[Fn]` grounding facts from user `Context: [...]`,
/// inline retrieved knowledge blocks, `DerivedRagContext`, and declarative statements in the prompt.
///
/// // ARCH: ARCHITECTURE.md §5 GAP-42 & §7.2 Layer 4 — untrusted text is screened for prompt
/// // injection and XML-escaped before entering `<facts trust="untrusted">`.
///
/// # Panics
/// Never.
#[must_use]
pub fn collect_grounding_facts(
    prompt: &StructuredPrompt,
    raw_input: &str,
    derived_rag: Option<&DerivedRagContext>,
) -> Vec<NumberedFact> {
    let (context_opt, rag_opt, user_prompt) = split_raw_input(raw_input);
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut facts: Vec<NumberedFact> = Vec::new();

    let mut push_candidate = |candidate: &str| {
        let trimmed = candidate.trim().trim_end_matches('.');
        if trimmed.len() < 4
            || trimmed.starts_with("[Source:")
            || contains_prompt_injection(trimmed)
        {
            return;
        }
        let norm_key: String = trimmed
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if norm_key.is_empty() || !seen_keys.insert(norm_key) {
            return;
        }
        let next_num = facts.len() + 1;
        facts.push(NumberedFact {
            id: format!("F{next_num}"),
            text: sanitize_untrusted_fact(trimmed),
        });
    };

    if let Some(ref ctx) = context_opt {
        for clause in split_sentences_preserving_decimals(ctx) {
            push_candidate(&clause);
        }
    }

    if let Some(ref rag_block) = rag_opt {
        for clause in split_sentences_preserving_decimals(rag_block) {
            push_candidate(&clause);
        }
    }

    if let Some(rag) = derived_rag {
        for fact in &rag.facts {
            push_candidate(fact);
        }
    } else if let Some(idx) = prompt.context.background.find("Known facts: ") {
        if let Some(rest) = prompt.context.background.get(idx + "Known facts: ".len()..) {
            for fact in rest.split(';') {
                push_candidate(fact);
            }
        }
    }

    for sentence in split_sentences_preserving_decimals(&user_prompt) {
        if let Some(decl_fact) = extract_declarative_user_fact(&sentence) {
            push_candidate(decl_fact);
        }
    }

    facts
}

const HARNESS_TASK_STARTERS: &[&str] = &[
    "analyze",
    "analyse",
    "build",
    "create",
    "write",
    "draft",
    "explain",
    "describe",
    "compare",
    "evaluate",
    "summarize",
    "summarise",
    "list",
    "find",
    "show",
    "tell",
    "give",
    "provide",
    "calculate",
    "compute",
    "extract",
    "identify",
    "check",
    "verify",
    "review",
    "plan",
    "design",
    "generate",
    "recommend",
    "suggest",
    "help",
    "please",
    "report",
    "state",
    "what",
    "which",
    "who",
    "whom",
    "whose",
    "when",
    "where",
    "why",
    "how",
    "is",
    "are",
    "was",
    "were",
    "do",
    "does",
    "did",
    "can",
    "could",
    "should",
    "would",
    "will",
];

pub(super) fn extract_declarative_user_fact(sentence: &str) -> Option<&str> {
    let trimmed = sentence.trim();
    if trimmed.ends_with('?') || trimmed.split_whitespace().count() < 3 {
        return None;
    }

    let lower = trimmed.to_lowercase();
    // FIX: G-12 — Reject hypothetical or conditional clauses so speculative premises are never
    // promoted into self-licensing `[F#]` grounding facts.
    for cond_prefix in [
        "if ",
        "suppose ",
        "assuming ",
        "what if ",
        "when ",
        "unless ",
    ] {
        if lower.starts_with(cond_prefix) {
            return None;
        }
    }

    for prefix in ["fact:", "facts:", "context:", "data:", "record:"] {
        if lower.starts_with(prefix) {
            let rest = trimmed.get(prefix.len()..)?.trim();
            if rest.split_whitespace().count() >= 2 {
                return Some(rest);
            }
        }
    }

    let ask_phrases = [
        "what is",
        "what are",
        "what was",
        "what were",
        "how much",
        "how many",
        "tell me",
        "give me",
        "show me",
        "find the",
    ];
    if ask_phrases.iter().any(|p| lower.contains(p)) {
        return None;
    }

    let first_word = lower
        .split(|c: char| !c.is_alphabetic())
        .find(|s| !s.is_empty())?;
    if HARNESS_TASK_STARTERS.contains(&first_word) {
        return None;
    }

    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    let padded = format!(" {lower} ");
    let has_state_verb = [
        " is ",
        " are ",
        " was ",
        " were ",
        " has ",
        " have ",
        " had ",
        " equals ",
        " totaled ",
        " costs ",
        " includes ",
        " contains ",
        " given ",
    ]
    .iter()
    .any(|v| padded.contains(v));

    if has_digit || has_state_verb {
        Some(trimmed)
    } else {
        None
    }
}

/// Derives concise, ordered items for `<requested_items>` from the user's question/task
/// and explicit required inclusions.
///
/// # Panics
/// Never.
#[must_use]
pub fn derive_requested_items(prompt: &StructuredPrompt, raw_input: &str) -> Vec<String> {
    let (_, _, user_prompt) = split_raw_input(raw_input);
    let mut items: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let sentences = split_sentences_preserving_decimals(&user_prompt);
    for sentence in &sentences {
        if extract_declarative_user_fact(sentence).is_some() && sentences.len() > 1 {
            continue;
        }
        // FIX: Trailing negative formatting/topic constraints (e.g. `no json`, `without tables`)
        // belong in `<constraints>`, not as separate rows in `<requested_items>`, once primary
        // requested items have already been extracted.
        let s_low = sentence.trim().to_lowercase();
        if !items.is_empty()
            && (s_low.starts_with("no ")
                || s_low.starts_with("without ")
                || s_low.starts_with("avoid ")
                || s_low.starts_with("exclude "))
        {
            continue;
        }
        if let Some(enumerated) = split_enumerated_ask(sentence) {
            for part in enumerated {
                push_requested_item(&mut items, &mut seen, &part);
            }
        } else {
            push_requested_item(&mut items, &mut seen, sentence);
        }
    }

    if items.is_empty() {
        for inc in &prompt.constraints.required_inclusions {
            if inc.split_whitespace().count() <= 12 {
                push_requested_item(&mut items, &mut seen, inc);
            }
        }
    }

    if items.is_empty() {
        for sc in prompt.success_criteria.iter().take(3) {
            push_requested_item(&mut items, &mut seen, sc);
        }
    }

    if items.is_empty() && !user_prompt.trim().is_empty() {
        push_requested_item(&mut items, &mut seen, &user_prompt);
    }

    items
}

fn push_requested_item(items: &mut Vec<String>, seen: &mut HashSet<String>, raw_item: &str) {
    if items.len() >= 6 {
        return;
    }
    let cleaned = raw_item
        .trim()
        .trim_end_matches(&['?', '.', ';', ':'][..])
        .trim();
    if cleaned.is_empty() {
        return;
    }
    let key = cleaned.to_lowercase();
    if seen.insert(key) {
        items.push(cleaned.to_string());
    }
}

fn split_enumerated_ask(sentence: &str) -> Option<Vec<String>> {
    let trimmed = sentence.trim().trim_end_matches('?').trim();
    let lower = trimmed.to_lowercase();
    let prefixes = [
        "what is ",
        "what are ",
        "what was ",
        "what were ",
        "report ",
        "extract ",
        "list ",
        "provide ",
        "state ",
        "find ",
        "give me ",
        "tell me ",
    ];

    let mut tail: Option<&str> = None;
    for prefix in prefixes {
        if let Some(pos) = lower.find(prefix) {
            if pos <= 12 {
                tail = trimmed.get(pos + prefix.len()..);
                break;
            }
        }
    }
    let body = tail?.trim();
    if !body.contains(',') && !body.to_lowercase().contains(" and ") {
        return None;
    }

    let normalized = body
        .replace(", and ", " | ")
        .replace(" and ", " | ")
        .replace(',', " | ");
    let mut parts = Vec::new();
    for raw_part in normalized.split('|') {
        let mut p = raw_part.trim();
        for determiner in ["our ", "the ", "their ", "my ", "its "] {
            if p.len() > determiner.len() && p.to_lowercase().starts_with(determiner) {
                if let Some(stripped) = p.get(determiner.len()..) {
                    p = stripped.trim();
                }
                break;
            }
        }
        let wc = p.split_whitespace().count();
        if (1..=8).contains(&wc) {
            parts.push(p.to_string());
        } else {
            return None;
        }
    }

    if parts.len() >= 2 {
        Some(parts)
    } else {
        None
    }
}

// --- Phase 3: Provenance-Tagged EvidenceBundle, Conflict Detection & Token-Budgeted Packing ---

/// Mandatory provenance tag for every untrusted evidence item entering the prompt.
///
/// // ARCH: ARCHITECTURE.md §6.6 & §7.2 — engine-inferred template defaults are intentionally
/// // excluded from this enum so they can never masquerade as `[F#]` grounding facts.
#[derive(Debug, Clone, PartialEq)]
pub enum Provenance {
    /// Stated verbatim by the user in the raw prompt or `Context: [...]` block.
    UserVerbatim,
    /// Retrieved from a Cloud RAG or Local Own-Data RAG document chunk.
    RagDocument {
        source_file: String,
        page_number: Option<u32>,
        score: f32,
    },
    /// Supplied from a structured external source (database row, CSV/Markdown table, API record).
    StructuredRecord {
        source_id: String,
        record_key: String,
    },
}

impl Provenance {
    /// Formats a concise, XML-escaped source label suitable for inline `[F#]` rendering.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn format_label(&self) -> String {
        match self {
            Self::UserVerbatim => String::new(),
            Self::RagDocument {
                source_file,
                page_number,
                ..
            } => match page_number {
                Some(p) if !source_file.is_empty() => {
                    format!(" (Source: {}, p.{p})", sanitize_untrusted_fact(source_file))
                }
                _ if !source_file.is_empty() => {
                    format!(" (Source: {})", sanitize_untrusted_fact(source_file))
                }
                _ => String::new(),
            },
            Self::StructuredRecord {
                source_id,
                record_key,
            } => {
                let s = sanitize_untrusted_fact(source_id);
                let k = sanitize_untrusted_fact(record_key);
                if k.is_empty() {
                    format!(" (Record: {s})")
                } else {
                    format!(" (Record: {s}/{k})")
                }
            }
        }
    }

    /// Priority tier used during token-budgeted packing (lower = higher priority).
    #[inline]
    fn priority_tier(&self) -> u8 {
        match self {
            Self::UserVerbatim => 0,
            Self::StructuredRecord { .. } => 1,
            Self::RagDocument { .. } => 2,
        }
    }
}

/// Typed payload kind for an [`EvidenceItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Free-text declarative fact sentence.
    Fact,
    /// Structured key-value pair (e.g. user profile attribute, config field, API metric).
    KeyValue { key: String },
    /// Structured table row with column headers for compact, lossless tabular rendering.
    TableRow {
        table_name: String,
        columns: Vec<String>,
    },
}

/// A single sanitized, provenance-tagged evidence entry ready for token-budgeted packing.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceItem {
    /// Stable 1-based identifier assigned during packing (`"F1"`, `"F2"`, ...).
    pub id: String,
    /// Typed evidence kind (`Fact`, `KeyValue`, or `TableRow`).
    pub kind: EvidenceKind,
    /// Sanitized, XML-escaped textual representation of the evidence.
    pub text: String,
    /// Origin and trust provenance of the datum.
    pub provenance: Provenance,
    /// Relevance score in `[0.0, 1.0]` used to rank items within the same provenance tier.
    pub relevance_score: f32,
}

/// A deterministic numeric/metric contradiction detected between two packed evidence items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceConflict {
    /// First conflicting fact ID (e.g. `"F1"`).
    pub left_id: String,
    /// Second conflicting fact ID (e.g. `"F3"`).
    pub right_id: String,
    /// Shared domain terms linking the two items.
    pub shared_terms: Vec<String>,
    /// Human-readable description of the numeric/date discrepancy.
    pub reason: String,
}

/// Explicit token and item-count budget for packing an [`EvidenceBundle`].
///
/// // ARCH: ARCHITECTURE.md §6.6 — replaces ad-hoc length cutoffs with deterministic token packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptBudget {
    /// Maximum estimated tokens allowed for the rendered evidence block.
    pub max_evidence_tokens: u32,
    /// Hard cap on the number of packed evidence items.
    pub max_items: usize,
}

impl Default for PromptBudget {
    fn default() -> Self {
        Self {
            max_evidence_tokens: 600,
            max_items: 24,
        }
    }
}

/// Result of packing an [`EvidenceBundle`] against a [`PromptBudget`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackedEvidence {
    /// Selected, deduplicated, provenance-tagged evidence items with `[F1]..[Fn]` IDs.
    pub items: Vec<EvidenceItem>,
    /// Deterministic numeric/date contradictions detected among `items`.
    pub conflicts: Vec<EvidenceConflict>,
    /// Estimated tokens consumed by `items`.
    pub tokens_used: u32,
    /// Number of valid, unique evidence items omitted due to `PromptBudget` limits.
    pub omitted_count: usize,
}

impl PackedEvidence {
    /// Converts packed evidence items into legacy [`NumberedFact`] records for backward compatibility.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn to_numbered_facts(&self) -> Vec<NumberedFact> {
        self.items
            .iter()
            .map(|item| NumberedFact {
                id: item.id.clone(),
                text: item.text.clone(),
            })
            .collect()
    }
}

/// Multi-source, injection-screened evidence collector supporting facts, key-value records,
/// and structured table rows with mandatory [`Provenance`].
///
/// // ARCH: ARCHITECTURE.md §6.6 & §7.2 — scales data ingestion safely without hallucination.
#[derive(Debug, Clone, Default)]
pub struct EvidenceBundle {
    items: Vec<EvidenceItem>,
}

impl EvidenceBundle {
    /// Creates an empty `EvidenceBundle`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Builds an `EvidenceBundle` from the standard pipeline inputs (`StructuredPrompt`, `raw_input`,
    /// and optional `DerivedRagContext`), preserving full `source_file` and `page_number` provenance
    /// from `DerivedRagContext::detailed_facts`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn from_inputs(
        prompt: &StructuredPrompt,
        raw_input: &str,
        derived_rag: Option<&DerivedRagContext>,
    ) -> Self {
        let mut bundle = Self::new();
        let (context_opt, rag_opt, user_prompt) = split_raw_input(raw_input);

        if let Some(ref ctx) = context_opt {
            for clause in split_sentences_preserving_decimals(ctx) {
                bundle.push_fact(&clause, Provenance::UserVerbatim, 1.0);
            }
        }

        if let Some(ref rag_block) = rag_opt {
            for clause in split_sentences_preserving_decimals(rag_block) {
                bundle.push_fact(
                    &clause,
                    Provenance::RagDocument {
                        source_file: String::new(),
                        page_number: None,
                        score: 0.85,
                    },
                    0.85,
                );
            }
        }

        if let Some(rag) = derived_rag {
            if !rag.detailed_facts.is_empty() {
                for (idx, fact) in rag.detailed_facts.iter().enumerate() {
                    let score = (0.95 - (idx as f32 * 0.02)).max(0.50);
                    bundle.push_fact(
                        &fact.text,
                        Provenance::RagDocument {
                            source_file: fact.source_file.clone(),
                            page_number: fact.page_number,
                            score,
                        },
                        score,
                    );
                }
            } else {
                for (idx, fact) in rag.facts.iter().enumerate() {
                    let score = (0.95 - (idx as f32 * 0.02)).max(0.50);
                    bundle.push_fact(
                        fact,
                        Provenance::RagDocument {
                            source_file: String::new(),
                            page_number: None,
                            score,
                        },
                        score,
                    );
                }
            }
        } else if let Some(idx) = prompt.context.background.find("Known facts: ") {
            if let Some(rest) = prompt.context.background.get(idx + "Known facts: ".len()..) {
                for fact in rest.split(';') {
                    bundle.push_fact(
                        fact,
                        Provenance::RagDocument {
                            source_file: String::new(),
                            page_number: None,
                            score: 0.75,
                        },
                        0.75,
                    );
                }
            }
        }

        for sentence in split_sentences_preserving_decimals(&user_prompt) {
            if let Some(decl_fact) = extract_declarative_user_fact(&sentence) {
                bundle.push_fact(decl_fact, Provenance::UserVerbatim, 0.95);
            }
        }

        bundle
    }

    /// Adds a declarative fact sentence after screening for prompt injection and XML-escaping.
    ///
    /// Returns `true` if the fact was accepted, or `false` if rejected (empty, too short, or injection).
    ///
    /// # Panics
    /// Never.
    pub fn push_fact(&mut self, text: &str, provenance: Provenance, relevance_score: f32) -> bool {
        let trimmed = text.trim().trim_end_matches('.');
        if trimmed.len() < 4
            || trimmed.starts_with("[Source:")
            || contains_prompt_injection(trimmed)
        {
            return false;
        }
        self.items.push(EvidenceItem {
            id: String::new(),
            kind: EvidenceKind::Fact,
            text: sanitize_untrusted_fact(trimmed),
            provenance,
            relevance_score: relevance_score.clamp(0.0, 1.0),
        });
        true
    }

    /// Adds a structured key-value datum (e.g. user profile, metric, or API field) after injection
    /// screening and XML-escaping.
    ///
    /// # Panics
    /// Never.
    pub fn push_key_value(
        &mut self,
        key: &str,
        value: &str,
        provenance: Provenance,
        relevance_score: f32,
    ) -> bool {
        let k = key.trim();
        let v = value.trim();
        if k.is_empty()
            || v.is_empty()
            || contains_prompt_injection(k)
            || contains_prompt_injection(v)
        {
            return false;
        }
        let safe_k = sanitize_untrusted_fact(k);
        let safe_v = sanitize_untrusted_fact(v);
        self.items.push(EvidenceItem {
            id: String::new(),
            kind: EvidenceKind::KeyValue {
                key: safe_k.clone(),
            },
            text: format!("{safe_k}: {safe_v}"),
            provenance,
            relevance_score: relevance_score.clamp(0.0, 1.0),
        });
        true
    }

    /// Adds a structured table row (paired with column headers) after injection screening and
    /// XML-escaping.
    ///
    /// # Panics
    /// Never.
    pub fn push_table_row(
        &mut self,
        table_name: &str,
        columns: &[&str],
        values: &[&str],
        provenance: Provenance,
        relevance_score: f32,
    ) -> bool {
        let tname = table_name.trim();
        if tname.is_empty() || columns.is_empty() || columns.len() != values.len() {
            return false;
        }
        if contains_prompt_injection(tname)
            || columns.iter().any(|c| contains_prompt_injection(c))
            || values.iter().any(|v| contains_prompt_injection(v))
        {
            return false;
        }
        let safe_cols: Vec<String> = columns
            .iter()
            .map(|c| sanitize_untrusted_fact(c.trim()))
            .collect();
        let safe_pairs: Vec<String> = safe_cols
            .iter()
            .zip(values.iter())
            .map(|(col, val)| format!("{col}={}", sanitize_untrusted_fact(val.trim())))
            .collect();
        let safe_table = sanitize_untrusted_fact(tname);
        self.items.push(EvidenceItem {
            id: String::new(),
            kind: EvidenceKind::TableRow {
                table_name: safe_table.clone(),
                columns: safe_cols,
            },
            text: format!("{safe_table} | {}", safe_pairs.join(" | ")),
            provenance,
            relevance_score: relevance_score.clamp(0.0, 1.0),
        });
        true
    }

    /// Returns the total number of raw (unpacked) evidence items in the bundle.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if no evidence items have been added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Deduplicates, ranks by `(provenance tier, relevance_score)`, packs within `budget`,
    /// assigns stable `[F1]..[Fn]` IDs, and detects deterministic numeric conflicts.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn pack(&self, budget: &PromptBudget) -> PackedEvidence {
        if self.items.is_empty() || budget.max_items == 0 || budget.max_evidence_tokens == 0 {
            return PackedEvidence {
                omitted_count: self.items.len(),
                ..PackedEvidence::default()
            };
        }

        // 1. Rank deterministically: provenance tier ASC, relevance_score DESC, insertion index ASC.
        let mut indexed: Vec<(usize, &EvidenceItem)> = self.items.iter().enumerate().collect();
        indexed.sort_by(|(idx_a, a), (idx_b, b)| {
            a.provenance
                .priority_tier()
                .cmp(&b.provenance.priority_tier())
                .then_with(|| {
                    b.relevance_score
                        .partial_cmp(&a.relevance_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| idx_a.cmp(idx_b))
        });

        // 2. Deduplicate and greedily pack within token & item budgets.
        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut selected: Vec<EvidenceItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        let mut omitted_count: usize = 0;

        for (_, item) in indexed {
            let norm_key: String = item
                .text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            if norm_key.is_empty() || !seen_keys.insert(norm_key) {
                continue;
            }

            let item_tokens = crate::utils::tokens::estimate_tokens(&item.text)
                .saturating_add(crate::utils::tokens::estimate_tokens(
                    &item.provenance.format_label(),
                ))
                .max(1);

            if selected.len() >= budget.max_items
                || (tokens_used.saturating_add(item_tokens) > budget.max_evidence_tokens
                    && !selected.is_empty())
            {
                omitted_count += 1;
                continue;
            }

            let mut cloned = item.clone();
            cloned.id = format!("F{}", selected.len() + 1);
            tokens_used = tokens_used.saturating_add(item_tokens);
            selected.push(cloned);
        }

        let conflicts = detect_numeric_conflicts(&selected);

        PackedEvidence {
            items: selected,
            conflicts,
            tokens_used,
            omitted_count,
        }
    }
}

/// Extracts normalized numeric tokens (containing at least one ASCII digit, preserving `$`, `%`, `.`)
/// from an evidence string for contradiction detection.
fn extract_numeric_tokens(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '.' && c != '$' && c != '%' && c != '-')
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric() && c != '$' && c != '%')
                .to_lowercase()
        })
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
        .collect()
}

/// Extracts meaningful domain content terms (`len >= 4`, non-numeric, non-stopword) from an
/// evidence string for linking potentially conflicting claims.
fn extract_domain_terms(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphabetic())
        .map(str::to_lowercase)
        .filter(|w| {
            w.len() >= 4
                && !matches!(
                    w.as_str(),
                    "this"
                        | "that"
                        | "with"
                        | "from"
                        | "they"
                        | "have"
                        | "were"
                        | "been"
                        | "about"
                        | "there"
                        | "their"
                        | "which"
                        | "where"
                        | "when"
                        | "total"
                        | "reported"
                        | "according"
                        | "source"
                )
        })
        .collect()
}

/// Extracts temporal/period/version qualifiers (e.g. `q1`, `q2`, `2025`, `2026`, `fy24`, `v2`)
/// so metrics from distinct time periods are not falsely flagged as contradictions.
fn extract_period_qualifiers(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| {
            matches!(w.as_str(), "q1" | "q2" | "q3" | "q4")
                || (w.len() == 4
                    && w.chars().all(|c| c.is_ascii_digit())
                    && (w.starts_with("19") || w.starts_with("20")))
                || (w.starts_with("fy")
                    && w.len() >= 4
                    && w[2..].chars().all(|c| c.is_ascii_digit()))
                || (w.starts_with('v')
                    && w.len() >= 2
                    && w[1..].chars().all(|c| c.is_ascii_digit()))
        })
        .collect()
}

/// Deterministically flags pairs of packed evidence items that share >= 2 domain terms but
/// report disjoint numeric values (e.g., `"Q3 revenue was $4.2M"` vs `"Q3 revenue was $4.8M"`).
fn detect_numeric_conflicts(items: &[EvidenceItem]) -> Vec<EvidenceConflict> {
    let mut conflicts = Vec::new();
    let signatures: Vec<(HashSet<String>, HashSet<String>, HashSet<String>)> = items
        .iter()
        .map(|it| {
            (
                extract_domain_terms(&it.text),
                extract_numeric_tokens(&it.text),
                extract_period_qualifiers(&it.text),
            )
        })
        .collect();

    for i in 0..items.len() {
        let (ref terms_i, ref nums_i, ref periods_i) = signatures[i];
        if nums_i.is_empty() || terms_i.len() < 2 {
            continue;
        }
        for j in (i + 1)..items.len() {
            let (ref terms_j, ref nums_j, ref periods_j) = signatures[j];
            if nums_j.is_empty() || terms_j.len() < 2 {
                continue;
            }
            // Distinct time periods or versions (e.g. Q1 vs Q2, 2025 vs 2026) are not conflicts.
            if !periods_i.is_empty() && !periods_j.is_empty() && periods_i.is_disjoint(periods_j) {
                continue;
            }
            let mut shared: Vec<String> = terms_i.intersection(terms_j).cloned().collect();
            if shared.len() < 2 {
                continue;
            }
            // Flag when the two items carry different numeric sets (neither is a subset of the other).
            if nums_i != nums_j && !nums_i.is_subset(nums_j) && !nums_j.is_subset(nums_i) {
                shared.sort();
                let mut left_nums: Vec<&str> = nums_i.iter().map(String::as_str).collect();
                let mut right_nums: Vec<&str> = nums_j.iter().map(String::as_str).collect();
                left_nums.sort_unstable();
                right_nums.sort_unstable();
                conflicts.push(EvidenceConflict {
                    left_id: items[i].id.clone(),
                    right_id: items[j].id.clone(),
                    shared_terms: shared.clone(),
                    reason: format!(
                        "Conflicting numeric values on [{}]: {} reports [{}] vs {} reports [{}]",
                        shared.join(", "),
                        items[i].id,
                        left_nums.join(", "),
                        items[j].id,
                        right_nums.join(", "),
                    ),
                });
            }
        }
    }

    conflicts
}
