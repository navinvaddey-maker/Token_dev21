//! Continuation intent router and constraint-operation parser.
//!
//! // ARCH: ARCHITECTURE.md §6.7 — classifies extracted prompt deltas into deterministic
//! // continuation kinds (`SlotFill`, `ConstraintPatch`, `PhaseStep`, `EvidenceDrill`,
//! // `ArtifactTransform`, `Repair`, `TopicReset`) and parses structured constraint operations
//! // (`Override`, `Revoke`, `AddInclusion`, `AddExclusion`), phase jumps, and format shifts.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::npae::aggressive::constraints::extract_constraints;
use crate::npae::grounding::verifier::split_sentences_preserving_decimals;

use super::diff::{ContinuationPath, DeltaExtraction};
use super::ledger::{
    strip_directive_or_exclusion_prefix, tokenize_boundary, tokens_contain_subsequence,
    PromptStateLedger,
};

/// Maximum length in characters for [`ContinuationRoute::delta_summary`].
const MAX_SUMMARY_CHARS: usize = 160;

/// Compiled regex for `"replace Y with X"`.
static RE_REPLACE_WITH: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\breplace\s+(.+?)\s+with\s+(.+)$").ok());

/// Compiled regex for `"switch from Y to X"` or `"switch Y to X"`.
static RE_SWITCH_TO: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\bswitch\s+(?:from\s+)?(.+?)\s+to\s+(.+)$").ok());

/// Compiled regex for `"change Y to X"`.
static RE_CHANGE_TO: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\bchange\s+(.+?)\s+to\s+(.+)$").ok());

/// Compiled regex for `"X instead of Y"`.
static RE_INSTEAD_OF: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?)\s+instead\s+of\s+(.+)$").ok());

/// Compiled regex for `"X rather than Y"`.
static RE_RATHER_THAN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?)\s+rather\s+than\s+(.+)$").ok());

/// Compiled regex for `"remove Y"`, `"drop Y"`, `"allow Y"`.
static RE_REVOKE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:also\s+|please\s+)?(?:remove|drop|allow)\s+(?:the\s+|any\s+)?(.+)$").ok()
});

/// Compiled regex for `"must include X"`, `"include X"`, `"require X"`, `"must support X"`.
static RE_INCLUDE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:also\s+|please\s+)?(?:must\s+include|include|require|must\s+support)\s+(.+)$",
    )
    .ok()
});

/// Compiled regex for `"phase N"` / `"step N"`.
static RE_PHASE_STEP: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:phase|step)\s+(\d{1,3})\b").ok());

/// High-level continuation intent category assigned to a follow-up turn.
///
/// // ARCH: ARCHITECTURE.md §6.7 — drives ledger updates and downstream delta rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationKind {
    /// Initial turn with no prior parent state.
    Initial,
    /// Answers an open ambiguity slot or supplies missing context.
    SlotFill,
    /// Adds, overrides, or revokes a prompt constraint or numeric bound.
    ConstraintPatch,
    /// Advances or jumps to a numbered execution phase or step.
    PhaseStep,
    /// Requests explanation, citations, or deeper evidence inspection.
    EvidenceDrill,
    /// Changes only the requested output format (e.g. JSON, YAML, markdown, bullet points).
    ArtifactTransform,
    /// Corrects a failed grounding check or missing/violated constraint.
    Repair,
    /// Discards prior prompt state and starts a new topic.
    TopicReset,
}

/// Deterministic mutation operation applied to [`crate::npae::continuation::ledger::PromptStateLedger`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOp {
    /// Adds a required inclusion or quantitative bound constraint.
    AddInclusion {
        /// Normalized inclusion or bound description.
        text: String,
    },
    /// Adds an exclusion or forbidden topic constraint.
    AddExclusion {
        /// Normalized exclusion description.
        text: String,
    },
    /// Supersedes an existing target (`old_target`) with a replacement (`new_text`).
    Override {
        /// Existing term or constraint target to mark as `Superseded`.
        old_target: String,
        /// Replacement requirement or exclusion text.
        new_text: String,
        /// `true` if `new_text` represents an exclusion rather than an inclusion.
        is_exclusion: bool,
    },
    /// Revokes an existing constraint target (`Status::Revoked`).
    Revoke {
        /// Target term or constraint fragment to revoke.
        target: String,
    },
}

/// Complete routing decision emitted by [`ContinuationRouter::classify`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationRoute {
    /// Highest-priority continuation kind for this turn.
    pub primary_kind: ContinuationKind,
    /// All detected continuation kinds (deduplicated, ordered by precedence).
    pub all_kinds: Vec<ContinuationKind>,
    /// Parsed constraint operations to apply to the ledger.
    pub constraint_ops: Vec<ConstraintOp>,
    /// Target phase number when a `"phase N"` / `"step N"` directive is detected.
    pub target_phase: Option<u32>,
    /// Requested output format override when a format shift is detected.
    pub output_format_override: Option<String>,
    /// Concise single-line summary of the delta for telemetry and response metadata.
    pub delta_summary: String,
}

