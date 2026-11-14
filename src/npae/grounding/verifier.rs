//! Deterministic verifier and text helpers for Aggressive-mode Grounding Harness (`GAP-42`).
//!
//! Implements `ARCHITECTURE.md` §5 (`GAP-42`) and §7.2 (`Layer 4`):
//! validates that an external LLM's response obeys the `<output_contract>` table, cites only
//! existing `[F#]` identifiers, grounds numbers and dates verbatim in cited facts, explicitly
//! reports missing data as `NOT_FOUND` without speculation, and never relies on prompt-injected
//! instructions embedded inside untrusted facts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use thiserror::Error;

/// Minimum lexical concept overlap required between an answer row and its cited `[F#]` facts
/// for a `FOUND` claim to be accepted as grounded.
const FOUND_MIN_LEXICAL_SUPPORT: f32 = 0.34;

/// Prompt-injection trigger substrings screened in untrusted fact lines (Rules §9).
const INJECTION_MARKERS: &[&str] = &[
    "ignore previous",
    "ignore all previous",
    "ignore instructions",
    "disregard previous",
    "disregard all",
    "system override",
    "system prompt",
    "new instructions",
    "you must now",
    "forget previous",
    "developer mode",
];

/// Errors returned when a grounding verification request is malformed.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GroundingError {
    /// The caller submitted an empty or whitespace-only `llm_answer`.
    #[error("llm_answer must not be empty")]
    EmptyAnswer,
}

/// A single fact entry assigned a stable `[F#]` identifier inside `<facts trust="untrusted">`.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct NumberedFact {
    /// Fact identifier without brackets, e.g. `"F1"`, `"F2"`.
    pub id: String,
    /// XML-escaped, single-line fact statement.
    pub text: String,
}

/// Request payload for `POST /api/npae/verify`.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct VerifyGroundingRequest {
    /// Plain or `[F#]`-prefixed fact strings supplied to the prompt.
    #[serde(default)]
    pub facts: Vec<String>,
    /// Pre-numbered facts (for example `StructuredPromptResponse.grounding_facts`).
    #[serde(default)]
    pub numbered_facts: Vec<NumberedFact>,
    /// Optional rendered harness prompt; if provided, `<facts>` and `<requested_items>`
    /// are extracted automatically when `facts`/`numbered_facts` or `requested_items` are empty.
    #[serde(default)]
    pub harness_prompt: Option<String>,
    /// Optional explicit list of requested items expected in the output table.
    #[serde(default)]
    pub requested_items: Vec<String>,
    /// Downstream LLM response to verify.
    pub llm_answer: String,
}

/// Normalized status reported by a single row of the `<output_contract>` table.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClaimStatus {
    /// Explicitly supported by one or more cited `[F#]` facts.
    Found,
    /// Partially or approximately supported by cited `[F#]` facts with missing details stated.
    PartialMatch,
    /// Absent from supplied facts ("Not present in provided data").
    NotFound,
}

/// Machine-readable reason explaining why a claim failed deterministic grounding verification.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimIssueKind {
    /// Row used a status token outside `FOUND | PARTIAL_MATCH | NOT_FOUND`.
    InvalidStatus,
    /// `FOUND` or `PARTIAL_MATCH` row omitted `[F#]` citations.
    MissingCitation,
    /// Row cited a fact ID (e.g. `F99`) that does not exist in supplied facts.
    NonExistentFactId,
    /// Answer contained a number, percentage, currency amount, or date absent from cited facts.
    UnverifiedNumberOrDate,
    /// Answer content words lacked sufficient lexical overlap with the cited `[F#]` text.
    InsufficientLexicalSupport,
    /// `PARTIAL_MATCH` row failed to state what was missing or unmatched.
    MissingGapExplanation,
    /// `NOT_FOUND` row smuggled speculative numbers or ungrounded factual claims.
    SpeculationInNotFound,
    /// Claim relied on a quarantined prompt-injection clause or echoed injected instructions.
    PromptInjectionDetected,
    /// Free-form claim appeared outside the `<output_contract>` table without fact support.
    UngroundedFreeformClaim,
}

/// A verified row that satisfied its `FOUND`, `PARTIAL_MATCH`, or `NOT_FOUND` contract.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct VerifiedItem {
    /// Requested item label from the table row.
    pub item: String,
    /// Verified status (`FOUND`, `PARTIAL_MATCH`, or `NOT_FOUND`).
    pub status: ClaimStatus,
    /// Answer text supplied by the LLM for this row.
    pub answer: String,
    /// Validated fact IDs cited by this row (empty for `NOT_FOUND`).
    pub cited_facts: Vec<String>,
    /// Lexical support ratio in `[0.0, 1.0]` against cited facts (`1.0` for valid `NOT_FOUND`).
    pub support_score: f32,
}

/// A claim or row that failed deterministic grounding verification.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedClaim {
    /// Item label or `"<freeform>"` when detected outside the contract table.
    pub item: String,
    /// Offending answer or sentence text.
    pub claim: String,
    /// Fact IDs cited on the offending row, if any.
    pub cited_facts: Vec<String>,
    /// Primary failure classification.
    pub issue: ClaimIssueKind,
    /// Human-readable diagnostic detail (e.g. which number or fact ID failed).
    pub detail: String,
}

