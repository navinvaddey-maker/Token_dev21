use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationMetrics {
    pub lexical_overlap: f32,
    pub semantic_similarity: f32,
    pub fact_recall: f32,
}

/// Tokenizes text into a bag of lowercase alphanumeric words, filtering small words.
fn get_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect()
}

/// Calculates lexical overlap: (intersection of keywords) / (total unique keywords in original).
pub fn calculate_lexical_overlap(original: &str, optimized: &str) -> f32 {
    let orig_tokens: HashSet<String> = get_tokens(original).into_iter().collect();
    let opt_tokens: HashSet<String> = get_tokens(optimized).into_iter().collect();

    if orig_tokens.is_empty() {
        return 1.0;
    }

    let intersection = orig_tokens.intersection(&opt_tokens).count();
    intersection as f32 / orig_tokens.len() as f32
}

/// Heuristic semantic similarity: Cosine similarity of word frequency vectors.
pub fn calculate_semantic_similarity(original: &str, optimized: &str) -> f32 {
    let orig_tokens = get_tokens(original);
    let opt_tokens = get_tokens(optimized);

    if orig_tokens.is_empty() || opt_tokens.is_empty() {
        return 0.0;
    }

    let mut v1: HashMap<String, f32> = HashMap::new();
    let mut v2: HashMap<String, f32> = HashMap::new();

    for t in &orig_tokens {
        *v1.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    for t in &opt_tokens {
        *v2.entry(t.clone()).or_insert(0.0) += 1.0;
    }

    let mut dot = 0.0;
    for (k, val1) in &v1 {
        if let Some(val2) = v2.get(k) {
            dot += val1 * val2;
        }
    }

    let n1 = v1.values().map(|v| v * v).sum::<f32>().sqrt();
    let n2 = v2.values().map(|v| v * v).sum::<f32>().sqrt();

    if n1 == 0.0 || n2 == 0.0 {
        return 0.0;
    }
    (dot / (n1 * n2)).clamp(0.0, 1.0)
}

/// Fact recall: percentage of original sentences that have a semantic match in the output.
pub fn calculate_fact_recall(original: &str, optimized: &str) -> f32 {
    let orig_sentences: Vec<&str> = original
        .split(|c| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| s.len() > 10) // Only consider meaningful sentences as "facts"
        .collect();

    let opt_sentences: Vec<&str> = optimized
        .split(|c| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| s.len() > 10)
        .collect();

    if orig_sentences.is_empty() {
        return 1.0;
    }

    let mut recalled = 0usize;
    for orig_s in &orig_sentences {
        let mut best_sim = 0.0_f32;
        for opt_s in &opt_sentences {
            let sim = calculate_semantic_similarity(orig_s, opt_s);
            if sim > best_sim {
                best_sim = sim;
            }
        }
        // Threshold for "recalled" fact: 0.45 (loose heuristic for summary recall)
        if best_sim > 0.45 {
            recalled += 1;
        }
    }

    recalled as f32 / orig_sentences.len() as f32
}

pub fn evaluate(original: &str, optimized: &str) -> EvaluationMetrics {
    EvaluationMetrics {
        lexical_overlap: calculate_lexical_overlap(original, optimized),
        semantic_similarity: calculate_semantic_similarity(original, optimized),
        fact_recall: calculate_fact_recall(original, optimized),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_text() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let metrics = evaluate(text, text);
        assert!((metrics.lexical_overlap - 1.0).abs() < 0.01);
        assert!((metrics.semantic_similarity - 1.0).abs() < 0.01);
        assert!((metrics.fact_recall - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_unrelated_text() {
        let t1 = "The quick brown fox jumps over the lazy dog.";
        let t2 = "Quantum physics is a fundamental theory in physics.";
        let metrics = evaluate(t1, t2);
        assert!(metrics.lexical_overlap < 0.2);
        assert!(metrics.semantic_similarity < 0.2);
        assert!(metrics.fact_recall < 0.2);
    }
}