/// Deterministic classifier that maps a [`DeltaExtraction`] into a [`ContinuationRoute`].
///
/// # Invariants
/// Executes without LLM or I/O calls; `all_kinds` is always non-empty and contains `primary_kind`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContinuationRouter;

impl ContinuationRouter {
    /// Classifies an extracted delta into continuation kinds and constraint operations.
    ///
    /// # Arguments
    /// * `delta` - Output of [`crate::npae::continuation::diff::ParentMatcher::extract_delta`].
    /// * `open_slots` - Currently unresolved ambiguity/gap slot labels in the ledger.
    /// * `has_phases` - Whether the active ledger already has execution phases.
    /// * `is_repair` - Whether the caller flagged this turn as a verification/guard repair.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn classify(
        delta: &DeltaExtraction,
        open_slots: &[String],
        has_phases: bool,
        is_repair: bool,
    ) -> ContinuationRoute {
        if delta.path == ContinuationPath::Initial {
            return ContinuationRoute {
                primary_kind: ContinuationKind::Initial,
                all_kinds: vec![ContinuationKind::Initial],
                constraint_ops: Vec::new(),
                target_phase: None,
                output_format_override: None,
                delta_summary: summarize_text(&delta.delta_text),
            };
        }

        if delta.path == ContinuationPath::TopicReset {
            return ContinuationRoute {
                primary_kind: ContinuationKind::TopicReset,
                all_kinds: vec![ContinuationKind::TopicReset],
                constraint_ops: Vec::new(),
                target_phase: None,
                output_format_override: None,
                delta_summary: summarize_text(&delta.delta_text),
            };
        }

        let mut constraint_ops = parse_constraint_ops(delta);
        dedup_constraint_ops(&mut constraint_ops);

        let target_phase = detect_target_phase(&delta.delta_text);
        let output_format_override = detect_format_override(&delta.delta_text);
        let repair_signal = is_repair || has_repair_cue(&delta.delta_text);
        let evidence_signal = has_evidence_drill_cue(&delta.delta_text);
        let phase_signal =
            target_phase.is_some() || (has_phases && has_next_phase_cue(&delta.delta_text));
        let has_other_directive = !constraint_ops.is_empty()
            || phase_signal
            || output_format_override.is_some()
            || repair_signal;
        let slot_signal = detect_slot_fill(&delta.delta_text, open_slots, has_other_directive);

        let mut all_kinds = Vec::new();
        for (flag, kind) in [
            (repair_signal, ContinuationKind::Repair),
            (
                !constraint_ops.is_empty(),
                ContinuationKind::ConstraintPatch,
            ),
            (phase_signal, ContinuationKind::PhaseStep),
            (
                output_format_override.is_some(),
                ContinuationKind::ArtifactTransform,
            ),
            (slot_signal, ContinuationKind::SlotFill),
            (evidence_signal, ContinuationKind::EvidenceDrill),
        ] {
            if flag && !all_kinds.contains(&kind) {
                all_kinds.push(kind);
            }
        }
        if all_kinds.is_empty() {
            all_kinds.push(fallback_kind(!open_slots.is_empty(), has_phases));
        }

        let primary_kind = all_kinds
            .first()
            .copied()
            .unwrap_or(ContinuationKind::EvidenceDrill);

        let delta_summary = build_delta_summary(&delta.delta_text, &constraint_ops, target_phase);

        ContinuationRoute {
            primary_kind,
            all_kinds,
            constraint_ops,
            target_phase,
            output_format_override,
            delta_summary,
        }
    }
}

/// Parses overrides, revocations, explicit inclusions, and `constraints.rs` bounds/exclusions.
fn parse_constraint_ops(delta: &DeltaExtraction) -> Vec<ConstraintOp> {
    let mut ops = Vec::new();
    let clauses = split_clauses(&delta.delta_text);

    for clause in &clauses {
        if let Some(op) = parse_override_clause(clause) {
            ops.push(op);
            continue;
        }
        if let Some(op) = parse_revoke_clause(clause) {
            ops.push(op);
            continue;
        }
        if let Some(op) = parse_include_clause(clause) {
            ops.push(op);
        }
    }

    // FIX: High #4 — infer Path B edit overrides before `extract_constraints` so `overlaps_existing_op`
    // suppresses conflicting `AddInclusion` / duplicate `AddExclusion` ops.
    if delta.path == ContinuationPath::EditContinuation
        && !ops
            .iter()
            .any(|op| matches!(op, ConstraintOp::Override { .. }))
    {
        ops.extend(infer_edit_overrides(
            &delta.removed_clauses,
            &delta.added_clauses,
        ));
    }

    // Extract canonical/verbatim bounds and exclusions from `constraints.rs`.
    let (extracted_inc, extracted_forb) = extract_constraints(&delta.delta_text, &[], &[]);
    for forb in extracted_forb {
        if !overlaps_existing_op(&ops, &forb) {
            ops.push(ConstraintOp::AddExclusion { text: forb });
        }
    }
    for inc in extracted_inc {
        if !overlaps_existing_op(&ops, &inc) {
            ops.push(ConstraintOp::AddInclusion { text: inc });
        }
    }

    ops
}