/// Complete deterministic verification outcome returned by [`verify_grounding`].
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct GroundingReport {
    /// `true` iff the contract table was present, every row passed, and no requested items were omitted.
    pub passed: bool,
    /// `true` when `llm_answer` failed to provide a parseable `<output_contract>` table.
    pub contract_violated: bool,
    /// Verified `FOUND` items backed by cited `[F#]` facts.
    pub grounded: Vec<VerifiedItem>,
    /// Verified `PARTIAL_MATCH` items backed by cited `[F#]` facts plus an explicit gap note.
    pub partial: Vec<VerifiedItem>,
    /// Verified `NOT_FOUND` items that abstained cleanly without inventing facts.
    pub not_found: Vec<VerifiedItem>,
    /// Claims or rows that violated citation, numeric/date, lexical, or injection checks.
    pub unsupported_claims: Vec<UnsupportedClaim>,
    /// Requested items that were absent from the LLM's response table.
    pub missing_items: Vec<String>,
    /// Overall faithfulness score in `[0.0, 1.0]`.
    pub grounding_score: f32,
}

/// Escapes `<` and `>` inside untrusted fact strings so embedded tags such as `</facts>` or
/// `<rules>` cannot break out of the `<facts trust="untrusted">` XML fence.
///
/// # Panics
/// Never.
#[must_use]
pub fn sanitize_untrusted_fact(raw: &str) -> String {
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace('<', "&lt;").replace('>', "&gt;")
}

/// Returns `true` if `text` contains a known prompt-injection override phrase.
///
/// # Panics
/// Never.
#[must_use]
pub fn contains_prompt_injection(text: &str) -> bool {
    let lower = text.to_lowercase();
    INJECTION_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Splits `text` into sentences on `.`, `!`, `?`, and newlines while preserving decimal numbers
/// such as `$4.2M` or `99.95%`.
///
/// # Panics
/// Never.
#[must_use]
pub fn split_sentences_preserving_decimals(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut current = String::new();

    for (idx, &ch) in chars.iter().enumerate() {
        if ch == '\n' || ch == '!' || ch == '?' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                // Keep trailing '?' so callers can distinguish questions from declarative facts.
                let mut entry = trimmed.to_string();
                if ch == '?' {
                    entry.push('?');
                }
                sentences.push(entry);
            }
            current.clear();
            continue;
        }

        if ch == '.' {
            let prev_is_digit = idx > 0 && chars.get(idx - 1).is_some_and(char::is_ascii_digit);
            let next_is_digit = chars.get(idx + 1).is_some_and(char::is_ascii_digit);
            if prev_is_digit && next_is_digit {
                current.push(ch);
                continue;
            }
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let trailing = current.trim();
    if !trailing.is_empty() {
        sentences.push(trailing.to_string());
    }
    sentences
}

/// Parses `<facts ...>...</facts>` from a rendered harness prompt into [`NumberedFact`] entries.
///
/// Ignores the `[NONE]` sentinel emitted when a prompt had zero supplied facts.
///
/// # Panics
/// Never.
#[must_use]
pub fn parse_facts_from_harness_prompt(prompt: &str) -> Vec<NumberedFact> {
    // FIX: `<rules>` references `<facts>` inline in prose (`Use ONLY the facts inside <facts>.`).
    // Match only standalone `<facts...>` / `</facts>` tag lines so `<rules>` prose is never parsed.
    let mut in_facts_block = false;
    let mut out = Vec::new();
    let mut auto_idx = 1usize;

    for line in prompt.lines() {
        let trimmed = line.trim();
        if !in_facts_block {
            if trimmed.starts_with("<facts")
                && trimmed.ends_with('>')
                && !trimmed.contains("</facts>")
            {
                in_facts_block = true;
            }
            continue;
        }
        if trimmed == "</facts>" {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with("[NONE]") {
            continue;
        }
        if let Some((id, rest)) = parse_bracketed_fact_prefix(trimmed) {
            if !rest.is_empty() {
                out.push(NumberedFact {
                    id,
                    text: rest.to_string(),
                });
            }
        } else {
            out.push(NumberedFact {
                id: format!("F{auto_idx}"),
                text: trimmed.to_string(),
            });
            auto_idx += 1;
        }
    }
    out
}

/// Parses `<requested_items>...</requested_items>` from a rendered harness prompt.
///
/// # Panics
/// Never.
#[must_use]
pub fn parse_requested_items_from_harness_prompt(prompt: &str) -> Vec<String> {
    // FIX: `<rules>` mentions `<requested_items>` inline (`For each entry in <requested_items>,`).
    // Match only standalone `<requested_items>` / `</requested_items>` tag lines.
    let mut in_block = false;
    let mut items = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed == "<requested_items>" {
                in_block = true;
            }
            continue;
        }
        if trimmed == "</requested_items>" {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = strip_numbered_list_prefix(trimmed);
        if !cleaned.is_empty() {
            items.push(cleaned.to_string());
        }
    }
    items
}

/// Verifies a downstream LLM answer against numbered grounding facts and the harness contract.
///
/// // ARCH: ARCHITECTURE.md §5 GAP-42 & §7.2 Layer 4 — deterministic, LLM-free enforcement of
/// // `FOUND` / `PARTIAL_MATCH` / `NOT_FOUND`, citation existence, lexical overlap, and verbatim
/// // numeric/date grounding.
///
/// # Errors
/// Returns [`GroundingError::EmptyAnswer`] if `req.llm_answer` is empty or whitespace.
///
/// # Panics
/// Never.
pub fn verify_grounding(req: &VerifyGroundingRequest) -> Result<GroundingReport, GroundingError> {
    if req.llm_answer.trim().is_empty() {
        return Err(GroundingError::EmptyAnswer);
    }

    let (clean_facts, quarantined_facts) = build_fact_maps(req);
    let requested_items = resolve_requested_items(req);
    let allowed_request_numbers = collect_numeric_tokens_for_slice(&requested_items);

    let (rows, non_table_lines) = parse_contract_table(&req.llm_answer);

    let mut grounded = Vec::new();
    let mut partial = Vec::new();
    let mut not_found = Vec::new();
    let mut unsupported_claims = Vec::new();

    let contract_violated = rows.is_empty();

    if contract_violated {
        // Fallback: LLM disobeyed `<output_contract>`. Audit every declarative sentence in prose.
        verify_freeform_lines(
            &non_table_lines,
            &clean_facts,
            &quarantined_facts,
            &allowed_request_numbers,
            &mut unsupported_claims,
        );
    } else {
        for row in &rows {
            evaluate_table_row(
                row,
                &clean_facts,
                &quarantined_facts,
                &allowed_request_numbers,
                &mut grounded,
                &mut partial,
                &mut not_found,
                &mut unsupported_claims,
            );
        }

        // Audit prose outside the table for fabricated numbers/dates or injection echoes.
        audit_trailing_prose(
            &non_table_lines,
            &clean_facts,
            &quarantined_facts,
            &allowed_request_numbers,
            &mut unsupported_claims,
        );
    }

    let missing_items = find_missing_requested_items(&requested_items, &rows);

    let valid_count = grounded.len() + partial.len() + not_found.len();
    let penalty_count = unsupported_claims.len() + missing_items.len();
    let denom = valid_count + penalty_count;

    let grounding_score = if contract_violated || denom == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let score = valid_count as f32 / denom as f32;
        score.clamp(0.0, 1.0)
    };

    let passed = !contract_violated
        && unsupported_claims.is_empty()
        && missing_items.is_empty()
        && valid_count > 0;

    Ok(GroundingReport {
        passed,
        contract_violated,
        grounded,
        partial,
        not_found,
        unsupported_claims,
        missing_items,
        grounding_score,
    })
}

