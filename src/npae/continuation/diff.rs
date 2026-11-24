//! Parent prompt matching and delta extraction for multi-turn continuations.
//!
//! // ARCH: ARCHITECTURE.md §6.7 — isolates incremental user deltas from retyped/edited prompts
//! // (Path B: `EditContinuation`), short parent-linked follow-ups (Path A: `DeltaFollowUp`),
//! // initial turns (`Initial`), and explicit/unrelated resets (`TopicReset`).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::npae::grounding::verifier::split_sentences_preserving_decimals;

/// Minimum Jaccard token similarity to treat an edited prompt as Path B (`EditContinuation`).
const EDIT_CONTINUATION_MIN_JACCARD: f32 = 0.45;

/// Minimum normalized leading-token prefix ratio to treat an edited prompt as Path B.
const EDIT_CONTINUATION_MIN_PREFIX_RATIO: f32 = 0.45;

/// Minimum shared leading tokens required alongside `EDIT_CONTINUATION_MIN_PREFIX_RATIO`
/// so short starter overlaps (for example `"Use ..."`) do not mask Path A follow-ups.
const EDIT_CONTINUATION_MIN_PREFIX_TOKENS: usize = 3;

/// Maximum word count for a parent-linked follow-up without explicit continuation cues
/// before an unrelated message is treated as a `TopicReset`.
const DELTA_FOLLOWUP_MAX_WORDS: usize = 40;

/// Explicit reset phrases indicating the user wants to discard prior prompt state.
const RESET_CUES: &[&str] = &[
    "new prompt:",
    "new topic:",
    "start over",
    "forget previous",
    "reset prompt",
    "scratch that",
    "ignore previous prompt",
    "disregard previous prompt",
];

/// Continuation phrases indicating a short follow-up is modifying or extending the parent prompt.
const CONTINUATION_CUES: &[&str] = &[
    "instead of",
    "rather than",
    "replace ",
    "switch from",
    "switch to",
    "change ",
    "remove ",
    "drop ",
    "allow ",
    "phase ",
    "step ",
    "as json",
    "as yaml",
    "to markdown",
    "as markdown",
    "bullet points",
    "as a table",
    "also ",
    "add ",
    "without ",
    "avoid ",
    "exclude ",
    "must include",
    "include ",
    "only ",
    "explain ",
    "why ",
    "cite ",
    "fix ",
    "repair ",
    "update ",
    "use ",
    "prefer ",
    "continue",
    "next ",
];

/// Routing path classification produced by [`ParentMatcher::extract_delta`].
///
/// // ARCH: ARCHITECTURE.md §6.7 — distinguishes cold starts, Path A delta follow-ups,
/// // Path B retyped/edited prompts, and topic resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationPath {
    /// First turn with no parent prompt.
    Initial,
    /// Path A: concise follow-up referencing an explicit parent prompt.
    DeltaFollowUp,
    /// Path B: retyped or edited version of the parent prompt sharing prefix or high overlap.
    EditContinuation,
    /// Explicit reset cue or unrelated long prompt replacing the active topic.
    TopicReset,
}

/// Output of comparing a new prompt turn against its parent prompt.
///
/// # Invariants
/// `similarity` is always clamped to `[0.0, 1.0]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeltaExtraction {
    /// Selected continuation path.
    pub path: ContinuationPath,
    /// Extracted delta text passed to [`crate::npae::continuation::router::ContinuationRouter`].
    pub delta_text: String,
    /// Token-level Jaccard similarity in `[0.0, 1.0]` between parent and new prompt.
    pub similarity: f32,
    /// Sentences present in the new prompt but absent from the parent prompt.
    pub added_clauses: Vec<String>,
    /// Sentences present in the parent prompt but absent from the new prompt.
    pub removed_clauses: Vec<String>,
}

/// Deterministic matcher that isolates user edits and follow-up deltas against a parent prompt.
///
/// # Invariants
/// Stateless and deterministic; executes in O(N + M) token time without LLM or I/O calls.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParentMatcher;

