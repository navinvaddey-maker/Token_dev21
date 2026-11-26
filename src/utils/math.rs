//! Shared L2 vector normalization, cosine/dot similarity, and Reciprocal Rank Fusion (RRF) scoring.
//!
//! // ARCH: ARCHITECTURE.md §3, §6 — Consolidates duplicated vector math and hybrid RRF fusion
//! // across RAG retrieval, local vector search, classifier similarity, and Ory pattern memory.

/// Module-level RRF smoothing constant alias (`k = 60.0`).
pub const RRF_SMOOTHING_K: f32 = OryxVectorMath::RRF_SMOOTHING_K;

/// Stateless vector arithmetic and hybrid ranking utility.
#[derive(Debug, Clone, Copy, Default)]
pub struct OryxVectorMath;

impl OryxVectorMath {
    /// Standard Reciprocal Rank Fusion smoothing constant (`k = 60.0`).
    pub const RRF_SMOOTHING_K: f32 = 60.0;

    /// Normalizes `vector` to unit L2 length in-place when its Euclidean norm exceeds `1e-10`.
    ///
    /// Leaves zero or near-zero vectors unchanged to avoid division-by-zero NaNs.
    ///
    /// # Panics
    /// Never.
    pub fn normalize_l2_in_place(vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for x in vector.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Computes full-norm cosine similarity between `vec_a` and `vec_b`.
    ///
    /// Returns `0.0` if either slice is empty, slice lengths differ, or either vector has zero norm.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn compute_cosine_similarity(vec_a: &[f32], vec_b: &[f32]) -> f32 {
        if vec_a.is_empty() || vec_a.len() != vec_b.len() {
            return 0.0;
        }
        let mut dot = 0.0_f32;
        let mut norm_a = 0.0_f32;
        let mut norm_b = 0.0_f32;
        for (a, b) in vec_a.iter().zip(vec_b.iter()) {
            dot += a * b;
            norm_a += a * a;
            norm_b += b * b;
        }
        let denom = norm_a.sqrt() * norm_b.sqrt();
        if denom == 0.0 {
            0.0
        } else {
            dot / denom
        }
    }

    /// Computes dot product between two pre-normalized unit vectors.
    ///
    /// Returns `0.0` on length mismatch.
    ///
    /// # Invariants
    /// Callers must pass L2-normalized vectors if the result is interpreted as cosine similarity.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn compute_unit_dot_similarity(unit_a: &[f32], unit_b: &[f32]) -> f32 {
        if unit_a.len() != unit_b.len() {
            return 0.0;
        }
        unit_a.iter().zip(unit_b.iter()).map(|(a, b)| a * b).sum()
    }

    /// Computes a normalized Reciprocal Rank Fusion (RRF) score in `[0.0, 1.0]` from optional
    /// 1-based dense and lexical ranks.
    ///
    /// Formula: `((d + l) * (rrf_k * 0.5)).min(1.0)` where `d = 1.0 / (rrf_k + dense_rank)` and
    /// `l = 1.0 / (rrf_k + lexical_rank)`.
    ///
    /// # Arguments
    /// * `dense_rank_1based` - 1-based rank in the dense vector result list (`1` = best).
    /// * `lexical_rank_1based` - 1-based rank in the lexical/BM25 result list (`1` = best).
    /// * `rrf_k` - Smoothing constant (typically [`Self::RRF_SMOOTHING_K`]).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn compute_normalized_rrf_score(
        dense_rank_1based: Option<usize>,
        lexical_rank_1based: Option<usize>,
        rrf_k: f32,
    ) -> f32 {
        // Explicit `allow`: retrieval ranks are small result-list indices well within f32 precision.
        let rrf_term = |rank_opt: Option<usize>| -> f32 {
            rank_opt.map_or(0.0, |rank| {
                let denom = rrf_k + (rank as f32);
                if denom > 0.0 {
                    1.0 / denom
                } else {
                    0.0
                }
            })
        };
        let d = rrf_term(dense_rank_1based);
        let l = rrf_term(lexical_rank_1based);
        ((d + l) * (rrf_k * 0.5)).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_and_dot_and_cosine() {
        let mut v = [3.0_f32, 4.0_f32];
        OryxVectorMath::normalize_l2_in_place(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);

        let unit_dot = OryxVectorMath::compute_unit_dot_similarity(&v, &v);
        assert!((unit_dot - 1.0).abs() < 1e-6);

        let cos = OryxVectorMath::compute_cosine_similarity(&[3.0, 4.0], &[6.0, 8.0]);
        assert!((cos - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_and_dot_edge_cases() {
        assert!((OryxVectorMath::compute_cosine_similarity(&[], &[]) - 0.0).abs() < f32::EPSILON);
        assert!(
            (OryxVectorMath::compute_cosine_similarity(&[1.0], &[1.0, 2.0]) - 0.0).abs()
                < f32::EPSILON
        );
        assert!(
            (OryxVectorMath::compute_cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]) - 0.0).abs()
                < f32::EPSILON
        );
        assert!(
            (OryxVectorMath::compute_unit_dot_similarity(&[1.0], &[1.0, 2.0]) - 0.0).abs()
                < f32::EPSILON
        );

        let mut tiny = [1e-12_f32, 0.0];
        OryxVectorMath::normalize_l2_in_place(&mut tiny);
        assert!((tiny[0] - 1e-12).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_normalized_rrf_score() {
        let both_top = OryxVectorMath::compute_normalized_rrf_score(
            Some(1),
            Some(1),
            OryxVectorMath::RRF_SMOOTHING_K,
        );
        let expected = ((2.0 / 61.0_f32) * 30.0_f32).min(1.0);
        assert!((both_top - expected).abs() < 1e-6);
        assert!(both_top <= 1.0 && both_top > 0.9);

        let dense_only =
            OryxVectorMath::compute_normalized_rrf_score(Some(1), None, RRF_SMOOTHING_K);
        assert!(dense_only < both_top);
        assert!(
            (OryxVectorMath::compute_normalized_rrf_score(None, None, RRF_SMOOTHING_K) - 0.0).abs()
                < f32::EPSILON
        );
    }
}
