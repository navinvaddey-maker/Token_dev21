use std::collections::HashMap;

use super::types::EMBEDDING_DIM;

/// A lightweight, local embedding engine that produces fixed-dimension vectors
/// from text without requiring any external ML model or API.
///
/// Uses a deterministic hash-based projection (random indexing / feature hashing)
/// combined with TF-IDF-style term weighting. This approach:
/// - Requires zero setup (no model downloads)
/// - Is deterministic (same text → same vector)
/// - Is fast (~microseconds per embedding)
/// - Produces vectors suitable for cosine similarity retrieval
///
/// For production-grade semantic similarity, replace the `embed()` method with
/// a call to `rust-bert` sentence-transformers or `fastembed-rs`.
pub struct EmbeddingEngine {
    dim: usize,
}

impl EmbeddingEngine {
    pub fn new() -> Self {
        Self { dim: EMBEDDING_DIM }
    }

    /// Embed a single text string into a fixed-dimension f32 vector.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return vec![0.0; self.dim];
        }

        // Count term frequencies
        let mut tf: HashMap<&str, f32> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.as_str()).or_default() += 1.0;
        }

        // Normalize TF by total token count
        let total = tokens.len() as f32;
        for v in tf.values_mut() {
            *v /= total;
        }

        // Build embedding via feature hashing (random indexing)
        let mut embedding = vec![0.0f32; self.dim];
        for (token, weight) in &tf {
            let hash = stable_hash(token);
            let idx = (hash as usize) % self.dim;
            // Use sign from second hash to reduce collision bias
            let sign = if stable_hash_secondary(token) % 2 == 0 {
                1.0
            } else {
                -1.0
            };
            embedding[idx] += weight * sign;
        }

        // L2 normalize for cosine similarity compatibility
        l2_normalize(&mut embedding);
        embedding
    }

    /// Embed multiple texts — convenience batch method.
    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// Compute cosine similarity between two L2-normalized vectors.
/// Returns value in [-1.0, 1.0] where 1.0 = identical.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ── Internals ──────────────────────────────────────────────────────────────

/// Simple whitespace + punctuation tokenizer with lowercasing and stop-word removal.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|s| !s.is_empty() && s.len() > 1)
        .filter(|s| !is_stop_word(s))
        .map(|s| s.to_string())
        .collect()
}

/// Deterministic hash function (FNV-1a) for stable cross-session consistency.
fn stable_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Secondary hash for sign determination (different seed).
fn stable_hash_secondary(s: &str) -> u64 {
    let mut hash: u64 = 0x517cc1b727220a95;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x9e3779b97f4a7c15);
    }
    hash
}

/// L2 normalize a vector in-place.
fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Minimal English stop-word set.
fn is_stop_word(s: &str) -> bool {
    matches!(
        s,
        "the" | "is" | "at" | "in" | "on" | "of" | "to" | "a" | "an" | "and" | "or"
        | "it" | "by" | "as" | "be" | "do" | "if" | "so" | "we" | "he" | "up" | "no"
        | "my" | "me" | "am" | "was" | "are" | "has" | "had" | "not" | "but" | "for"
        | "this" | "that" | "with" | "from" | "they" | "been" | "have" | "its"
        | "will" | "would" | "could" | "should" | "their" | "what" | "which"
        | "when" | "where" | "how" | "who" | "whom" | "than" | "then" | "these"
        | "those" | "each" | "every" | "all" | "both" | "few" | "more" | "most"
        | "other" | "some" | "such" | "only" | "own" | "into" | "over" | "after"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_produces_correct_dim() {
        let engine = EmbeddingEngine::new();
        let vec = engine.embed("hello world this is a test");
        assert_eq!(vec.len(), EMBEDDING_DIM);
    }

    #[test]
    fn test_embed_is_deterministic() {
        let engine = EmbeddingEngine::new();
        let v1 = engine.embed("medical diagnosis for patient");
        let v2 = engine.embed("medical diagnosis for patient");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_similar_texts_have_higher_similarity() {
        let engine = EmbeddingEngine::new();
        let v1 = engine.embed("machine learning deep neural network");
        let v2 = engine.embed("deep learning neural network model");
        let v3 = engine.embed("chocolate cake baking recipe ingredients");
        let sim_related = cosine_similarity(&v1, &v2);
        let sim_unrelated = cosine_similarity(&v1, &v3);
        assert!(sim_related > sim_unrelated);
    }

    #[test]
    fn test_empty_text_produces_zero_vector() {
        let engine = EmbeddingEngine::new();
        let vec = engine.embed("");
        assert!(vec.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn test_cosine_similarity_self() {
        let engine = EmbeddingEngine::new();
        let v = engine.embed("test sentence for similarity");
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5);
    }
}
