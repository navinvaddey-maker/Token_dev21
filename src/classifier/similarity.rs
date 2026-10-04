use crate::rag::types::bytes_to_embedding;

/// Converts a raw BLOB from SQLite into a usable f32 vector.
///
/// Uses safe byte parsing from little-endian bytes.
/// Assumes blob was written by the same EmbeddingEngine (same byte order, same dimension).
pub fn deserialize_vector_blob(blob: &[u8]) -> Vec<f32> {
    bytes_to_embedding(blob)
}

/// Computes cosine similarity between two L2-normalized vectors.
///
/// Because both vectors are already L2-normalized (contract from EmbeddingEngine),
/// cosine similarity equals the dot product — no division required.
/// This makes batch comparison across all archetypes very fast.
pub fn cosine_similarity_normalized(query: &[f32], archetype: &[f32]) -> f32 {
    debug_assert_eq!(
        query.len(),
        archetype.len(),
        "Vector dimension mismatch: query={} archetype={}",
        query.len(),
        archetype.len()
    );
    query.iter().zip(archetype.iter()).map(|(a, b)| a * b).sum()
}

/// Holds one archetype's similarity score alongside its metadata.
/// Sorted descending by score during top-k selection.
#[derive(Debug, Clone)]
pub struct RankedScenario {
    pub score: f32,
    pub domain: String,
    pub task_type: String,
    pub expertise: String,
    pub intent_class: String,
}

/// Selects the top-k most similar archetypes to the query vector.
///
/// `candidates` = all valid (non-empty-blob) rows from scenario_index.
/// Returns results sorted by score descending, truncated to k.
pub fn select_top_k_archetypes(
    query_vec: &[f32],
    candidates: &[(Vec<f32>, String, String, String, String)],
    k: usize,
) -> Vec<RankedScenario> {
    let mut scored: Vec<RankedScenario> = candidates
        .iter()
        .map(
            |(vec, domain, task_type, expertise, intent)| RankedScenario {
                score: cosine_similarity_normalized(query_vec, vec),
                domain: domain.clone(),
                task_type: task_type.clone(),
                expertise: expertise.clone(),
                intent_class: intent.clone(),
            },
        )
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    scored
}

/// Computes a single confidence score from top-k ranked archetypes.
///
/// Formula: confidence = (top_score × 0.70) + (score_gap × 0.30)
///
/// Rationale:
/// - Top score alone rewards high similarity but ignores ambiguity.
/// - Score gap penalizes cases where two archetypes are nearly equally similar
///   (ambiguous queries), reducing false high-confidence signals.
pub fn compute_confidence_from_rankings(ranked: &[RankedScenario]) -> f32 {
    match ranked.len() {
        0 => 0.0,
        1 => ranked[0].score, // single archetype: use raw score, no gap available
        _ => {
            let top_score = ranked[0].score;
            let second_score = ranked[1].score;
            let score_gap = top_score - second_score;
            ((top_score * 0.70) + (score_gap * 0.30)).clamp(0.0, 1.0)
        }
    }
}