/// Splits `sentence` on `;`, `\n`, and `,`, preserving thousands commas inside numbers (`$1,800`).
fn split_segments_preserving_numbers(sentence: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut iter = sentence.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        let should_split = match ch {
            ';' | '\n' => true,
            ',' => {
                let prev_digit = sentence[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_digit());
                let next_digit = iter.peek().is_some_and(|(_, nc)| nc.is_ascii_digit());
                !(prev_digit && next_digit)
            }
            _ => false,
        };
        if should_split {
            out.push(&sentence[start..idx]);
            start = idx + ch.len_utf8();
        }
    }
    if start <= sentence.len() {
        out.push(&sentence[start..]);
    }
    out
}

/// Splits text into trimmed clause fragments while preserving decimal/thousands numbers (`3.12`, `$1,800`).
fn split_clauses(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // FIX: Medium #8 & Low — split sentences with decimal preservation before splitting on `;`, `,`, `\n`.
    for sentence in split_sentences_preserving_decimals(text) {
        for segment in split_segments_preserving_numbers(&sentence) {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }
            for sub in split_conjunction_clauses(trimmed) {
                if !sub.is_empty() {
                    out.push(sub);
                }
            }
        }
    }
    out
}

/// Splits a segment on `" and "` / `" but "` unless inside an override/revoke phrase.
fn split_conjunction_clauses(segment: &str) -> Vec<String> {
    let lower = segment.to_lowercase();
    for sep in [" and ", " but ", " then "] {
        if let Some(idx) = lower.find(sep) {
            let left = segment.get(..idx).unwrap_or("").trim();
            let right = segment.get(idx + sep.len()..).unwrap_or("").trim();
            if !left.is_empty() && !right.is_empty() {
                let mut parts = vec![left.to_string()];
                parts.extend(split_conjunction_clauses(right));
                return parts;
            }
        }
    }
    vec![segment.to_string()]
}

/// Returns `true` when `text` is or contains a leading exclusion directive cue.
fn has_exclusion_cue(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    if lower.is_empty() {
        return false;
    }
    // FIX: Medium #2 — recognize standalone negation triggers (`no`, `without`, `avoid`, etc.).
    if matches!(
        lower.as_str(),
        "no" | "without"
            | "avoid"
            | "exclude"
            | "never"
            | "excluding"
            | "must not"
            | "do not"
            | "don't"
    ) {
        return true;
    }
    let prefixes = [
        "no ",
        "without ",
        "avoid ",
        "exclude ",
        "never ",
        "must not ",
        "do not use ",
        "don't use ",
    ];
    prefixes
        .iter()
        .any(|p| lower.starts_with(p) || lower.contains(&format!(" {p}")))
}

/// Strips trailing prepositional phrases (e.g. `" for persistence"`) from override targets.
fn strip_trailing_prepositional_phrase(target: &str) -> String {
    let lower = target.to_lowercase();
    let mut earliest: Option<usize> = None;
    for prep in [
        " for ", " as ", " in ", " on ", " during ", " across ", " via ",
    ] {
        if let Some(idx) = lower.find(prep) {
            if idx > 0 {
                earliest = Some(earliest.map_or(idx, |cur| cur.min(idx)));
            }
        }
    }
    if let Some(idx) = earliest {
        let head = clean_target(target.get(..idx).unwrap_or(target));
        if !head.is_empty() {
            return head;
        }
    }
    target.to_string()
}