impl ParentMatcher {
    /// Compares `new_raw` against `parent_raw` and isolates the incremental delta.
    ///
    /// # Arguments
    /// * `parent_raw` - Raw prompt text of the prior parent turn (empty on cold start).
    /// * `new_raw` - Raw prompt text submitted on the current turn.
    /// * `has_explicit_parent` - Whether the caller explicitly linked a parent turn/ledger ID.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn extract_delta(
        parent_raw: &str,
        new_raw: &str,
        has_explicit_parent: bool,
    ) -> DeltaExtraction {
        let trimmed_new = new_raw.trim();
        let trimmed_parent = parent_raw.trim();

        if trimmed_parent.is_empty() && !has_explicit_parent {
            return DeltaExtraction {
                path: ContinuationPath::Initial,
                delta_text: trimmed_new.to_string(),
                similarity: 0.0,
                added_clauses: split_sentences_preserving_decimals(trimmed_new),
                removed_clauses: Vec::new(),
            };
        }

        let parent_tokens = normalize_tokens(trimmed_parent);
        let new_tokens = normalize_tokens(trimmed_new);
        let similarity = jaccard_similarity(&parent_tokens, &new_tokens);

        let parent_sentences = split_sentences_preserving_decimals(trimmed_parent);
        let new_sentences = split_sentences_preserving_decimals(trimmed_new);

        if has_reset_cue(trimmed_new) {
            return DeltaExtraction {
                path: ContinuationPath::TopicReset,
                delta_text: trimmed_new.to_string(),
                similarity,
                added_clauses: new_sentences,
                removed_clauses: Vec::new(),
            };
        }

        if trimmed_parent.is_empty() {
            return DeltaExtraction {
                path: if has_explicit_parent {
                    ContinuationPath::DeltaFollowUp
                } else {
                    ContinuationPath::Initial
                },
                delta_text: trimmed_new.to_string(),
                similarity: 0.0,
                added_clauses: new_sentences,
                removed_clauses: Vec::new(),
            };
        }

        let (added_clauses, removed_clauses) = diff_sentences(&parent_sentences, &new_sentences);
        let shared_sentences = new_sentences.len().saturating_sub(added_clauses.len());
        let common_prefix = common_prefix_len(&parent_tokens, &new_tokens);
        let prefix_ratio = if parent_tokens.is_empty() {
            0.0
        } else {
            usize_to_f32(common_prefix) / usize_to_f32(parent_tokens.len())
        };

        // ARCH: ARCHITECTURE.md §6.7 — Path B triggers when the prompt shares a meaningful
        // prefix or high token overlap with the parent, isolating added and removed clauses.
        let is_prefix_edit = prefix_ratio >= EDIT_CONTINUATION_MIN_PREFIX_RATIO
            && (common_prefix >= EDIT_CONTINUATION_MIN_PREFIX_TOKENS || shared_sentences >= 1);
        let is_high_overlap_edit = similarity >= EDIT_CONTINUATION_MIN_JACCARD
            && (new_tokens.len() >= 4 || shared_sentences >= 1);

        if is_prefix_edit || is_high_overlap_edit {
            let delta_text = build_edit_delta_text(&added_clauses, &removed_clauses);
            return DeltaExtraction {
                path: ContinuationPath::EditContinuation,
                delta_text,
                similarity,
                added_clauses,
                removed_clauses,
            };
        }

        // Path A: explicit parent with continuation cues or concise follow-up (<= 40 words).
        let word_count = trimmed_new.split_whitespace().count();
        if has_explicit_parent
            && (has_continuation_cue(trimmed_new) || word_count <= DELTA_FOLLOWUP_MAX_WORDS)
        {
            return DeltaExtraction {
                path: ContinuationPath::DeltaFollowUp,
                delta_text: trimmed_new.to_string(),
                similarity,
                added_clauses: new_sentences,
                removed_clauses: Vec::new(),
            };
        }

        DeltaExtraction {
            path: ContinuationPath::TopicReset,
            delta_text: trimmed_new.to_string(),
            similarity,
            added_clauses: new_sentences,
            removed_clauses: Vec::new(),
        }
    }
}

/// Tokenizes text into normalized lowercase alphanumeric/hyphen tokens.
fn normalize_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|tok| !tok.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Losslessly converts small token counts (`<= u16::MAX`) to `f32` without precision-loss lints.
fn usize_to_f32(n: usize) -> f32 {
    f32::from(u16::try_from(n).unwrap_or(u16::MAX))
}

/// Computes Jaccard similarity `|A ∩ B| / |A ∪ B|` in `[0.0, 1.0]`.
fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let set_a: HashSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: HashSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        (usize_to_f32(intersection) / usize_to_f32(union)).clamp(0.0, 1.0)
    }
}

