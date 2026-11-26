//! Vector Mathematics for Semantic Architecture
//!
//! Provides lightweight vector operations (cosine similarity, normalization)
//! backed by [`crate::utils::math::OryxVectorMath`].

/// Calculates the cosine similarity between two vectors.
/// Returns a value between -1.0 and 1.0, where 1.0 is exactly similar.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    crate::utils::math::OryxVectorMath::compute_cosine_similarity(a, b)
}

/// Normalizes a vector in-place (L2 normalization).
pub fn l2_normalize(vec: &mut [f32]) {
    crate::utils::math::OryxVectorMath::normalize_l2_in_place(vec);
}