/// Parses an override directive from a single clause.
fn parse_override_clause(clause: &str) -> Option<ConstraintOp> {
    if let Some(re) = &*RE_REPLACE_WITH {
        if let Some(caps) = re.captures(clause) {
            let old_target = clean_target(caps.get(1)?.as_str());
            let new_text = clean_target(caps.get(2)?.as_str());
            if !old_target.is_empty() && !new_text.is_empty() {
                let is_exclusion = has_exclusion_cue(&new_text);
                return Some(ConstraintOp::Override {
                    old_target,
                    new_text,
                    is_exclusion,
                });
            }
        }
    }

    if let Some(re) = &*RE_SWITCH_TO {
        if let Some(caps) = re.captures(clause) {
            let old_target = clean_target(caps.get(1)?.as_str());
            let new_text = clean_target(caps.get(2)?.as_str());
            if !old_target.is_empty() && !new_text.is_empty() {
                let is_exclusion = has_exclusion_cue(&new_text);
                return Some(ConstraintOp::Override {
                    old_target,
                    new_text,
                    is_exclusion,
                });
            }
        }
    }

    if let Some(re) = &*RE_CHANGE_TO {
        if let Some(caps) = re.captures(clause) {
            let old_target = clean_target(caps.get(1)?.as_str());
            let new_text = clean_target(caps.get(2)?.as_str());
            if !old_target.is_empty() && !new_text.is_empty() {
                let is_exclusion = has_exclusion_cue(&new_text);
                return Some(ConstraintOp::Override {
                    old_target,
                    new_text,
                    is_exclusion,
                });
            }
        }
    }

    for re in [&*RE_INSTEAD_OF, &*RE_RATHER_THAN].into_iter().flatten() {
        if let Some(caps) = re.captures(clause) {
            let raw_head = caps.get(1)?.as_str();
            let raw_new = strip_lead_in_verb(raw_head);
            let old_target =
                strip_trailing_prepositional_phrase(&clean_target(caps.get(2)?.as_str()));
            let new_text = clean_target(&raw_new);
            if !old_target.is_empty() && !new_text.is_empty() {
                let is_exclusion = has_exclusion_cue(raw_head) || has_exclusion_cue(&new_text);
                return Some(ConstraintOp::Override {
                    old_target,
                    new_text,
                    is_exclusion,
                });
            }
        }
    }

    None
}

/// Parses a revocation directive (`"remove Y"`, `"drop Y"`, `"allow Y"`).
fn parse_revoke_clause(clause: &str) -> Option<ConstraintOp> {
    let re = (*RE_REVOKE).as_ref()?;
    let caps = re.captures(clause)?;
    let target = clean_target(caps.get(1)?.as_str());
    if target.is_empty() {
        None
    } else {
        Some(ConstraintOp::Revoke { target })
    }
}

/// Parses an explicit inclusion directive (`"include X"`, `"require X"`, `"must support X"`).
fn parse_include_clause(clause: &str) -> Option<ConstraintOp> {
    if detect_format_override(clause).is_some()
        || detect_target_phase(clause).is_some()
        || has_exclusion_cue(clause)
    {
        return None;
    }
    let re = (*RE_INCLUDE).as_ref()?;
    let caps = re.captures(clause)?;
    let text = clean_target(caps.get(1)?.as_str());
    if text.is_empty() {
        None
    } else {
        Some(ConstraintOp::AddInclusion { text })
    }
}

/// Strips the last lead-in verb prefix (e.g. `"Build an API using PostgreSQL"` -> `"PostgreSQL"`),
/// preserving exclusion directives like `"Avoid gluten"` or `"Never use Redis"`.
fn strip_lead_in_verb(raw: &str) -> String {
    let trimmed = raw.trim();
    if has_exclusion_cue(trimmed) {
        return trimmed.to_string();
    }
    let lower = trimmed.to_lowercase();
    let prefixes = [
        "switch to ",
        "go with ",
        "using ",
        "use ",
        "prefer ",
        "choose ",
        "pick ",
        "with ",
    ];

    let mut best_end: Option<usize> = None;
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let end = prefix.len();
            best_end = Some(best_end.map_or(end, |cur| cur.max(end)));
        }
        let spaced = format!(" {prefix}");
        if let Some(pos) = lower.rfind(&spaced) {
            let end = pos + spaced.len();
            best_end = Some(best_end.map_or(end, |cur| cur.max(end)));
        }
    }

    if let Some(end) = best_end {
        if let Some(rest) = trimmed.get(end..) {
            let candidate = rest.trim();
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Cleans trailing punctuation and surrounding quotes from a parsed target string.
fn clean_target(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\''))
        .trim()
        .to_string()
}

/// Returns the exclusion cue immediately preceding `prefix` in `tokens`, if any.
fn preceding_exclusion_cue(tokens: &[&str], prefix: usize) -> Option<&'static str> {
    if prefix == 0 {
        return None;
    }
    let prev = tokens
        .get(prefix - 1)?
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .to_lowercase();
    if prefix >= 2 {
        let prev2 = tokens
            .get(prefix - 2)?
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
            .to_lowercase();
        if prev2 == "must" && prev == "not" {
            return Some("must not");
        }
        if (prev2 == "do" || prev2 == "don't") && prev == "not" {
            return Some("do not");
        }
    }
    match prev.as_str() {
        "no" => Some("no"),
        "without" => Some("without"),
        "avoid" => Some("avoid"),
        "exclude" => Some("exclude"),
        "never" => Some("never"),
        _ => None,
    }
}

