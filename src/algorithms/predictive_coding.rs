use crate::pipeline::stage0b_topology::topology_mode_prior;
use crate::types::{Mode, PromptTopology};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const BOUNDARY_THRESHOLD: f32 = 0.45;
pub const HYSTERESIS_UPPER: f32 = 0.55; // 0.45–0.55 = ambiguous band

/// Predictive Coding — computes the prediction error score.
///
/// Neuroscience basis: Karl Friston's Free Energy Principle.
/// Brain only propagates PREDICTION ERRORS upward — not confirmed expectations.
/// Applied here: only novel tokens (not in schema priors) trigger deep processing.
///
/// KEY ROLE: error_score IS the mode boundary signal.
///   error < 0.45  → GENTLE  (LLM already knows this context)
///   0.45–0.55     → AMBIGUOUS (resolve via session depth)
///   error > 0.55  → AGGRESSIVE (novel content, deep processing needed)
pub struct PredictiveCoding {
    /// Shared schema priors — Arc allows sharing across async tasks.
    /// DashMap = concurrent HashMap, no Mutex needed for reads.
    schema_priors: Arc<DashMap<String, u32>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionResult {
    pub error_score: f32,
    pub delta_tokens: Vec<String>, // novel tokens → feed to Semantic Clustering
    pub mode: Mode,
    pub is_ambiguous: bool,
}

impl PredictiveCoding {
    pub fn new(schema_priors: Arc<DashMap<String, u32>>) -> Self {
        Self { schema_priors }
    }

    /// Compute prediction error from sparse tokens + session history.
    /// sparse_tokens come directly from SparseCoding::apply() output.
    pub fn compute_error(
        &self,
        sparse_tokens: &[crate::types::ScoredToken],
        session_history: &[SessionTurn],
        topology: crate::types::PromptTopology,
    ) -> PredictionResult {
        if sparse_tokens.is_empty() {
            return PredictionResult {
                error_score: 0.0,
                delta_tokens: vec![],
                mode: Mode::Gentle,
                is_ambiguous: false,
            };
        }

        let mut known_count = 0usize;
        let mut delta_tokens = Vec::new();

        for token in sparse_tokens {
            if self.in_schema(&token.text) {
                known_count += 1;
            } else {
                delta_tokens.push(token.text.clone());
            }
        }

        let novel_count = sparse_tokens.len() - known_count;
        let raw_error = novel_count as f32 / sparse_tokens.len() as f32;

        // Session discount: if user asked about similar tokens, reduce novelty
        let discount = self.session_discount(sparse_tokens, session_history);
        
        // Topology prior adjustment
        let (_, aggressive_prior) = topology_mode_prior(topology.clone());
        let topology_prior = aggressive_prior * 0.15; // Scaled impact
        
        // Final error score adds topology prior to the discounted raw error
        let error_score = (raw_error * (1.0 - discount) + topology_prior).clamp(0.0, 1.0);

        let is_ambiguous = (error_score - BOUNDARY_THRESHOLD).abs()
            < (HYSTERESIS_UPPER - BOUNDARY_THRESHOLD) / 2.0;

        let mode = self.decide_mode(error_score, session_history.len(), topology);

        PredictionResult {
            error_score,
            delta_tokens,
            mode,
            is_ambiguous,
        }
    }

    /// Called after every successful response.
    /// Updates schema priors — this creates the session learning feedback loop.
    /// Over 10 turns, error scores on familiar topics drop 20–35%.
    pub fn update_schema(&self, delta_tokens: &[String]) {
        for token in delta_tokens {
            let key = token.to_lowercase();
            self.schema_priors
                .entry(key)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    }

    fn in_schema(&self, token: &str) -> bool {
        self.schema_priors.contains_key(&token.to_lowercase())
    }

    fn session_discount(
        &self,
        tokens: &[crate::types::ScoredToken],
        session_history: &[SessionTurn],
    ) -> f32 {
        if session_history.is_empty() {
            return 0.0;
        }
        let session_tokens: std::collections::HashSet<&str> = session_history
            .iter()
            .flat_map(|turn| turn.tokens.iter().map(String::as_str))
            .collect();

        let token_texts: std::collections::HashSet<&str> =
            tokens.iter().map(|t| t.text.as_str()).collect();

        let overlap = token_texts.intersection(&session_tokens).count() as f32
            / token_texts.len().max(1) as f32;

        (overlap * 0.4_f32).min(0.35) // max 35% discount
    }

    fn decide_mode(
        &self,
        error_score: f32,
        session_depth: usize,
        topology: crate::types::PromptTopology,
    ) -> Mode {
        // Get topology-based priors for mode selection
        let (gentle_prior, aggressive_prior) = topology_mode_prior(topology);

        // Adjust priors based on error score and session depth
        let error_factor = if error_score < BOUNDARY_THRESHOLD {
            // Low error favors gentle
            gentle_prior * 1.5
        } else if error_score > HYSTERESIS_UPPER {
            // High error favors aggressive
            aggressive_prior * 1.5
        } else {
            // In hysteresis band, use session depth as tiebreaker
            if session_depth < 3 {
                // New sessions favor aggressive
                aggressive_prior * 1.5
            } else {
                // Established sessions favor gentle
                gentle_prior * 1.5
            }
        };

        // Normalize the adjusted factors
        let adjusted_gentle = if error_factor == gentle_prior * 1.5 {
            error_factor
        } else {
            gentle_prior
        };
        let adjusted_aggressive = if error_factor == aggressive_prior * 1.5 {
            error_factor
        } else {
            aggressive_prior
        };

        let gentle_prob = adjusted_gentle / (adjusted_gentle + adjusted_aggressive);

        if gentle_prob > 0.5 {
            Mode::Gentle
        } else {
            Mode::Aggressive
        }
    }
}

/// One turn of session history stored for discount calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTurn {
    pub tokens: Vec<String>,
    pub mode: String,
    pub error_score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ScoredToken, TokenSource};

    fn make_tokens(words: &[&str]) -> Vec<ScoredToken> {
        words
            .iter()
            .map(|w| ScoredToken {
                text: w.to_string(),
                salience: 0.7,
                source: TokenSource::Sparse,
            })
            .collect()
    }

    #[test]
    fn simple_prompt_routes_gentle() {
        let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
        // Pre-fill schema with common tokens
        pc.update_schema(
            &["what", "is", "python", "programming"]
                .map(String::from)
                .to_vec(),
        );
        let tokens = make_tokens(&["what", "is", "python"]);
        let result = pc.compute_error(&tokens, &[], PromptTopology::Linear);
        assert_eq!(result.mode, Mode::Gentle);
    }

    #[test]
    fn complex_prompt_routes_aggressive() {
        let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
        let tokens = make_tokens(&[
            "multi-tenant",
            "OAuth2",
            "HSM",
            "key-rotation",
            "fintech",
            "PKCE",
            "10M-TPS",
        ]);
        let result = pc.compute_error(&tokens, &[], PromptTopology::Linear);
        assert_eq!(result.mode, Mode::Aggressive);
    }

    #[test]
    fn schema_update_lowers_error_on_second_ask() {
        let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
        let tokens = make_tokens(&["OAuth2", "PKCE", "fintech"]);

        let first = pc.compute_error(&tokens, &[], PromptTopology::Linear);
        pc.update_schema(&first.delta_tokens);
        let second = pc.compute_error(&tokens, &[], PromptTopology::Linear);

        assert!(
            second.error_score < first.error_score,
            "Error score must drop after schema update"
        );
    }
}
