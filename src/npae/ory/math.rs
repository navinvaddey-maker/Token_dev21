//! Vector Mathematics for Semantic Architecture
//! 
//! Provides lightweight vector operations (cosine similarity, normalization)
//! without requiring external crates like `ndarray`.

/// Calculates the cosine similarity between two vectors.
/// Returns a value between -1.0 and 1.0, where 1.0 is exactly similar.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}

/// Normalizes a vector in-place (L2 normalization)
pub fn l2_normalize(vec: &mut [f32]) {
    let mut norm = 0.0;
    for val in vec.iter() {
        norm += val * val;
    }
    
    if norm > 0.0 {
        let sqrt_norm = norm.sqrt();
        for val in vec.iter_mut() {
            *val /= sqrt_norm;
        }
    }
}