/// Returns `true` if `token` is solely a connector or preposition (`with`, `without`, `for`, etc.).
fn is_connector_or_preposition(token: &str) -> bool {
    matches!(
        token.trim().to_lowercase().as_str(),
        "with"
            | "without"
            | "for"
            | "in"
            | "on"
            | "to"
            | "of"
            | "and"
            | "or"
            | "no"
            | "not"
            | "a"
            | "an"
            | "the"
    )
}

/// Infers `Override` operations when a Path B edit swaps a concise span between removed and added clauses.
fn infer_edit_overrides(removed_clauses: &[String], added_clauses: &[String]) -> Vec<ConstraintOp> {
    let mut ops = Vec::new();
    for removed in removed_clauses {
        let rem_tokens: Vec<&str> = removed.split_whitespace().collect();
        for added in added_clauses {
            let add_tokens: Vec<&str> = added.split_whitespace().collect();
            let prefix = rem_tokens
                .iter()
                .zip(add_tokens.iter())
                .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
                .count();
            let suffix = rem_tokens
                .get(prefix..)
                .unwrap_or(&[])
                .iter()
                .rev()
                .zip(add_tokens.get(prefix..).unwrap_or(&[]).iter().rev())
                .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
                .count();

            let shared = prefix + suffix;
            if shared < 2 {
                continue;
            }
            let rem_end = rem_tokens.len().saturating_sub(suffix);
            let add_end = add_tokens.len().saturating_sub(suffix);
            if prefix >= rem_end || prefix >= add_end {
                continue;
            }
            let old_span = rem_tokens.get(prefix..rem_end).unwrap_or(&[]).join(" ");
            let new_span = add_tokens.get(prefix..add_end).unwrap_or(&[]).join(" ");
            let old_clean = clean_target(&old_span);
            let new_clean = clean_target(&new_span);
            // FIX: Medium #2 — never emit a bare connector/preposition override (e.g. `with` -> `without`)
            // so `extract_constraints` produces `AddExclusion("Exclude <noun>")` instead.
            if is_connector_or_preposition(&old_clean) || is_connector_or_preposition(&new_clean) {
                continue;
            }
            if !old_clean.is_empty()
                && !new_clean.is_empty()
                && old_clean.split_whitespace().count() <= 6
                && new_clean.split_whitespace().count() <= 6
            {
                let add_cue = preceding_exclusion_cue(&add_tokens, prefix);
                let is_exclusion = add_cue.is_some() || has_exclusion_cue(&new_clean);
                let new_text = match add_cue {
                    Some(cue) if !has_exclusion_cue(&new_clean) => format!("{cue} {new_clean}"),
                    _ => new_clean,
                };
                ops.push(ConstraintOp::Override {
                    old_target: old_clean,
                    new_text,
                    is_exclusion,
                });
                break;
            }
        }
    }
    ops
}

/// Checks whether a candidate string overlaps an existing `Override`, `AddExclusion`, or `Revoke` target.
fn overlaps_existing_op(ops: &[ConstraintOp], candidate: &str) -> bool {
    let cand_low = candidate.trim().to_lowercase();
    let cand_tokens = tokenize_boundary(&cand_low);
    let cand_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&cand_low));

    ops.iter().any(|op| match op {
        ConstraintOp::AddInclusion { text } | ConstraintOp::AddExclusion { text } => {
            let t_low = text.trim().to_lowercase();
            let t_tokens = tokenize_boundary(&t_low);
            let t_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&t_low));
            tokens_contain_subsequence(&t_tokens, &cand_tokens)
                || tokens_contain_subsequence(&cand_tokens, &t_tokens)
                || (!cand_core_tokens.is_empty() && cand_core_tokens == t_core_tokens)
        }
        ConstraintOp::Override {
            old_target,
            new_text,
            ..
        } => {
            let old_low = old_target.trim().to_lowercase();
            let new_low = new_text.trim().to_lowercase();
            let old_tokens = tokenize_boundary(&old_low);
            let new_tokens = tokenize_boundary(&new_low);
            let old_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&old_low));
            let new_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&new_low));
            tokens_contain_subsequence(&cand_tokens, &old_tokens)
                || tokens_contain_subsequence(&cand_tokens, &new_tokens)
                || tokens_contain_subsequence(&new_tokens, &cand_tokens)
                || (!cand_core_tokens.is_empty()
                    && (cand_core_tokens == old_core_tokens || cand_core_tokens == new_core_tokens))
        }
        ConstraintOp::Revoke { target } => {
            let t_low = target.trim().to_lowercase();
            let t_tokens = tokenize_boundary(&t_low);
            tokens_contain_subsequence(&cand_tokens, &t_tokens)
                || tokens_contain_subsequence(&t_tokens, &cand_tokens)
        }
    })
}

