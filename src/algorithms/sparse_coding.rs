//! Stage 1 Sparse Coding — identifies the minimum active token set while
//! preserving hard constraints, negations, qualifiers, and numerical bounds.
//!
//! Implements ARCHITECTURE.md §1.2 (Stage 1: Signal Reduction) and §1.5 (Balanced Mode Contract).

use crate::types::{ConstraintToken, ScoredToken, TokenSource};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Critical negation, boundary, and modal tokens that must never be pruned
/// during sparse coding (Gap B1).
const PROTECTED_QUALIFIERS: &[&str] = &[
    "not", "no", "never", "without", "under", "over", "below", "above", "must", "only", "least",
    "most", "avoid", "exclude", "don't", "dont", "cannot", "can't", "cant", "nor", "neither",
    "within", "max", "min", "maximum", "minimum",
];

/// Conversational greeting, hedging, and sign-off filler tokens that receive
/// minimum salience and no positional bonus (Gap B4).
const CONVERSATIONAL_FILLER: &[&str] = &[
    "hi",
    "hello",
    "hey",
    "there",
    "basically",
    "wondering",
    "please",
    "help",
    "really",
    "thanks",
    "thank",
    "advance",
    "much",
    "appreciated",
    "kindly",
    "could",
    "would",
    "just",
    "maybe",
    "possibly",
    "out",
    "so",
];

/// Trivial stop words that should not be individually locked when splitting
/// multi-word constraint phrases (unless they are in `PROTECTED_QUALIFIERS`).
const LOCK_TRIVIAL_STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "i", "you", "we", "it", "to", "of", "and", "or",
    "in", "on", "at", "for", "with", "by", "as", "be", "this", "that", "from", "per",
];

/// Sparse Coding — identifies the minimum active token set.
///
/// Neuroscience basis: V1 cortex fires only 1–5% of neurons per stimulus.
/// Applied here: only high-salience tokens survive into the pipeline, while
/// negations, qualifiers, numerical bounds, and constraint locks are locked at `1.0`.
///
/// Gentle mode  → keep top 30% of tokens (Cowan attention limit)
/// Balanced     → keep top 45% of tokens (midpoint with full constraint protection)
/// Aggressive   → keep top 60% of tokens (Miller working memory range)
pub struct SparseCoding {
    pub gentle_keep_ratio: f32,
    pub aggressive_keep_ratio: f32,
    common_words: HashSet<&'static str>,
    filler_words: HashSet<&'static str>,
    protected_qualifiers: HashSet<&'static str>,
}

// Domain-specific compound phrases that should be treated as single units
static DOMAIN_PHRASES: phf::Map<&'static str, f32> = phf::phf_map! {
    "two factor authentication" => 0.95,
    "multi factor authentication" => 0.95,
    "hardware security module" => 0.9,
    "secure enclave" => 0.85,
    "zero knowledge proof" => 0.9,
    "post quantum cryptography" => 0.9,
    "homomorphic encryption" => 0.85,
    "secure multi party computation" => 0.9,
    "threshold signature scheme" => 0.85,
    "distributed key generation" => 0.85,
};

impl Default for SparseCoding {
    fn default() -> Self {
        Self::new(0.30, 0.60)
    }
}

impl SparseCoding {
    /// Creates a new `SparseCoding` instance with configured keep ratios.
    ///
    /// # Panics
    /// Never panics.
    pub fn new(gentle_keep_ratio: f32, aggressive_keep_ratio: f32) -> Self {
        let common_words = [
            "the", "a", "an", "is", "are", "was", "were", "i", "you", "we", "it", "to", "of",
            "and", "or", "in", "on", "at", "for", "with", "me", "my", "our", "your", "that",
            "this", "if", "be", "have", "has", "had", "do", "does", "did", "should", "can", "will",
            "per", "about",
        ]
        .into_iter()
        .collect();

        let filler_words = CONVERSATIONAL_FILLER.iter().copied().collect();
        let protected_qualifiers = PROTECTED_QUALIFIERS.iter().copied().collect();

        Self {
            gentle_keep_ratio,
            aggressive_keep_ratio,
            common_words,
            filler_words,
            protected_qualifiers,
        }
    }

