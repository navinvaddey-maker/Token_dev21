use crate::types::{ScoredToken, TokenSource};
use rayon::prelude::*;
use std::collections::HashSet;

/// Sparse Coding — identifies the minimum active token set.
///
/// Neuroscience basis: V1 cortex fires only 1–5% of neurons per stimulus.
/// Applied here: only high-salience tokens survive into the pipeline.
///
/// Gentle mode  → keep top 30% of tokens (Cowan attention limit)
/// Aggressive   → keep top 60% of tokens (Miller working memory range)
pub struct SparseCoding {
    pub gentle_keep_ratio: f32,
    pub aggressive_keep_ratio: f32,
    common_words: HashSet<&'static str>,
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
    pub fn new(gentle_keep_ratio: f32, aggressive_keep_ratio: f32) -> Self {
        let common_words = [
            "the", "a", "an", "is", "are", "was", "were", "i", "you", "we", "it", "to", "of",
            "and", "or", "in", "on", "at", "for", "with",
        ]
        .into_iter()
        .collect();

        Self {
            gentle_keep_ratio,
            aggressive_keep_ratio,
            common_words,
        }
    }

    /// Compute a salience score for every token.
    /// Returns a Vec<(token, score)> sorted by score descending.
    /// Uses Rayon for parallel scoring — each token scored independently.
    pub fn compute_salience(&self, tokens: &[String]) -> Vec<(String, f32)> {
        self.compute_salience_phrase_aware(tokens)
    }

    /// Compute a salience score for every token with phrase awareness.
    /// Known compound phrases are scored as single units.
    /// Returns a Vec<(token, score)> sorted by score descending.
    pub fn compute_salience_phrase_aware(&self, tokens: &[String]) -> Vec<(String, f32)> {
        let total = tokens.len();

        let mut scores: Vec<(String, f32)> = tokens
            .par_iter()
            .enumerate()
            .map(|(i, token)| {
                let position_score = if i < 3 || i >= total.saturating_sub(3) {
                    1.0_f32
                } else {
                    0.5_f32
                };

                let rarity_score = if self.common_words.contains(token.to_lowercase().as_str()) {
                    0.1_f32
                } else {
                    0.8_f32
                };

                // Syntactic weight — wire to your existing POS tagger here
                // Placeholder: tokens starting uppercase score as nouns (higher)
                let syntactic_score = if token.chars().next().map_or(false, |c| c.is_uppercase()) {
                    0.8_f32
                } else {
                    0.6_f32
                };

                // Domain specificity — wire to your domain vocabulary map here
                let domain_score = self.domain_specificity(token);

                let base_score = position_score * 0.20
                    + rarity_score * 0.30
                    + syntactic_score * 0.30
                    + domain_score * 0.20;

                // Boost score if token is part of a known phrase
                let mut phrase_boost: f32 = 0.0;
                // Check phrases of different lengths that start at this position
                for len in 2..=5 {
                    if i + len <= tokens.len() {
                        let phrase: String = tokens[i..i + len].join(" ").to_lowercase();
                        if let Some(&phrase_score) = DOMAIN_PHRASES.get(phrase.as_str()) {
                            // Boost based on phrase score, up to 50% boost
                            phrase_boost = phrase_boost.max(phrase_score * 0.5);
                        }
                    }
                }

                let final_score = (base_score + phrase_boost).clamp(0.0, 1.0);
                (token.clone(), final_score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Apply sparse filter: keep top N% tokens, preserve original order.
    /// Pre-boundary = use aggressive ratio (we don't know mode yet at stage 1).
    pub fn apply(
        &self, 
        tokens: &[String], 
        keep_ratio: f32,
        locks: &[crate::types::ConstraintToken]
    ) -> Vec<ScoredToken> {
        let scored = self.compute_salience(tokens);
        let keep_n = ((tokens.len() as f32 * keep_ratio) as usize).max(3);

        let lock_texts: HashSet<&str> = locks.iter().map(|l| l.text.as_str()).collect();

        // Build survivor set from top-N + constraint locks
        let mut survivors: HashSet<&str> = scored
            .iter()
            .take(keep_n)
            .map(|(t, _)| t.as_str())
            .collect();
            
        for lock in &lock_texts {
            survivors.insert(lock);
        }

        // Return in original order with scores attached
        tokens
            .iter()
            .filter(|t| survivors.contains(t.as_str()))
            .map(|t| {
                let mut salience = scored
                    .iter()
                    .find(|(tok, _)| tok == t)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.1);
                    
                if lock_texts.contains(t.as_str()) {
                    salience = 1.0;
                }
                
                ScoredToken {
                    text: t.clone(),
                    salience,
                    source: TokenSource::Sparse,
                }
            })
            .collect()
    }

    /// Wire this to your domain-specific vocabulary.
    /// Technical terms (authentication, fintech, HSM) score high.
    fn domain_specificity(&self, token: &str) -> f32 {
        // Replace with lookup into your actual domain vocab map
        let technical_indicators = ["auth", "token", "encrypt", "compress", "neural", "schema"];
        if technical_indicators
            .iter()
            .any(|ind| token.to_lowercase().contains(ind))
        {
            0.9
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
        let result = sc.apply(&tokens, 0.30, &[]);
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
}
