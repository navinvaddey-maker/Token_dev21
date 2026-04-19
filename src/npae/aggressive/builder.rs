use super::parser::ParsedPrompt;
use super::config::UnifiedConfig;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRule {
    pub trigger_words: Vec<String>,
    pub inferred_domain: String,
    pub inferred_intent: String,   // "inclusion" | "forbidden"
    pub confidence: f32,           // initially 0.0, set by builder's heuristics or left for verifier
}

pub fn build_candidate_rules(
    parsed: &ParsedPrompt,
    config: &UnifiedConfig,
) -> Vec<CandidateRule> {
    let mut candidates = Vec::new();

    // Single token candidates
    for token in &parsed.tokens {
        if let Some(domain) = infer_domain(token, config) {
            candidates.push(CandidateRule {
                trigger_words: vec![token.clone()],
                inferred_domain: domain,
                inferred_intent: infer_intent(token),
                confidence: 0.0,
            });
        }
    }

    // Bigram candidates: "no gluten", "high carb", etc.
    for (a, b) in &parsed.bigrams {
        if let Some(domain) = infer_domain_pair(a, b, config) {
            candidates.push(CandidateRule {
                trigger_words: vec![a.clone(), b.clone()],
                inferred_domain: domain,
                inferred_intent: infer_intent_pair(a, b),
                confidence: 0.0,
            });
        }
    }

    candidates
}

fn infer_domain(word: &str, config: &UnifiedConfig) -> Option<String> {
    // Check if the word appears in any taxonomy keywords
    for tax in &config.domain_taxonomy {
        if tax.keywords.iter().any(|kw| kw.to_lowercase() == word.to_lowercase()) {
            return Some(tax.domain.clone());
        }
    }
    None
}

fn infer_domain_pair(a: &str, b: &str, config: &UnifiedConfig) -> Option<String> {
    // Check if either word or the pair triggers a domain
    infer_domain(b, config).or_else(|| infer_domain(a, config))
}

fn infer_intent(word: &str) -> String {
    match word {
        "no" | "avoid" | "without" | "exclude" | "free"
            => "forbidden".to_string(),
        _   => "inclusion".to_string(),
    }
}

fn infer_intent_pair(a: &str, b: &str) -> String {
    if matches!(a, "no" | "avoid" | "without" | "exclude") {
        "forbidden".to_string()
    } else if matches!(b, "free") {
        "forbidden".to_string()
    } else {
        "inclusion".to_string()
    }
}