// ── Internal Parsing & Verification Helpers ──────────────────────────────────

#[derive(Debug, Clone)]
struct ParsedRow {
    index_cell: String,
    item: String,
    status_raw: String,
    answer: String,
    source_raw: String,
}

/// Builds `(clean_facts, quarantined_injection_facts)` keyed by uppercase fact ID (`"F1"`).
///
/// If a single fact string mixes a benign clause with an injected imperative clause, the
/// injected clause is quarantined into `quarantined_facts` while benign clauses remain in
/// `clean_facts`, preventing untrusted injections from acting as grounding evidence.
fn build_fact_maps(
    req: &VerifyGroundingRequest,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let raw_numbered: Vec<NumberedFact> = if !req.numbered_facts.is_empty() {
        req.numbered_facts.clone()
    } else if !req.facts.is_empty() {
        req.facts
            .iter()
            .enumerate()
            .filter_map(|(idx, f)| {
                let trimmed = f.trim();
                if trimmed.is_empty() || trimmed.starts_with("[NONE]") {
                    return None;
                }
                if let Some((id, rest)) = parse_bracketed_fact_prefix(trimmed) {
                    Some(NumberedFact {
                        id,
                        text: rest.to_string(),
                    })
                } else {
                    Some(NumberedFact {
                        id: format!("F{}", idx + 1),
                        text: trimmed.to_string(),
                    })
                }
            })
            .collect()
    } else if let Some(ref prompt) = req.harness_prompt {
        parse_facts_from_harness_prompt(prompt)
    } else {
        Vec::new()
    };

    let mut clean = BTreeMap::new();
    let mut quarantined = BTreeMap::new();

    for entry in raw_numbered {
        let id = entry.id.trim().to_uppercase();
        let clauses = split_sentences_preserving_decimals(&entry.text);
        let mut benign_parts = Vec::new();
        let mut injected_parts = Vec::new();

        for clause in clauses {
            if contains_prompt_injection(&clause) {
                injected_parts.push(clause);
            } else {
                benign_parts.push(clause);
            }
        }

        if !benign_parts.is_empty() {
            clean.insert(id.clone(), benign_parts.join(". "));
        }
        if !injected_parts.is_empty() {
            quarantined.insert(id, injected_parts.join(". "));
        }
    }

    (clean, quarantined)
}

fn resolve_requested_items(req: &VerifyGroundingRequest) -> Vec<String> {
    if !req.requested_items.is_empty() {
        return req
            .requested_items
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(ref prompt) = req.harness_prompt {
        return parse_requested_items_from_harness_prompt(prompt);
    }
    Vec::new()
}

fn parse_bracketed_fact_prefix(line: &str) -> Option<(String, &str)> {
    let rest_after_open = line.strip_prefix('[')?;
    let close_idx = rest_after_open.find(']')?;
    let id_candidate = rest_after_open.get(..close_idx)?.trim();
    let first = id_candidate.chars().next()?;
    if (first == 'F' || first == 'f')
        && id_candidate.len() >= 2
        && id_candidate.get(1..)?.chars().all(|c| c.is_ascii_digit())
    {
        let remainder = rest_after_open.get(close_idx + 1..)?.trim();
        return Some((id_candidate.to_uppercase(), remainder));
    }
    None
}

fn strip_numbered_list_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && idx < bytes.len() && (bytes[idx] == b'.' || bytes[idx] == b')') {
        if let Some(rest) = line.get(idx + 1..) {
            return rest.trim();
        }
    }
    if let Some(rest) = line.strip_prefix("- ") {
        return rest.trim();
    }
    line
}

