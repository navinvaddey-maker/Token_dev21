use crate::{
    constraints::{ConstraintResult, ConstraintSet},
    types::{Domain, Intent, PromptSchema, StyleHint, TokenActivation},
};

pub trait SchemaExtractor {
    fn extract(&self, prompt: &str) -> PromptSchema;

    /// Primary output — the only value neuro crates ever receive.
    fn activate(&self, prompt: &str, constraints: &ConstraintSet) -> TokenActivation {
        let schema = self.extract(prompt);
        let tokens = tokenize(prompt);
        let result = constraints.validate(&schema, &tokens);
        let blocked = matches!(result, ConstraintResult::Violated(_));
        TokenActivation {
            weight_hint: intent_weight(&schema.intent),
            tokens,
            blocked,
        }
    }
}

pub struct RuleBasedExtractor;

impl SchemaExtractor for RuleBasedExtractor {
    fn extract(&self, prompt: &str) -> PromptSchema {
        let lower = prompt.to_lowercase();
        PromptSchema {
            intent: detect_intent(&lower),
            domains: detect_domains(&lower),
            style: detect_style(&lower),
            token_count: prompt.split_whitespace().count(),
        }
    }
}

fn tokenize(prompt: &str) -> Vec<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "a", "an", "the", "and", "or", "but", "if", "then", "else", "when", "at", "by", "for",
        "from", "in", "into", "of", "off", "on", "onto", "out", "over", "to", "up", "with", "is",
        "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
        "i", "me", "my", "we", "us", "our", "you", "your", "he", "him", "his", "she", "her", "it",
        "its", "they", "them", "their", "this", "that", "these", "those", "which", "who", "whom",
        "whose", "can", "could", "will", "would", "shall", "should", "may", "might", "must", "not",
        "no", "yes", "so", "as", "than", "more", "most", "some", "any", "all", "each", "every",
    ]
    .iter()
    .cloned()
    .collect();

    prompt
        .split_whitespace()
        .map(|t| {
            t.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|t| !t.is_empty() && !stop_words.contains(t.as_str()) && t.len() > 2)
        .collect()
}

fn intent_weight(intent: &Intent) -> f32 {
    match intent {
        Intent::CodeGeneration => 0.9,
        Intent::Debugging => 0.85,
        Intent::Refactoring => 0.75,
        Intent::Explanation => 0.6,
        Intent::Unknown => 0.3,
    }
}

fn detect_intent(p: &str) -> Intent {
    if p.contains("write") || p.contains("generate") || p.contains("create") {
        Intent::CodeGeneration
    } else if p.contains("fix") || p.contains("bug") || p.contains("error") {
        Intent::Debugging
    } else if p.contains("refactor") || p.contains("improve") {
        Intent::Refactoring
    } else if p.contains("explain") || p.contains("how") {
        Intent::Explanation
    } else {
        Intent::Unknown
    }
}

fn detect_domains(p: &str) -> Vec<Domain> {
    [
        ("rust", Domain::Rust),
        ("async", Domain::Async),
        ("sqlx", Domain::Sqlx),
        ("tokio", Domain::Tokio),
        ("axum", Domain::Axum),
        ("postgres", Domain::Postgres),
        ("serde", Domain::Serde),
    ]
    .iter()
    .filter(|(kw, _)| p.contains(kw))
    .map(|(_, d)| d.clone())
    .collect()
}

fn detect_style(p: &str) -> StyleHint {
    if p.contains("document") || p.contains("comment") {
        StyleHint::Documented
    } else if p.contains("concise") || p.contains("brief") {
        StyleHint::Concise
    } else if p.contains("verbose") || p.contains("detailed") {
        StyleHint::Verbose
    } else {
        StyleHint::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_filters_stop_words() {
        let prompt = "The quick brown fox jumps over the lazy dog and a cat.";
        let tokens = tokenize(prompt);
        // "the", "over", "and", "a" should be filtered out.
        // "fox", "dog", "cat" are >= 3 chars.
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"over".to_string()));
        assert!(!tokens.contains(&"and".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(tokens.contains(&"jumps".to_string()));
    }
}
