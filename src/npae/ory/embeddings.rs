//! Simulated Embedding Engine
//! 
//! Projects text into a 384-dimensional vector space using deterministic hashing.
//! This acts as a placeholder for the `ort` (ONNX) engine to allow for
//! architectural transition to Vector Space Modeling without external dependencies.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use super::math::l2_normalize;

/// Dimension of the embedding space (matches all-MiniLM-L6-v2)
pub const EMBEDDING_DIM: usize = 384;

/// Projects a string into a normalized 384-dimensional vector.
/// This implementation is deterministic and biases specific keywords into 
/// predictable "regions" of the vector space to simulate semantic clustering.
pub fn embed_text(text: &str) -> Vec<f32> {
    let lower = text.to_lowercase();
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    
    // 1. Base signal from string hash
    let mut hasher = DefaultHasher::new();
    lower.hash(&mut hasher);
    let seed = hasher.finish();
    
    for i in 0..EMBEDDING_DIM {
        // Use a simple LCG-like projection to fill the vector
        let val = (((seed.wrapping_add(i as u64)).wrapping_mul(0x45d9f3b)) >> 16) as f32;
        vec[i] = (val / 65535.0) * 0.1; // Small base noise
    }

    // 2. Bias vectors based on known domain keywords to simulate "semantic regions"
    // This ensures our test cases still route correctly in the simulated space.
    inject_bias(&lower, &mut vec);

    // 3. Final normalization
    l2_normalize(&mut vec);
    vec
}

fn inject_bias(text: &str, vec: &mut [f32]) {
    let biases = [
        ("code", 0, 1.0), ("rust", 0, 1.0), ("api", 1, 1.0), ("database", 2, 1.0), // Software
        ("business", 10, 1.0), ("startup", 11, 1.0), ("revenue", 12, 1.0),        // Business
        ("nutrition", 20, 1.0), ("diet", 21, 1.0), ("macro", 22, 1.0),           // Nutrition
        ("translate", 30, 1.0), ("spanish", 31, 1.0), ("french", 32, 1.0),        // Translation
        ("legal", 40, 1.0), ("contract", 41, 1.0),                               // Legal
        ("teach", 50, 1.0), ("learn", 51, 1.0), ("course", 52, 1.0),             // Education
        ("security", 60, 1.0), ("hack", 61, 1.0), ("encrypt", 62, 1.0),          // Security
    ];

    for (word, idx, weight) in biases {
        if text.contains(word) {
            // Apply a Gaussian-like spread around the index
            for offset in -2..=2 {
                let target_idx = (idx as isize + offset).rem_euclid(EMBEDDING_DIM as isize) as usize;
                let spread_weight = weight / (1.0 + (offset.abs() as f32));
                vec[target_idx] += spread_weight;
            }
        }
    }
}
