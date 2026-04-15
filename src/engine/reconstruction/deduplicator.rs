use crate::types::WeightedToken;
use std::collections::HashMap;

pub struct Deduplicator;

impl Deduplicator {
    pub fn new() -> Self {
        Self
    }

    pub fn process(&self, input: &str) -> Vec<WeightedToken> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        // Maintain token order
        let mut order = Vec::new();

        let tokens: Vec<&str> = input.split_whitespace().collect();
        
        for &t in &tokens {
            let lower = t.to_lowercase();
            if !counts.contains_key(&lower) {
                order.push(lower.clone());
            }
            *counts.entry(lower).or_insert(0) += 1;
        }

        order.into_iter().map(|text| {
            let count = counts[&text];
            WeightedToken {
                text,
                weight: if count > 1 { count as f32 * 0.5 + 0.5 } else { 1.0 },
            }
        }).collect()
    }
}