fn parse_contract_table(llm_answer: &str) -> (Vec<ParsedRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut non_table_lines = Vec::new();

    for line in llm_answer.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2 {
            let inner = trimmed.get(1..trimmed.len() - 1).unwrap_or("");
            let cells: Vec<&str> = inner.split('|').map(str::trim).collect();

            // Skip markdown alignment separator rows such as `|---|---|---|---|---|`.
            let is_separator = cells.iter().all(|cell| {
                !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':' || c == ' ')
            });
            if is_separator {
                continue;
            }

            // Skip the header row (`| # | Item | Status | Answer | Source |`).
            if cells.len() >= 4 {
                let c0 = cells.first().copied().unwrap_or("").to_lowercase();
                let c1 = cells.get(1).copied().unwrap_or("").to_lowercase();
                let c2 = cells.get(2).copied().unwrap_or("").to_lowercase();
                if (c0 == "#" || c0 == "id" || c0 == "no")
                    && c1.contains("item")
                    && c2.contains("status")
                {
                    continue;
                }
            }

            if cells.len() >= 5 {
                rows.push(ParsedRow {
                    index_cell: cells.first().copied().unwrap_or("").to_string(),
                    item: cells.get(1).copied().unwrap_or("").to_string(),
                    status_raw: cells.get(2).copied().unwrap_or("").to_string(),
                    answer: cells.get(3).copied().unwrap_or("").to_string(),
                    source_raw: cells.get(4).copied().unwrap_or("").to_string(),
                });
                continue;
            } else if cells.len() == 4 {
                // Tolerate tables omitting the leading `#` index column:
                // `| Item | Status | Answer | Source |`.
                let c0_low = cells.first().copied().unwrap_or("").to_lowercase();
                let c1_low = cells.get(1).copied().unwrap_or("").to_lowercase();
                if c0_low == "item" && c1_low == "status" {
                    continue;
                }
                rows.push(ParsedRow {
                    index_cell: String::new(),
                    item: cells.first().copied().unwrap_or("").to_string(),
                    status_raw: cells.get(1).copied().unwrap_or("").to_string(),
                    answer: cells.get(2).copied().unwrap_or("").to_string(),
                    source_raw: cells.get(3).copied().unwrap_or("").to_string(),
                });
                continue;
            }
        }

        non_table_lines.push(trimmed.to_string());
    }

    (rows, non_table_lines)
}

fn parse_claim_status(raw: &str) -> Option<ClaimStatus> {
    let upper = raw.trim().to_uppercase();
    // Strip optional inline citation suffix such as `FOUND [F1]` or markdown bold `**FOUND**`.
    let cleaned = upper.replace(['*', '`'], "");
    let token = cleaned
        .split(|c: char| c.is_whitespace() || c == '[')
        .next()
        .unwrap_or("");
    match token {
        "FOUND" => Some(ClaimStatus::Found),
        "PARTIAL_MATCH" | "PARTIAL" => Some(ClaimStatus::PartialMatch),
        "NOT_FOUND" | "NOTFOUND" => Some(ClaimStatus::NotFound),
        _ => None,
    }
}

/// Extracts ordered, deduplicated fact citations (`"F1"`, `"F2"`, …) from any string.
///
/// # Panics
/// Never.
#[must_use]
pub fn extract_fact_citations(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut idx = 0;

    while idx < chars.len() {
        let ch = chars[idx];
        let prev_ok = idx == 0
            || chars
                .get(idx - 1)
                .is_none_or(|p| !p.is_ascii_alphanumeric());
        if prev_ok && (ch == 'F' || ch == 'f') {
            let mut end = idx + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let next_ok =
                end == chars.len() || chars.get(end).is_none_or(|n| !n.is_ascii_alphabetic());
            if end > idx + 1 && next_ok {
                let digits: String = chars[idx + 1..end].iter().collect();
                let id = format!("F{digits}");
                if seen.insert(id.clone()) {
                    ordered.push(id);
                }
                idx = end;
                continue;
            }
        }
        idx += 1;
    }

    ordered
}

/// Extracts normalized numeric and ISO-date tokens from `text`, ignoring `[F#]` fact citations.
///
/// When `include_date_parts` is `true` (used when indexing facts), hyphenated dates like
/// `2026-10-15` register both `"2026-10-15"` and their year `"2026"` so truthful year-level
/// summaries succeed while invented dates (`2026-11-15`) still fail exact match.
fn extract_numeric_tokens(text: &str, include_date_parts: bool) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = HashSet::new();
    let mut idx = 0;

    while idx < chars.len() {
        // Skip `F<digits>` citations so `[F1]` is never treated as numeric claim `1`.
        let prev_non_alnum = idx == 0
            || chars
                .get(idx - 1)
                .is_none_or(|p| !p.is_ascii_alphanumeric());
        if prev_non_alnum && (chars[idx] == 'F' || chars[idx] == 'f') {
            let mut f_end = idx + 1;
            while f_end < chars.len() && chars[f_end].is_ascii_digit() {
                f_end += 1;
            }
            let after_ok =
                f_end == chars.len() || chars.get(f_end).is_none_or(|n| !n.is_ascii_alphabetic());
            if f_end > idx + 1 && after_ok {
                idx = f_end;
                continue;
            }
        }

        if chars[idx].is_ascii_digit() {
            let mut raw_num = String::new();
            let mut end = idx;
            while end < chars.len() {
                let c = chars[end];
                if c.is_ascii_digit() {
                    raw_num.push(c);
                    end += 1;
                } else if c == ',' && chars.get(end + 1).is_some_and(char::is_ascii_digit) {
                    // Strip thousands comma (`1,800` -> `1800`).
                    end += 1;
                } else if (c == '.' || c == '-' || c == '/')
                    && chars.get(end + 1).is_some_and(char::is_ascii_digit)
                {
                    raw_num.push(c);
                    end += 1;
                } else {
                    break;
                }
            }

            if !raw_num.is_empty() {
                if include_date_parts && (raw_num.contains('-') || raw_num.contains('/')) {
                    for part in raw_num.split(&['-', '/'][..]) {
                        if part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()) {
                            out.insert(part.to_string());
                        }
                    }
                }
                out.insert(raw_num);
            }
            idx = end;
            continue;
        }

        idx += 1;
    }

    out
}