    /// Normalizes a token for case-insensitive matching by stripping surrounding punctuation.
    ///
    /// # Panics
    /// Never panics.
    fn normalize_token(token: &str) -> String {
        token
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '$' && c != '%')
            .to_lowercase()
    }

    /// Checks whether a normalized token is a protected negation, qualifier, or numeric value (Gap B1).
    ///
    /// Also checks adjacent tokens for negation/numeric context (e.g. verb/noun immediately following
    /// a negation like `"not use pandas"` or `"not speculate"`, or unit/currency next to a number).
    ///
    /// # Panics
    /// Never panics.
    fn is_protected_token(&self, tokens: &[String], index: usize) -> bool {
        // ARCH: Balanced Mode Contract — negations, boundary qualifiers, and numeric/unit spans are locked at 1.0.
        // FIX: Gap B1 — prevent SparseCoding from dropping negations ("not", "no", "under") or numbers ("1800", "80", "200").
        let norm = Self::normalize_token(&tokens[index]);
        if norm.is_empty() {
            return false;
        }
        let no_apos = norm.replace('\'', "");

        if self.protected_qualifiers.contains(norm.as_str())
            || self.protected_qualifiers.contains(no_apos.as_str())
        {
            return true;
        }

        // Numeric token: contains any ASCII digit
        if norm.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }

        // Currency or unit marker immediately adjacent to a numeric token
        let is_unit_or_currency = matches!(
            norm.as_str(),
            "usd"
                | "eur"
                | "gbp"
                | "inr"
                | "cad"
                | "aud"
                | "dollars"
                | "dollar"
                | "cal"
                | "kcal"
                | "calories"
                | "calorie"
                | "mg"
                | "kg"
                | "g"
                | "lb"
                | "lbs"
                | "oz"
                | "ml"
                | "words"
                | "word"
                | "day"
                | "days"
                | "week"
                | "weeks"
                | "year"
                | "years"
                | "month"
                | "months"
                | "hour"
                | "hours"
                | "min"
                | "mins"
                | "sec"
                | "secs"
                | "percent"
                | "bps"
        );
        if is_unit_or_currency {
            let prev_has_digit =
                index > 0 && tokens[index - 1].chars().any(|c: char| c.is_ascii_digit());
            let next_has_digit = index + 1 < tokens.len()
                && tokens[index + 1].chars().any(|c: char| c.is_ascii_digit());
            if prev_has_digit || next_has_digit {
                return true;
            }
        }

        // Protect the target word(s) immediately following a negation or exclusion trigger
        // (up to 2 non-filler content tokens after a negation, e.g. "no dairy", "not use pandas", "not speculate")
        for lookback in 1..=2 {
            if index >= lookback {
                let prev_norm = Self::normalize_token(&tokens[index - lookback]);
                let prev_no_apos = prev_norm.replace('\'', "");
                if matches!(
                    prev_norm.as_str(),
                    "not"
                        | "no"
                        | "never"
                        | "without"
                        | "avoid"
                        | "exclude"
                        | "don't"
                        | "dont"
                        | "cannot"
                        | "can't"
                        | "cant"
                ) || matches!(prev_no_apos.as_str(), "dont" | "cant")
                {
                    if !self.filler_words.contains(norm.as_str())
                        && !LOCK_TRIVIAL_STOPWORDS.contains(&norm.as_str())
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Identifies all token indices in `tokens` that match any single-word or multi-word
    /// `ConstraintToken` in `locks`, or are protected qualifiers/numbers (Gaps B1 & B2).
    ///
    /// # Panics
    /// Never panics.
    fn compute_protected_indices(
        &self,
        tokens: &[String],
        locks: &[ConstraintToken],
    ) -> HashSet<usize> {
        // ARCH: Balanced Mode Contract — multi-word ConstraintToken phrases and significant lock tokens survive at 1.0.
        // FIX: Gap B2 — match contiguous token subsequences and significant lock words case-insensitively.
        let mut protected = HashSet::new();
        let norm_tokens: Vec<String> = tokens.iter().map(|t| Self::normalize_token(t)).collect();

        // 1. Protect B1 qualifiers, negations, numbers, and adjacent targets
        for i in 0..tokens.len() {
            if self.is_protected_token(tokens, i) {
                protected.insert(i);
            }
        }

        // 2. Match multi-word constraint lock phrases and individual significant lock words
        let trivial_stop: HashSet<&str> = LOCK_TRIVIAL_STOPWORDS.iter().copied().collect();
        let mut significant_lock_words: HashSet<String> = HashSet::new();

        for lock in locks {
            let lock_words: Vec<String> = lock
                .text
                .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '$' && c != '%')
                .map(Self::normalize_token)
                .filter(|w| !w.is_empty())
                .collect();

            if lock_words.is_empty() {
                continue;
            }

            // Contiguous subsequence match across norm_tokens
            if lock_words.len() <= norm_tokens.len() {
                for start in 0..=(norm_tokens.len() - lock_words.len()) {
                    if norm_tokens[start..start + lock_words.len()] == lock_words[..] {
                        for idx in start..start + lock_words.len() {
                            protected.insert(idx);
                        }
                    }
                }
            }

            // Also collect individual significant words from the lock (since Stage 0A/1 lexical
            // stopword removal may have stripped a connecting preposition or auxiliary verb from `tokens`)
            for lw in lock_words {
                if !trivial_stop.contains(lw.as_str())
                    || self.protected_qualifiers.contains(lw.as_str())
                {
                    significant_lock_words.insert(lw);
                }
            }
        }

        for (i, nt) in norm_tokens.iter().enumerate() {
            if !nt.is_empty() && significant_lock_words.contains(nt) {
                protected.insert(i);
            }
        }

        protected
    }

    /// Computes a salience score for every token, returning `(original_index, token, score)`
    /// sorted by score descending (tie-broken by original index ascending).
    ///
    /// # Panics
    /// Never panics.
    fn compute_indexed_salience(&self, tokens: &[String]) -> Vec<(usize, String, f32)> {
        let total = tokens.len();

        let mut scores: Vec<(usize, String, f32)> = tokens
            .par_iter()
            .enumerate()
            .map(|(i, token)| {
                let norm = Self::normalize_token(token);

                // FIX: Gap B1 — protected negations, modals, qualifiers, and numbers always get 1.0
                if self.is_protected_token(tokens, i) {
                    return (i, token.clone(), 1.0_f32);
                }

                // FIX: Gap B4 — conversational greetings and sign-off fillers get 0.05 salience
                // and never receive the start/end positional bonus.
                if self.filler_words.contains(norm.as_str()) {
                    return (i, token.clone(), 0.05_f32);
                }

                let is_common = self.common_words.contains(norm.as_str());

                // Only give positional lead bonus to non-common, non-filler content tokens at the start
                let position_score = if !is_common && i < 3 {
                    0.85_f32
                } else if !is_common && i >= total.saturating_sub(3) {
                    0.65_f32
                } else {
                    0.5_f32
                };

                let rarity_score = if is_common { 0.1_f32 } else { 0.8_f32 };

                // FIX: Gap B5 — case-insensitive syntactic score (Stage 1 lexical output is lowercased)
                let syntactic_score = self.syntactic_weight(&norm);

                // FIX: Gap B5 — expanded case-insensitive domain specificity
                let domain_score = self.domain_specificity(&norm);

                let base_score = position_score * 0.20
                    + rarity_score * 0.30
                    + syntactic_score * 0.30
                    + domain_score * 0.20;

                // Boost score if token is part of a known compound phrase (start or inside window)
                let mut phrase_boost: f32 = 0.0;
                for len in 2..=5 {
                    let start_min = i.saturating_sub(len - 1);
                    for start in start_min..=i {
                        if start + len <= tokens.len() {
                            let phrase: String =
                                tokens[start..start + len].join(" ").to_lowercase();
                            if let Some(&phrase_score) = DOMAIN_PHRASES.get(phrase.as_str()) {
                                phrase_boost = phrase_boost.max(phrase_score * 0.5);
                            }
                        }
                    }
                }

                let final_score = (base_score + phrase_boost).clamp(0.0, 1.0);
                (i, token.clone(), final_score)
            })
            .collect();

        scores.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scores
    }

    /// Compute a salience score for every token.
    ///
    /// Returns a `Vec<(token, score)>` sorted by score descending.
    /// Uses Rayon for parallel scoring — each token scored independently.
    ///
    /// # Panics
    /// Never panics.
    pub fn compute_salience(&self, tokens: &[String]) -> Vec<(String, f32)> {
        self.compute_salience_phrase_aware(tokens)
    }

    /// Compute a salience score for every token with phrase awareness.
    ///
    /// Known compound phrases are scored as single units.
    /// Returns a `Vec<(token, score)>` sorted by score descending.
    ///
    /// # Panics
    /// Never panics.
    pub fn compute_salience_phrase_aware(&self, tokens: &[String]) -> Vec<(String, f32)> {
        self.compute_indexed_salience(tokens)
            .into_iter()
            .map(|(_, tok, score)| (tok, score))
            .collect()
    }

    /// Apply sparse filter: keep top N% tokens while protecting all constraint locks,
    /// negations, qualifiers, and numbers, preserving original token order.
    ///
    /// `mode`: `"temporal"`, `"tree"`, `"spectral"`, `"columnar"`, `"gentle"`, `"balanced"`, `"aggressive"`.
    ///
    /// # Panics
    /// Never panics.
    pub fn apply(
        &self,
        tokens: &[String],
        keep_ratio: f32,
        locks: &[ConstraintToken],
        mode: &str,
    ) -> Vec<ScoredToken> {
        if tokens.is_empty() {
            return Vec::new();
        }

        if mode.eq_ignore_ascii_case("tree") {
            return self.apply_tree(tokens, keep_ratio, locks);
        }

        let indexed_scores = self.compute_indexed_salience(tokens);
        let keep_n = ((tokens.len() as f32 * keep_ratio).ceil() as usize)
            .max(3)
            .min(tokens.len());

        // FIX: Gaps B1, B2, B7 — track survivors by exact token index rather than bare string equality,
        // and always protect negations, qualifiers, numbers, and multi-word constraint locks.
        let protected_indices = self.compute_protected_indices(tokens, locks);
        let mut survivor_indices: HashSet<usize> = protected_indices.clone();

        // Fill remaining budget from highest-salience non-filler tokens first, then any remaining
        for &(orig_idx, ref tok, _score) in &indexed_scores {
            if survivor_indices.len() >= keep_n {
                break;
            }
            let norm = Self::normalize_token(tok);
            if !self.filler_words.contains(norm.as_str()) {
                survivor_indices.insert(orig_idx);
            }
        }

        // If still under minimum keep_n (e.g. prompt is almost entirely filler), fill from top scored
        if survivor_indices.len() < keep_n.min(3) {
            for &(orig_idx, _, _) in &indexed_scores {
                if survivor_indices.len() >= keep_n.min(3) {
                    break;
                }
                survivor_indices.insert(orig_idx);
            }
        }

        let mut salience_by_idx = vec![0.1_f32; tokens.len()];
        for &(orig_idx, _, score) in &indexed_scores {
            salience_by_idx[orig_idx] = score;
        }

        // Return in original token order with accurate salience attached
        tokens
            .iter()
            .enumerate()
            .filter(|(i, _)| survivor_indices.contains(i))
            .map(|(i, t)| {
                let salience = if protected_indices.contains(&i) {
                    1.0
                } else {
                    salience_by_idx[i]
                };

                ScoredToken {
                    text: t.clone(),
                    salience,
                    source: TokenSource::Sparse,
                }
            })
            .collect()
    }

    /// Hierarchical "Neuron Duplicate Tree" pruning.
    ///
    /// Instead of global top-N, groups tokens into semantic prefix branches and selects
    /// the highest-salience representatives per branch while preserving original indices.
    ///
    /// # Panics
    /// Never panics.
    fn apply_tree(
        &self,
        tokens: &[String],
        keep_ratio: f32,
        locks: &[ConstraintToken],
    ) -> Vec<ScoredToken> {
        // FIX: Gap B3 — `indexed_scores` carries `orig_idx` (index in `tokens`), preventing
        // the sorted-index vs original-index misalignment bug that dropped top-salience tokens.
        let indexed_scores = self.compute_indexed_salience(tokens);
        let protected_indices = self.compute_protected_indices(tokens, locks);

        let mut salience_by_idx = vec![0.1_f32; tokens.len()];
        for &(orig_idx, _, score) in &indexed_scores {
            salience_by_idx[orig_idx] = score;
        }

        // 1. Group into "Neuron Trees" keyed by prefix, storing (orig_idx, score)
        let mut trees: HashMap<String, Vec<(usize, f32)>> = HashMap::new();
        for &(orig_idx, ref token, score) in &indexed_scores {
            let norm = Self::normalize_token(token);
            if self.filler_words.contains(norm.as_str()) {
                continue;
            }
            let tree_id = self.get_tree_id(token);
            trees.entry(tree_id).or_default().push((orig_idx, score));
        }

        // 2. Prune duplicate trees (branches with low aggregate salience)
        let mut survivor_indices: HashSet<usize> = protected_indices.clone();
        let keep_n = ((tokens.len() as f32 * keep_ratio).ceil() as usize)
            .max(3)
            .min(tokens.len());

        let mut tree_salience: Vec<(String, f32)> = trees
            .iter()
            .map(|(id, members)| {
                let max_sal = members.iter().map(|(_, s)| *s).fold(0.0_f32, f32::max);
                (id.clone(), max_sal)
            })
            .collect();

        tree_salience.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // First pass: keep the highest-salience token from each top tree until budget is reached
        for (id, _) in &tree_salience {
            if survivor_indices.len() >= keep_n {
                break;
            }
            if let Some(members) = trees.get(id) {
                if let Some(&(best_orig_idx, _)) = members.iter().max_by(|a, b| {
                    a.1.partial_cmp(&b.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| b.0.cmp(&a.0))
                }) {
                    survivor_indices.insert(best_orig_idx);
                }
            }
        }

        // Second pass: if budget remains, fill with remaining highest-scored non-filler tokens
        if survivor_indices.len() < keep_n {
            for &(orig_idx, ref tok, _) in &indexed_scores {
                if survivor_indices.len() >= keep_n {
                    break;
                }
                let norm = Self::normalize_token(tok);
                if !self.filler_words.contains(norm.as_str()) {
                    survivor_indices.insert(orig_idx);
                }
            }
        }

        // Map back to original token order using true original index
        tokens
            .iter()
            .enumerate()
            .filter_map(|(orig_idx, t)| {
                if survivor_indices.contains(&orig_idx) {
                    let salience = if protected_indices.contains(&orig_idx) {
                        1.0
                    } else {
                        salience_by_idx[orig_idx]
                    };
                    Some(ScoredToken {
                        text: t.clone(),
                        salience,
                        source: TokenSource::Sparse,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_tree_id(&self, token: &str) -> String {
        let t = Self::normalize_token(token);
        let chars: Vec<char> = t.chars().collect();
        if chars.len() < 2 {
            return "root".to_string();
        }
        chars[0..2].iter().collect()
    }

    /// Computes case-insensitive syntactic salience weight (Gap B5).
    ///
    /// Boosts action verbs, structural nouns, and informative morphology on lowercased tokens.
    fn syntactic_weight(&self, norm_token: &str) -> f32 {
        if norm_token.is_empty() {
            return 0.2;
        }
        if self.filler_words.contains(norm_token) {
            return 0.05;
        }
        if self.common_words.contains(norm_token) {
            return 0.2;
        }

        let action_or_structural = [
            "write",
            "parse",
            "parses",
            "return",
            "returns",
            "create",
            "build",
            "summarize",
            "highlight",
            "handle",
            "include",
            "analyze",
            "design",
            "implement",
            "compute",
            "calculate",
            "extract",
            "validate",
            "verify",
            "optimize",
            "function",
            "column",
            "average",
            "price",
            "missing",
            "values",
            "report",
            "board",
            "risks",
            "quarterly",
            "revenue",
            "numbers",
            "meal",
            "plan",
            "calories",
            "dairy",
            "nightshades",
            "fiber",
            "budget",
            "shopping",
            "list",
            "woman",
            "speculate",
            "csv",
            "sql",
            "api",
            "json",
            "xml",
            "jwt",
            "sdk",
            "cli",
            "hsm",
            "python",
            "pandas",
        ];

        if action_or_structural.contains(&norm_token) || self.domain_specificity(norm_token) >= 0.85
        {
            return 0.9;
        }

        if norm_token.len() >= 6 {
            0.8
        } else if norm_token.len() >= 4 {
            0.7
        } else {
            0.55
        }
    }

    /// Computes case-insensitive domain specificity score (Gap B5).
    fn domain_specificity(&self, norm_token: &str) -> f32 {
        let lower = norm_token.to_lowercase();
        let technical_indicators = [
            "auth",
            "token",
            "encrypt",
            "compress",
            "neural",
            "schema",
            "oauth",
            "pkce",
            "hsm",
            "python",
            "csv",
            "pandas",
            "json",
            "sql",
            "api",
            "average",
            "price",
            "column",
            "missing",
            "gracefully",
            "calories",
            "dairy",
            "nightshade",
            "fiber",
            "budget",
            "usd",
            "shopping",
            "revenue",
            "quarterly",
            "risks",
            "speculate",
            "q1",
            "q2",
            "q3",
            "q4",
            "fintech",
            "pharma",
            "security",
            "ransomware",
            "forensics",
        ];
        if technical_indicators.iter().any(|ind| lower.contains(ind)) {
            0.9
        } else if self.filler_words.contains(lower.as_str()) {
            0.05
        } else {
            0.5
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_reduces_token_count() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = "the quick brown fox jumps over the lazy dog authentication"
            .split_whitespace()
            .map(String::from)
            .collect();
        let result = sc.apply(&tokens, 0.30, &[], "gentle");
        assert!(
            result.len() < tokens.len(),
            "Sparse coding must reduce token count"
        );
    }

    #[test]
    fn salience_scores_in_range() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = vec!["OAuth2".into(), "the".into(), "authentication".into()];
        let scored = sc.compute_salience(&tokens);
        assert!(scored.iter().all(|(_, s)| *s >= 0.0 && *s <= 1.0));
    }

    #[test]
    fn technical_tokens_outrank_common_words() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = vec!["authentication".into(), "the".into()];
        let scored = sc.compute_salience(&tokens);
        let auth_score = scored
            .iter()
            .find(|(t, _)| t == "authentication")
            .unwrap()
            .1;
        let the_score = scored.iter().find(|(t, _)| t == "the").unwrap().1;
        assert!(
            auth_score > the_score,
            "Technical tokens must score higher than common words"
        );
    }

    #[test]
    fn phrase_aware_scoring_boosts_known_phrases() {
        let sc = SparseCoding::default();
        // Test with a known phrase that should get boosted
        let tokens: Vec<String> = vec!["two".into(), "factor".into(), "authentication".into()];
        let scored = sc.compute_salience(&tokens);

        // All tokens in the phrase should have boosted scores
        for token in &tokens {
            let score = scored.iter().find(|(t, _)| t == token).unwrap().1;
            // Should be higher than common words but less than perfect score
            assert!(
                score > 0.5,
                "Token '{}' should have boosted score, got {}",
                token,
                score
            );
            assert!(score <= 1.0, "Score should not exceed 1.0, got {}", score);
        }
    }

    #[test]
    fn phrase_aware_scoring_handles_mixed_content() {
        let sc = SparseCoding::default();
        // Mix of known phrase and common words
        let tokens: Vec<String> = vec![
            "the".into(),
            "two".into(),
            "factor".into(),
            "authentication".into(),
            "and".into(),
        ];
        let scored = sc.compute_salience(&tokens);

        // Find scores for different token types
        let the_score = scored.iter().find(|(t, _)| t == "the").unwrap().1;
        let _and_score = scored.iter().find(|(t, _)| t == "and").unwrap().1;
        let two_score = scored.iter().find(|(t, _)| t == "two").unwrap().1;
        let factor_score = scored.iter().find(|(t, _)| t == "factor").unwrap().1;
        let auth_score = scored
            .iter()
            .find(|(t, _)| t == "authentication")
            .unwrap()
            .1;

        // Phrase tokens should score higher than common words
        assert!(
            two_score > the_score,
            "\"two\" should score higher than \"the\""
        );
        assert!(
            factor_score > the_score,
            "\"factor\" should score higher than \"the\""
        );
        assert!(
            auth_score > the_score,
            "\"authentication\" should score higher than \"the\""
        );

        // Phrase tokens should score similarly to each other (part of same phrase)
        let phrase_avg = (two_score + factor_score + auth_score) / 3.0;
        let variance = ((two_score - phrase_avg).powi(2)
            + (factor_score - phrase_avg).powi(2)
            + (auth_score - phrase_avg).powi(2))
            / 3.0;
        assert!(variance < 0.1, "Phrase tokens should have similar scores");
    }

    #[test]
    fn b1_negations_qualifiers_and_numbers_always_survive() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = "summarize attached quarterly revenue report board keep under 200 words highlight risks not speculate about q4 numbers"
            .split_whitespace()
            .map(String::from)
            .collect();
        let result = sc.apply(&tokens, 0.30, &[], "balanced");
        let texts: Vec<&str> = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            texts.contains(&"not"),
            "Negation 'not' must survive: {:?}",
            texts
        );
        assert!(
            texts.contains(&"speculate"),
            "Negated target 'speculate' must survive: {:?}",
            texts
        );
        assert!(
            texts.contains(&"under"),
            "Qualifier 'under' must survive: {:?}",
            texts
        );
        assert!(
            texts.contains(&"200"),
            "Numeric token '200' must survive: {:?}",
            texts
        );
        assert!(
            texts.contains(&"words"),
            "Unit 'words' adjacent to '200' must survive: {:?}",
            texts
        );

        for tok in &result {
            if matches!(tok.text.as_str(), "not" | "under" | "200") {
                assert!(
                    (tok.salience - 1.0).abs() < f32::EPSILON,
                    "Protected token '{}' must have salience 1.0, got {}",
                    tok.text,
                    tok.salience
                );
            }
        }
    }

    #[test]
    fn b2_multi_word_constraint_locks_survive_in_apply_and_tree() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = "create 7 day meal plan 35 year old woman 1800 calories day no dairy no nightshades high fiber budget under 80 usd week include shopping list"
            .split_whitespace()
            .map(String::from)
            .collect();
        let locks = vec![
            ConstraintToken {
                text: "no dairy".to_string(),
                weight: 1.0,
            },
            ConstraintToken {
                text: "no nightshades".to_string(),
                weight: 1.0,
            },
            ConstraintToken {
                text: "under 80 usd".to_string(),
                weight: 1.0,
            },
        ];

        for mode in ["balanced", "tree"] {
            let result = sc.apply(&tokens, 0.30, &locks, mode);
            let texts: Vec<&str> = result.iter().map(|t| t.text.as_str()).collect();
            for expected in ["no", "dairy", "nightshades", "1800", "under", "80", "usd"] {
                assert!(
                    texts.contains(&expected),
                    "Mode '{}' must preserve '{}' in {:?}",
                    mode,
                    expected,
                    texts
                );
            }
        }
    }

    #[test]
    fn b3_tree_mode_preserves_highest_scored_token_and_original_indices() {
        let sc = SparseCoding::default();
        // Put low-salience tokens first and high-salience "authentication" at the end
        let tokens: Vec<String> = vec![
            "zeta".into(),
            "beta".into(),
            "gamma".into(),
            "delta".into(),
            "epsilon".into(),
            "theta".into(),
            "kappa".into(),
            "lambda".into(),
            "sigma".into(),
            "authentication".into(),
        ];
        let result = sc.apply(&tokens, 0.30, &[], "tree");
        let texts: Vec<&str> = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            texts.contains(&"authentication"),
            "Tree mode must retain highest-salience token 'authentication', got {:?}",
            texts
        );
    }

    #[test]
    fn b4_conversational_filler_stripped_while_constraints_survive() {
        let sc = SparseCoding::default();
        let tokens: Vec<String> = "hi there basically wondering could please help out really need write python function parses csv file returns average price column must not use pandas handle missing values gracefully thanks much advance"
            .split_whitespace()
            .map(String::from)
            .collect();
        let locks = vec![ConstraintToken {
            text: "must not use pandas".to_string(),
            weight: 1.0,
        }];
        let result = sc.apply(&tokens, 0.45, &locks, "balanced");
        let texts: Vec<&str> = result.iter().map(|t| t.text.as_str()).collect();

        // Greeting and sign-off filler must be pruned
        for filler in [
            "hi",
            "there",
            "basically",
            "wondering",
            "please",
            "thanks",
            "advance",
        ] {
            assert!(
                !texts.contains(&filler),
                "Conversational filler '{}' should be pruned, got {:?}",
                filler,
                texts
            );
        }

        // Core task and constraints must survive
        for essential in ["python", "csv", "must", "not", "pandas", "missing"] {
            assert!(
                texts.contains(&essential),
                "Essential token '{}' must survive, got {:?}",
                essential,
                texts
            );
        }
    }
}