/// Returns `true` when `clause` is solely a `"phase N"` / `"next phase"` navigation directive.
fn is_pure_phase_directive(clause: &str) -> bool {
    let trimmed = clause.trim();
    if trimmed.is_empty() {
        return false;
    }
    if has_next_phase_cue(trimmed) {
        let lower = trimmed.to_lowercase();
        let remainder = lower
            .replace("next phase", "")
            .replace("next step", "")
            .replace("continue to phase", "");
        let has_extra = tokenize_boundary(&remainder).into_iter().any(|tok| {
            !matches!(
                tok.as_str(),
                "now" | "please" | "go" | "to" | "the" | "next" | "on" | "proceed" | "with"
            )
        });
        if !has_extra {
            return true;
        }
    }
    if let Some(re) = &*RE_PHASE_STEP {
        if re.is_match(trimmed) {
            let stripped = re.replace_all(trimmed, "");
            let has_extra = tokenize_boundary(&stripped).into_iter().any(|tok| {
                !matches!(
                    tok.as_str(),
                    "now"
                        | "please"
                        | "focus"
                        | "on"
                        | "go"
                        | "to"
                        | "the"
                        | "continue"
                        | "with"
                        | "move"
                        | "proceed"
                        | "start"
                        | "for"
                        | "in"
                )
            });
            return !has_extra;
        }
    }
    false
}

/// Returns `true` when `clause` is already captured by an active constraint in `ledger`.
fn is_captured_ledger_constraint(clause: &str, ledger: &PromptStateLedger) -> bool {
    let clause_low = clause.trim().to_lowercase();
    let clause_tokens = tokenize_boundary(&clause_low);
    let clause_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&clause_low));
    if clause_tokens.is_empty() {
        return true;
    }
    ledger
        .active_inclusions()
        .into_iter()
        .chain(ledger.active_exclusions())
        .any(|c| {
            let c_low = c.text.trim().to_lowercase();
            let c_tokens = tokenize_boundary(&c_low);
            let c_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&c_low));
            c_tokens == clause_tokens
                || (!clause_core_tokens.is_empty() && c_core_tokens == clause_core_tokens)
        })
}

/// Extracts non-constraint, non-phase follow-up instructions from Path A `delta_text` so
/// `EvidenceDrill`, `ArtifactTransform`, `SlotFill`, and additive feature requests are preserved
/// without re-injecting superseded/revoked terms or duplicating already-captured directives.
///
/// // ARCH: ARCHITECTURE.md §6.7 — Path A follow-up instruction retention.
///
/// # Panics
/// Never.
#[must_use]
pub fn extract_non_constraint_delta_instruction(
    delta_text: &str,
    ledger: &PromptStateLedger,
) -> Option<String> {
    let mut kept_segments: Vec<String> = Vec::new();
    for sentence in split_sentences_preserving_decimals(delta_text) {
        for segment in split_segments_preserving_numbers(&sentence) {
            let trimmed_seg = segment.trim();
            if trimmed_seg.is_empty() {
                continue;
            }
            let subs = split_conjunction_clauses(trimmed_seg);
            let mut kept_subs: Vec<String> = Vec::new();
            for sub in &subs {
                let is_handled = parse_override_clause(sub).is_some()
                    || parse_revoke_clause(sub).is_some()
                    || is_pure_phase_directive(sub)
                    || is_captured_ledger_constraint(sub, ledger);
                if !is_handled {
                    kept_subs.push(sub.clone());
                }
            }
            if kept_subs.len() == subs.len() {
                kept_segments.push(trimmed_seg.to_string());
            } else if !kept_subs.is_empty() {
                kept_segments.push(kept_subs.join("; "));
            }
        }
    }
    if kept_segments.is_empty() {
        None
    } else {
        Some(kept_segments.join("; "))
    }
}

/// Removes duplicate constraint operations preserving first occurrence order.
fn dedup_constraint_ops(ops: &mut Vec<ConstraintOp>) {
    let mut deduped: Vec<ConstraintOp> = Vec::with_capacity(ops.len());
    for op in ops.drain(..) {
        if !deduped.contains(&op) {
            deduped.push(op);
        }
    }
    *ops = deduped;
}

/// Extracts `"phase N"` or `"step N"` as `Some(N)`.
fn detect_target_phase(text: &str) -> Option<u32> {
    let re = (*RE_PHASE_STEP).as_ref()?;
    let caps = re.captures(text)?;
    caps.get(1)?.as_str().parse::<u32>().ok()
}

/// Detects requested output format shifts in delta text.
fn detect_format_override(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let mappings: &[(&[&str], &str)] = &[
        (&["as json", "in json", "to json", "json format"], "json"),
        (&["as yaml", "in yaml", "to yaml", "yaml format"], "yaml"),
        (
            &[
                "to markdown",
                "as markdown",
                "in markdown",
                "markdown format",
            ],
            "markdown",
        ),
        (
            &["bullet points", "as bullets", "bulleted list"],
            "bullet_points",
        ),
        (
            &["as a table", "as table", "in a table", "markdown table"],
            "table",
        ),
        (&["as csv", "in csv", "to csv"], "csv"),
    ];
    for (patterns, canonical) in mappings {
        if patterns.iter().any(|pat| lower.contains(pat)) {
            return Some((*canonical).to_string());
        }
    }
    None
}