fn collect_numeric_tokens_for_slice(items: &[String]) -> HashSet<String> {
    let mut set = HashSet::new();
    for item in items {
        set.extend(extract_numeric_tokens(item, true));
    }
    set
}

const STEM_STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "from",
    "that",
    "this",
    "these",
    "those",
    "into",
    "onto",
    "over",
    "under",
    "about",
    "after",
    "before",
    "between",
    "within",
    "without",
    "was",
    "were",
    "are",
    "been",
    "being",
    "have",
    "has",
    "had",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "must",
    "can",
    "not",
    "only",
    "also",
    "very",
    "more",
    "most",
    "some",
    "such",
    "than",
    "then",
    "there",
    "their",
    "they",
    "them",
    "our",
    "your",
    "his",
    "her",
    "its",
    "what",
    "which",
    "when",
    "where",
    "who",
    "whom",
    "how",
    "why",
    "present",
    "provided",
    "data",
    "stated",
    "fact",
    "facts",
    "found",
    "partial",
    "match",
    "missing",
    "specified",
    "unspecified",
    "available",
    "unavailable",
    "source",
    "based",
    "per",
];

/// Extracts normalized lowercase content stems (length >= 3, excluding stop-words, citations,
/// and pure numbers) for lexical grounding overlap checks.
fn extract_content_stems(text: &str) -> Vec<String> {
    let mut stems = Vec::new();
    for raw_word in text.split(|c: char| !c.is_alphanumeric()) {
        let lower = raw_word.trim().to_lowercase();
        if lower.len() < 3 {
            continue;
        }
        // Skip `f1`, `f2`, ... citations and pure digit tokens.
        if let Some(digits) = lower.strip_prefix('f') {
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
        }
        if lower.chars().all(|c| c.is_ascii_digit()) || STEM_STOP_WORDS.contains(&lower.as_str()) {
            continue;
        }
        stems.push(light_stem(&lower));
    }
    stems
}

fn light_stem(word: &str) -> String {
    for suffix in [
        "ing", "ers", "ies", "ied", "ally", "ation", "ments", "ment", "ness", "ly", "ed", "es", "s",
    ] {
        if word.len() >= suffix.len() + 4 {
            if let Some(stripped) = word.strip_suffix(suffix) {
                return stripped.to_string();
            }
        }
    }
    word.to_string()
}

fn stems_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let min_len = a.len().min(b.len());
    if min_len >= 5 {
        let prefix = min_len.min(6);
        return a.get(..prefix) == b.get(..prefix);
    }
    false
}

fn compute_lexical_support(query_stems: &[String], fact_stems: &[String]) -> f32 {
    if query_stems.is_empty() {
        return 0.0;
    }
    let matched = query_stems
        .iter()
        .filter(|q| fact_stems.iter().any(|f| stems_match(q, f)))
        .count();
    #[allow(clippy::cast_precision_loss)]
    {
        matched as f32 / query_stems.len() as f32
    }
}

fn indicates_partial_gap(answer: &str) -> bool {
    const GAP_CUES: &[&str] = &[
        "missing",
        "not ",
        "not_",
        "no ",
        "none",
        "only",
        "partial",
        "unspecified",
        "unknown",
        "unconfirmed",
        "unavailable",
        "absent",
        "lack",
        "without",
        "however",
        "but ",
        "except",
        "approximate",
        "doesn't",
        "does not",
        ";",
    ];
    let lower = answer.to_lowercase();
    GAP_CUES.iter().any(|cue| lower.contains(cue))
}

struct RowOutcome {
    grounded: Option<VerifiedItem>,
    partial: Option<VerifiedItem>,
    not_found: Option<VerifiedItem>,
    unsupported: Option<UnsupportedClaim>,
}

