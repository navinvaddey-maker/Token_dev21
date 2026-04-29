use crate::npae::compression::types::Stage1Out;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn compress(s1: &Stage1Out) -> Result<Vec<f32>, String> {
    let dim = 256;
    let mut centroid = vec![0.0f32; dim];
    
    if s1.clean_tokens.is_empty() {
        return Ok(centroid);
    }

    // Hash-based pseudo embedding
    for token in &s1.clean_tokens {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        let hash = hasher.finish();
        
        let mut rng_val = hash;
        for j in 0..dim {
            rng_val ^= rng_val << 13;
            rng_val ^= rng_val >> 17;
            rng_val ^= rng_val << 5;
            let val = ((rng_val % 2000) as f32 / 1000.0) - 1.0;
            
            // basic tf-idf weight heuristic: longer words get slightly more weight
            let weight = 1.0 + (token.len() as f32 * 0.1);
            centroid[j] += val * weight;
        }
    }

    let n = s1.clean_tokens.len() as f32;
    for x in &mut centroid {
        *x /= n;
    }

    Ok(centroid)
}