fn has_repair_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "fix the",
        "you missed",
        "you forgot",
        "contradiction",
        "not grounded",
        "violated constraint",
        "repair",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
}

fn has_evidence_drill_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "why ",
        "explain",
        "how does",
        "how do",
        "cite",
        "citation",
        "evidence",
        "source",
        "proof",
        "elaborate",
        "drill into",
        "details on",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
}

fn has_next_phase_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("next phase")
        || lower.contains("next step")
        || lower.contains("continue to phase")
        || lower.trim() == "next"
}

fn detect_slot_fill(text: &str, open_slots: &[String], has_other_directive: bool) -> bool {
    if open_slots.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();
    let mentions_slot = open_slots.iter().any(|slot| {
        slot.split(|c: char| !c.is_alphanumeric())
            .filter(|tok| tok.len() >= 3)
            .any(|tok| lower.contains(&tok.to_lowercase()))
    });
    if mentions_slot {
        return true;
    }
    let word_count = text.split_whitespace().count();
    !has_other_directive && word_count <= 25
}

fn fallback_kind(has_open_slots: bool, has_phases: bool) -> ContinuationKind {
    if has_open_slots {
        ContinuationKind::SlotFill
    } else if has_phases {
        ContinuationKind::PhaseStep
    } else {
        ContinuationKind::EvidenceDrill
    }
}

fn summarize_text(text: &str) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= MAX_SUMMARY_CHARS {
        return single_line;
    }
    let truncated: String = single_line
        .chars()
        .take(MAX_SUMMARY_CHARS.saturating_sub(3))
        .collect();
    format!("{truncated}...")
}