impl RowOutcome {
    fn fail(
        item: &str,
        claim: &str,
        cited_facts: Vec<String>,
        issue: ClaimIssueKind,
        detail: String,
    ) -> Self {
        Self {
            grounded: None,
            partial: None,
            not_found: None,
            unsupported: Some(UnsupportedClaim {
                item: item.to_string(),
                claim: claim.to_string(),
                cited_facts,
                issue,
                detail,
            }),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_table_row(
    row: &ParsedRow,
    clean_facts: &BTreeMap<String, String>,
    quarantined_facts: &BTreeMap<String, String>,
    allowed_request_numbers: &HashSet<String>,
    grounded: &mut Vec<VerifiedItem>,
    partial: &mut Vec<VerifiedItem>,
    not_found: &mut Vec<VerifiedItem>,
    unsupported_claims: &mut Vec<UnsupportedClaim>,
) {
    let outcome = classify_table_row(row, clean_facts, quarantined_facts, allowed_request_numbers);
    if let Some(g) = outcome.grounded {
        grounded.push(g);
    }
    if let Some(p) = outcome.partial {
        partial.push(p);
    }
    if let Some(nf) = outcome.not_found {
        not_found.push(nf);
    }
    if let Some(u) = outcome.unsupported {
        unsupported_claims.push(u);
    }
}

struct FactVerificationCtx<'a> {
    clean_facts: &'a BTreeMap<String, String>,
    quarantined_facts: &'a BTreeMap<String, String>,
    item_numbers: &'a HashSet<String>,
    allowed_request_numbers: &'a HashSet<String>,
}

fn classify_table_row(
    row: &ParsedRow,
    clean_facts: &BTreeMap<String, String>,
    quarantined_facts: &BTreeMap<String, String>,
    allowed_request_numbers: &HashSet<String>,
) -> RowOutcome {
    let mut citations = extract_fact_citations(&row.source_raw);
    for extra in extract_fact_citations(&row.status_raw)
        .into_iter()
        .chain(extract_fact_citations(&row.answer))
    {
        if !citations.contains(&extra) {
            citations.push(extra);
        }
    }

    let Some(status) = parse_claim_status(&row.status_raw) else {
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::InvalidStatus,
            format!(
                "Invalid status '{}'; expected FOUND, PARTIAL_MATCH, or NOT_FOUND",
                row.status_raw
            ),
        );
    };

    if contains_prompt_injection(&row.answer) {
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::PromptInjectionDetected,
            "Answer contains prompt-injection override language".to_string(),
        );
    }

    let item_numbers = extract_numeric_tokens(&row.item, true);
    let ctx = FactVerificationCtx {
        clean_facts,
        quarantined_facts,
        item_numbers: &item_numbers,
        allowed_request_numbers,
    };
    match status {
        ClaimStatus::NotFound => {
            classify_not_found_row(row, citations, &item_numbers, allowed_request_numbers)
        }
        ClaimStatus::Found | ClaimStatus::PartialMatch => {
            classify_cited_row(row, status, citations, &ctx)
        }
    }
}

fn classify_not_found_row(
    row: &ParsedRow,
    citations: Vec<String>,
    item_numbers: &HashSet<String>,
    allowed_request_numbers: &HashSet<String>,
) -> RowOutcome {
    let invented_nums: Vec<String> = extract_numeric_tokens(&row.answer, false)
        .into_iter()
        .filter(|n| !item_numbers.contains(n) && !allowed_request_numbers.contains(n))
        .collect();
    if !invented_nums.is_empty() {
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::SpeculationInNotFound,
            format!(
                "NOT_FOUND row introduced ungrounded numeric/date token(s): {}",
                invented_nums.join(", ")
            ),
        );
    }

    RowOutcome {
        grounded: None,
        partial: None,
        not_found: Some(VerifiedItem {
            item: row.item.clone(),
            status: ClaimStatus::NotFound,
            answer: row.answer.clone(),
            cited_facts: Vec::new(),
            support_score: 1.0,
        }),
        unsupported: None,
    }
}

fn resolve_cited_fact_text(
    row: &ParsedRow,
    citations: &[String],
    clean_facts: &BTreeMap<String, String>,
    quarantined_facts: &BTreeMap<String, String>,
) -> Result<String, Box<RowOutcome>> {
    if citations.is_empty() {
        return Err(Box::new(RowOutcome::fail(
            &row.item,
            &row.answer,
            Vec::new(),
            ClaimIssueKind::MissingCitation,
            "Cited claim row must cite at least one [F#] fact ID".to_string(),
        )));
    }

    let mut combined = String::new();
    for cid in citations {
        if let Some(ftext) = clean_facts.get(cid) {
            if !combined.is_empty() {
                combined.push(' ');
            }
            combined.push_str(ftext);
        } else if quarantined_facts.contains_key(cid) {
            return Err(Box::new(RowOutcome::fail(
                &row.item,
                &row.answer,
                citations.to_vec(),
                ClaimIssueKind::PromptInjectionDetected,
                format!("Cited fact [{cid}] was quarantined due to prompt-injection content"),
            )));
        } else {
            return Err(Box::new(RowOutcome::fail(
                &row.item,
                &row.answer,
                citations.to_vec(),
                ClaimIssueKind::NonExistentFactId,
                format!("Cited fact [{cid}] does not exist in supplied facts"),
            )));
        }
    }

    let clean_nums = extract_numeric_tokens(&combined, true);
    let ans_nums = extract_numeric_tokens(&row.answer, false);
    for cid in citations {
        if let Some(qtext) = quarantined_facts.get(cid) {
            let q_nums = extract_numeric_tokens(qtext, false);
            if ans_nums
                .iter()
                .any(|n| q_nums.contains(n) && !clean_nums.contains(n))
            {
                return Err(Box::new(RowOutcome::fail(
                    &row.item,
                    &row.answer,
                    citations.to_vec(),
                    ClaimIssueKind::PromptInjectionDetected,
                    format!(
                        "Answer used numeric value from quarantined prompt-injection clause in [{cid}]"
                    ),
                )));
            }
        }
    }

    Ok(combined)
}