/// Counts matching leading tokens between `a` and `b`.
fn common_prefix_len(a: &[String], b: &[String]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Computes canonical normalization key for sentence-level comparison.
fn sentence_key(sentence: &str) -> String {
    normalize_tokens(sentence).join(" ")
}

/// Isolates added and removed sentences between parent and new sentence lists.
fn diff_sentences(
    parent_sentences: &[String],
    new_sentences: &[String],
) -> (Vec<String>, Vec<String>) {
    let parent_keys: HashSet<String> = parent_sentences.iter().map(|s| sentence_key(s)).collect();
    let new_keys: HashSet<String> = new_sentences.iter().map(|s| sentence_key(s)).collect();

    let added = new_sentences
        .iter()
        .filter(|s| !parent_keys.contains(&sentence_key(s)))
        .cloned()
        .collect();
    let removed = parent_sentences
        .iter()
        .filter(|s| !new_keys.contains(&sentence_key(s)))
        .cloned()
        .collect();

    (added, removed)
}

/// Builds concise delta text for Path B edits from added and removed clauses.
fn build_edit_delta_text(added_clauses: &[String], removed_clauses: &[String]) -> String {
    if !added_clauses.is_empty() {
        return added_clauses.join(". ");
    }
    if !removed_clauses.is_empty() {
        return format!("remove {}", removed_clauses.join("; "));
    }
    String::new()
}

/// Returns `true` if `text` contains an explicit prompt-reset cue.
fn has_reset_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    RESET_CUES.iter().any(|cue| lower.contains(cue))
}

/// Returns `true` if `text` contains a continuation cue phrase.
fn has_continuation_cue(text: &str) -> bool {
    let padded = format!(" {} ", text.to_lowercase());
    CONTINUATION_CUES
        .iter()
        .any(|cue| padded.contains(cue) || text.to_lowercase().starts_with(cue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_without_parent() {
        let delta = ParentMatcher::extract_delta(
            "",
            "Build a REST API in Rust using MongoDB and JWT auth.",
            false,
        );
        assert_eq!(delta.path, ContinuationPath::Initial);
        assert_eq!(
            delta.delta_text,
            "Build a REST API in Rust using MongoDB and JWT auth."
        );
        assert_eq!(delta.removed_clauses, Vec::<String>::new());
        assert_eq!(delta.added_clauses.len(), 1);
    }

    #[test]
    fn test_path_a_delta_followup() {
        let parent = "Build a REST API in Rust using MongoDB and JWT authentication.";
        let followup = "Use PostgreSQL instead of MongoDB";
        let delta = ParentMatcher::extract_delta(parent, followup, true);

        assert_eq!(delta.path, ContinuationPath::DeltaFollowUp);
        assert_eq!(delta.delta_text, followup);
        assert_eq!(delta.added_clauses, vec![followup.to_string()]);
        assert_eq!(delta.removed_clauses, Vec::<String>::new());
    }

    #[test]
    fn test_path_b_edit_continuation_retyped_prompt() {
        let parent = "Build a REST API in Rust using MongoDB. Include JWT authentication. Deploy with Docker.";
        let edited = "Build a REST API in Rust using PostgreSQL instead of MongoDB. Include JWT authentication. Deploy with Docker.";
        let delta = ParentMatcher::extract_delta(parent, edited, true);

        assert_eq!(delta.path, ContinuationPath::EditContinuation);
        assert!(delta.similarity >= EDIT_CONTINUATION_MIN_JACCARD);
        assert_eq!(
            delta.added_clauses,
            vec!["Build a REST API in Rust using PostgreSQL instead of MongoDB".to_string()]
        );
        assert_eq!(
            delta.removed_clauses,
            vec!["Build a REST API in Rust using MongoDB".to_string()]
        );
        assert!(delta.delta_text.contains("PostgreSQL instead of MongoDB"));
    }

    #[test]
    fn test_topic_reset_on_explicit_cues() {
        let parent = "Build a REST API in Rust using MongoDB.";
        for reset_input in [
            "New prompt: write a children's story about a dragon.",
            "Please start over and plan a trip to Kyoto.",
            "Forget previous instructions and draft a recipe.",
        ] {
            let delta = ParentMatcher::extract_delta(parent, reset_input, true);
            assert_eq!(delta.path, ContinuationPath::TopicReset);
        }
    }

    #[test]
    fn test_topic_reset_on_unrelated_long_prompt() {
        let parent = "Build a REST API in Rust using MongoDB and JWT auth.";
        let unrelated_long = "Create a comprehensive four week marathon training schedule for an intermediate runner preparing for Chicago with Tuesday tempo runs Thursday track intervals Saturday progressive long runs recovery nutrition hydration targets taper mileage reduction race day pacing splits and post race recovery";
        let delta = ParentMatcher::extract_delta(parent, unrelated_long, true);
        assert_eq!(delta.path, ContinuationPath::TopicReset);
    }
}