fn build_delta_summary(
    delta_text: &str,
    ops: &[ConstraintOp],
    target_phase: Option<u32>,
) -> String {
    let trimmed = delta_text.trim();
    if !trimmed.is_empty() {
        return summarize_text(trimmed);
    }
    if let Some(phase) = target_phase {
        return format!("Focus phase {phase}");
    }
    if !ops.is_empty() {
        return format!("Applied {} constraint update(s)", ops.len());
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npae::continuation::diff::ParentMatcher;

    #[test]
    fn test_override_parsing_variants() {
        let cases = [
            ("Use PostgreSQL instead of MongoDB", "MongoDB", "PostgreSQL"),
            ("Replace MongoDB with PostgreSQL", "MongoDB", "PostgreSQL"),
            ("Switch from MongoDB to PostgreSQL", "MongoDB", "PostgreSQL"),
            (
                "Use PostgreSQL rather than MongoDB",
                "MongoDB",
                "PostgreSQL",
            ),
            ("Change MongoDB to PostgreSQL", "MongoDB", "PostgreSQL"),
        ];

        for (input, expected_old, expected_new) in cases {
            let delta = DeltaExtraction {
                path: ContinuationPath::DeltaFollowUp,
                delta_text: input.to_string(),
                similarity: 0.2,
                added_clauses: vec![input.to_string()],
                removed_clauses: Vec::new(),
            };
            let route = ContinuationRouter::classify(&delta, &[], false, false);
            assert_eq!(route.primary_kind, ContinuationKind::ConstraintPatch);
            assert!(
                route.constraint_ops.iter().any(|op| matches!(
                    op,
                    ConstraintOp::Override { old_target, new_text, is_exclusion: false }
                        if old_target == expected_old && new_text == expected_new
                )),
                "Expected override {expected_old} -> {expected_new} for `{input}`, got {:?}",
                route.constraint_ops
            );
        }
    }

    #[test]
    fn test_path_b_silent_edit_mongodb_to_postgresql() {
        let parent = "Build a REST API in Rust using MongoDB. Include JWT authentication.";
        let edited = "Build a REST API in Rust using PostgreSQL. Include JWT authentication.";
        let delta = ParentMatcher::extract_delta(parent, edited, true);
        assert_eq!(delta.path, ContinuationPath::EditContinuation);

        let route = ContinuationRouter::classify(&delta, &[], false, false);
        assert_eq!(route.primary_kind, ContinuationKind::ConstraintPatch);
        assert!(route.constraint_ops.iter().any(|op| matches!(
            op,
            ConstraintOp::Override { old_target, new_text, .. }
                if old_target == "MongoDB" && new_text == "PostgreSQL"
        )));
    }

    #[test]
    fn test_revoke_and_verbatim_constraints() {
        let delta = DeltaExtraction {
            path: ContinuationPath::DeltaFollowUp,
            delta_text: "Remove Redis, allow dairy, avoid gluten, and keep under 1800 cal."
                .to_string(),
            similarity: 0.1,
            added_clauses: Vec::new(),
            removed_clauses: Vec::new(),
        };
        let route = ContinuationRouter::classify(&delta, &[], false, false);

        assert_eq!(route.primary_kind, ContinuationKind::ConstraintPatch);
        assert!(route
            .constraint_ops
            .iter()
            .any(|op| matches!(op, ConstraintOp::Revoke { target } if target == "Redis")));
        assert!(route
            .constraint_ops
            .iter()
            .any(|op| matches!(op, ConstraintOp::Revoke { target } if target == "dairy")));
        assert!(route
            .constraint_ops
            .iter()
            .any(|op| matches!(op, ConstraintOp::AddExclusion { text } if text.to_lowercase().contains("gluten"))));
        assert!(route.constraint_ops.iter().any(
            |op| matches!(op, ConstraintOp::AddInclusion { text } if text.contains("1800 cal"))
        ));
    }

    #[test]
    fn test_phase_step_and_artifact_transform() {
        let phase_delta = DeltaExtraction {
            path: ContinuationPath::DeltaFollowUp,
            delta_text: "Now focus on phase 2 and render as json".to_string(),
            similarity: 0.1,
            added_clauses: Vec::new(),
            removed_clauses: Vec::new(),
        };
        let route = ContinuationRouter::classify(&phase_delta, &[], true, false);
        assert_eq!(route.primary_kind, ContinuationKind::PhaseStep);
        assert_eq!(route.target_phase, Some(2));
        assert_eq!(route.output_format_override.as_deref(), Some("json"));
        assert!(route
            .all_kinds
            .contains(&ContinuationKind::ArtifactTransform));

        let format_only = DeltaExtraction {
            path: ContinuationPath::DeltaFollowUp,
            delta_text: "Convert output to markdown bullet points".to_string(),
            similarity: 0.1,
            added_clauses: Vec::new(),
            removed_clauses: Vec::new(),
        };
        let fmt_route = ContinuationRouter::classify(&format_only, &[], false, false);
        assert_eq!(fmt_route.primary_kind, ContinuationKind::ArtifactTransform);
        assert_eq!(
            fmt_route.output_format_override.as_deref(),
            Some("markdown")
        );
    }

    #[test]
    fn test_slot_fill_and_repair() {
        let slot_delta = DeltaExtraction {
            path: ContinuationPath::DeltaFollowUp,
            delta_text: "Target audience is backend engineers".to_string(),
            similarity: 0.1,
            added_clauses: Vec::new(),
            removed_clauses: Vec::new(),
        };
        let route = ContinuationRouter::classify(
            &slot_delta,
            &["audience".to_string(), "deployment".to_string()],
            false,
            false,
        );
        assert_eq!(route.primary_kind, ContinuationKind::SlotFill);

        let repair_route = ContinuationRouter::classify(&slot_delta, &[], false, true);
        assert_eq!(repair_route.primary_kind, ContinuationKind::Repair);
    }

    #[test]
    fn test_with_to_without_swap_and_thousands_comma_preservation() {
        let parent = "Build a weekly meal plan with dairy and under $1,800.";
        let edited = "Build a weekly meal plan without dairy and under $1,800.";
        let delta = ParentMatcher::extract_delta(parent, edited, true);
        assert_eq!(delta.path, ContinuationPath::EditContinuation);

        let route = ContinuationRouter::classify(&delta, &[], false, false);
        assert!(
            !route.constraint_ops.iter().any(|op| matches!(
                op,
                ConstraintOp::AddInclusion { text } | ConstraintOp::Override { new_text: text, is_exclusion: false, .. }
                    if text.eq_ignore_ascii_case("without")
            )),
            "Must never emit inclusion/override with bare \"without\": {:?}",
            route.constraint_ops
        );
        assert!(
            route.constraint_ops.iter().any(|op| matches!(
                op,
                ConstraintOp::AddExclusion { text } | ConstraintOp::Override { new_text: text, is_exclusion: true, .. }
                    if text.to_lowercase().contains("dairy")
            )),
            "Expected dairy exclusion in {:?}",
            route.constraint_ops
        );

        let num_delta = DeltaExtraction {
            path: ContinuationPath::DeltaFollowUp,
            delta_text: "Keep under $1,800, avoid gluten".to_string(),
            similarity: 0.1,
            added_clauses: Vec::new(),
            removed_clauses: Vec::new(),
        };
        let num_route = ContinuationRouter::classify(&num_delta, &[], false, false);
        assert!(
            num_route.constraint_ops.iter().any(
                |op| matches!(op, ConstraintOp::AddInclusion { text } if text.contains("1,800"))
            ),
            "Expected $1,800 kept intact across comma split: {:?}",
            num_route.constraint_ops
        );
    }
}