fn score_row_lexical_support(
    answer_stems: &[String],
    item_stems: &[String],
    fact_stems: &[String],
    has_answer_numbers: bool,
) -> f32 {
    if answer_stems.len() >= 2 {
        let ans_support = compute_lexical_support(answer_stems, fact_stems);
        let item_support = compute_lexical_support(item_stems, fact_stems);
        ans_support.max(f32::midpoint(ans_support, item_support))
    } else if !answer_stems.is_empty() {
        compute_lexical_support(answer_stems, fact_stems)
    } else if has_answer_numbers {
        if item_stems.is_empty() {
            1.0
        } else {
            compute_lexical_support(item_stems, fact_stems)
        }
    } else {
        0.0
    }
}

fn classify_cited_row(
    row: &ParsedRow,
    status: ClaimStatus,
    citations: Vec<String>,
    ctx: &FactVerificationCtx<'_>,
) -> RowOutcome {
    let combined_fact_text =
        match resolve_cited_fact_text(row, &citations, ctx.clean_facts, ctx.quarantined_facts) {
            Ok(text) => text,
            Err(fail_outcome) => return *fail_outcome,
        };

    let cited_numbers = extract_numeric_tokens(&combined_fact_text, true);
    let answer_numbers = extract_numeric_tokens(&row.answer, false);
    let mut missing_numbers: Vec<String> = answer_numbers
        .iter()
        .filter(|num| {
            !cited_numbers.contains(*num)
                && !(status == ClaimStatus::PartialMatch
                    && (ctx.item_numbers.contains(*num)
                        || ctx.allowed_request_numbers.contains(*num)))
        })
        .cloned()
        .collect();

    if !missing_numbers.is_empty() {
        missing_numbers.sort();
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::UnverifiedNumberOrDate,
            format!(
                "Numeric/date token(s) not verbatim in cited facts: {}",
                missing_numbers.join(", ")
            ),
        );
    }

    let fact_stems = extract_content_stems(&combined_fact_text);
    let answer_stems = extract_content_stems(&row.answer);
    let item_stems = extract_content_stems(&row.item);
    let support_score = score_row_lexical_support(
        &answer_stems,
        &item_stems,
        &fact_stems,
        !answer_numbers.is_empty(),
    );

    if status == ClaimStatus::Found {
        if support_score < FOUND_MIN_LEXICAL_SUPPORT {
            return RowOutcome::fail(
                &row.item,
                &row.answer,
                citations,
                ClaimIssueKind::InsufficientLexicalSupport,
                format!(
                    "Lexical support {support_score:.2} below threshold {FOUND_MIN_LEXICAL_SUPPORT:.2} against cited facts"
                ),
            );
        }
        return RowOutcome {
            grounded: Some(VerifiedItem {
                item: row.item.clone(),
                status: ClaimStatus::Found,
                answer: row.answer.clone(),
                cited_facts: citations,
                support_score,
            }),
            partial: None,
            not_found: None,
            unsupported: None,
        };
    }

    let combined_query_stems: Vec<String> = answer_stems.into_iter().chain(item_stems).collect();
    let partial_support = if combined_query_stems.is_empty() {
        support_score
    } else {
        support_score.max(compute_lexical_support(&combined_query_stems, &fact_stems))
    };

    if partial_support <= 0.0 && answer_numbers.is_empty() {
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::InsufficientLexicalSupport,
            "PARTIAL_MATCH row has zero lexical or numeric overlap with cited facts".to_string(),
        );
    }
    if !indicates_partial_gap(&row.answer) {
        return RowOutcome::fail(
            &row.item,
            &row.answer,
            citations,
            ClaimIssueKind::MissingGapExplanation,
            "PARTIAL_MATCH row must state what matched and what is missing".to_string(),
        );
    }

    RowOutcome {
        grounded: None,
        partial: Some(VerifiedItem {
            item: row.item.clone(),
            status: ClaimStatus::PartialMatch,
            answer: row.answer.clone(),
            cited_facts: citations,
            support_score: partial_support,
        }),
        not_found: None,
        unsupported: None,
    }
}

fn verify_freeform_lines(
    lines: &[String],
    clean_facts: &BTreeMap<String, String>,
    quarantined_facts: &BTreeMap<String, String>,
    allowed_request_numbers: &HashSet<String>,
    unsupported_claims: &mut Vec<UnsupportedClaim>,
) {
    let all_clean_text: String = clean_facts.values().cloned().collect::<Vec<_>>().join(" ");
    let all_clean_nums = extract_numeric_tokens(&all_clean_text, true);
    let all_clean_stems = extract_content_stems(&all_clean_text);
    let all_quarantined_text: String = quarantined_facts
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let all_quarantined_stems = extract_content_stems(&all_quarantined_text);

    for line in lines {
        for sentence in split_sentences_preserving_decimals(line) {
            let CitationsResult {
                has_non_existent,
                citations,
            } = check_freeform_citations(&sentence, clean_facts);

            if contains_prompt_injection(&sentence)
                || (!all_quarantined_stems.is_empty()
                    && compute_lexical_support(
                        &extract_content_stems(&sentence),
                        &all_quarantined_stems,
                    ) >= 0.5)
            {
                unsupported_claims.push(UnsupportedClaim {
                    item: "<freeform>".to_string(),
                    claim: sentence,
                    cited_facts: citations,
                    issue: ClaimIssueKind::PromptInjectionDetected,
                    detail:
                        "Freeform response violated contract and echoed prompt-injection content"
                            .to_string(),
                });
                continue;
            }

            if has_non_existent {
                unsupported_claims.push(UnsupportedClaim {
                    item: "<freeform>".to_string(),
                    claim: sentence,
                    cited_facts: citations,
                    issue: ClaimIssueKind::NonExistentFactId,
                    detail: "Freeform sentence cited non-existent [F#]".to_string(),
                });
                continue;
            }

            let nums = extract_numeric_tokens(&sentence, false);
            if nums
                .iter()
                .any(|n| !all_clean_nums.contains(n) && !allowed_request_numbers.contains(n))
            {
                unsupported_claims.push(UnsupportedClaim {
                    item: "<freeform>".to_string(),
                    claim: sentence,
                    cited_facts: citations,
                    issue: ClaimIssueKind::UnverifiedNumberOrDate,
                    detail: "Freeform sentence contains ungrounded number or date".to_string(),
                });
                continue;
            }

            let stems = extract_content_stems(&sentence);
            if !stems.is_empty()
                && compute_lexical_support(&stems, &all_clean_stems) < FOUND_MIN_LEXICAL_SUPPORT
            {
                unsupported_claims.push(UnsupportedClaim {
                    item: "<freeform>".to_string(),
                    claim: sentence,
                    cited_facts: citations,
                    issue: ClaimIssueKind::UngroundedFreeformClaim,
                    detail: "LLM omitted <output_contract> table and emitted ungrounded prose"
                        .to_string(),
                });
            }
        }
    }
}

fn audit_trailing_prose(
    lines: &[String],
    clean_facts: &BTreeMap<String, String>,
    quarantined_facts: &BTreeMap<String, String>,
    allowed_request_numbers: &HashSet<String>,
    unsupported_claims: &mut Vec<UnsupportedClaim>,
) {
    if lines.is_empty() {
        return;
    }
    let all_clean_text: String = clean_facts.values().cloned().collect::<Vec<_>>().join(" ");
    let all_clean_nums = extract_numeric_tokens(&all_clean_text, true);
    let all_quarantined_text: String = quarantined_facts
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let all_quarantined_nums = extract_numeric_tokens(&all_quarantined_text, false);

    for line in lines {
        if contains_prompt_injection(line) {
            unsupported_claims.push(UnsupportedClaim {
                item: "<notes>".to_string(),
                claim: line.clone(),
                cited_facts: extract_fact_citations(line),
                issue: ClaimIssueKind::PromptInjectionDetected,
                detail: "Trailing note contains prompt-injection override language".to_string(),
            });
            continue;
        }

        let CitationsResult {
            has_non_existent,
            citations,
        } = check_freeform_citations(line, clean_facts);
        if has_non_existent {
            unsupported_claims.push(UnsupportedClaim {
                item: "<notes>".to_string(),
                claim: line.clone(),
                cited_facts: citations,
                issue: ClaimIssueKind::NonExistentFactId,
                detail: "Trailing note cited a non-existent [F#] fact ID".to_string(),
            });
            continue;
        }

        let line_nums = extract_numeric_tokens(line, false);
        let bad_nums: Vec<String> = line_nums
            .into_iter()
            .filter(|n| !all_clean_nums.contains(n) && !allowed_request_numbers.contains(n))
            .collect();
        if !bad_nums.is_empty() {
            let issue = if bad_nums.iter().any(|n| all_quarantined_nums.contains(n)) {
                ClaimIssueKind::PromptInjectionDetected
            } else {
                ClaimIssueKind::UnverifiedNumberOrDate
            };
            unsupported_claims.push(UnsupportedClaim {
                item: "<notes>".to_string(),
                claim: line.clone(),
                cited_facts: citations,
                issue,
                detail: format!(
                    "Trailing note introduced ungrounded numeric/date token(s): {}",
                    bad_nums.join(", ")
                ),
            });
        }
    }
}

struct CitationsResult {
    has_non_existent: bool,
    citations: Vec<String>,
}

fn check_freeform_citations(text: &str, clean_facts: &BTreeMap<String, String>) -> CitationsResult {
    let citations = extract_fact_citations(text);
    let has_non_existent = citations.iter().any(|cid| !clean_facts.contains_key(cid));
    CitationsResult {
        has_non_existent,
        citations,
    }
}

fn find_missing_requested_items(requested_items: &[String], rows: &[ParsedRow]) -> Vec<String> {
    if requested_items.is_empty() {
        return Vec::new();
    }
    if rows.is_empty() {
        return requested_items.to_vec();
    }

    let mut missing = Vec::new();
    for (idx, req_item) in requested_items.iter().enumerate() {
        let req_stems = extract_content_stems(req_item);
        let expected_idx = (idx + 1).to_string();

        let matched = rows.iter().any(|row| {
            let row_stems = extract_content_stems(&row.item);
            if !req_stems.is_empty()
                && !row_stems.is_empty()
                && (compute_lexical_support(&req_stems, &row_stems) >= 0.4
                    || compute_lexical_support(&row_stems, &req_stems) >= 0.4)
            {
                return true;
            }
            // Fallback: exact normalized substring or matching `#` index when row count aligns.
            let r_low = row.item.trim().to_lowercase();
            let q_low = req_item.trim().to_lowercase();
            if !r_low.is_empty() && (r_low.contains(&q_low) || q_low.contains(&r_low)) {
                return true;
            }
            rows.len() == requested_items.len() && row.index_cell.trim() == expected_idx
        });

        if !matched {
            missing.push(req_item.clone());
        }
    }
    missing
}
