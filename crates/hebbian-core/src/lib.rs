use schema_engine::TokenActivation;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WeightDelta {
    pub pair: (String, String),
    pub new_weight: f32,
}

pub struct HebbianNet {
    pub weights: HashMap<(String, String), f32>,
    pub learning_rate: f32,
    pub decay: f32,
}

impl HebbianNet {
    pub fn new(learning_rate: f32, decay: f32) -> Self {
        Self {
            weights: Default::default(),
            learning_rate,
            decay,
        }
    }

    /// Core rule: Δw_ij = η · x_i · x_j
    /// Silently skips if activation.blocked == true.
    /// Never inspects why it was blocked — that is schema-engine's concern.
    pub fn update(&mut self, activation: &TokenActivation) {
        if activation.blocked {
            return;
        }

        let eta = self.learning_rate * activation.weight_hint;
        let tokens = &activation.tokens;

        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                let key = ordered_pair(&tokens[i], &tokens[j]);
                let w = self.weights.entry(key).or_insert(0.0);
                *w += eta;
                *w *= 1.0 - self.decay;
                *w = w.clamp(-1.0, 1.0);
            }
        }
    }

    /// Top co-activating pairs — written to token_patterns by learning-engine.
    pub fn top_associations(&self, n: usize) -> Vec<WeightDelta> {
        let mut pairs: Vec<_> = self
            .weights
            .iter()
            .map(|(k, v)| WeightDelta {
                pair: k.clone(),
                new_weight: *v,
            })
            .collect();
        pairs.sort_by(|a, b| b.new_weight.partial_cmp(&a.new_weight).unwrap());
        pairs.truncate(n);
        pairs
    }

    /// Explicit feedback: strengthen or weaken associations.
    pub fn apply_feedback(&mut self, activation: &TokenActivation, value: f32) {
        if activation.blocked {
            return;
        }

        let eta = self.learning_rate * activation.weight_hint * value;
        let tokens = &activation.tokens;

        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                let key = ordered_pair(&tokens[i], &tokens[j]);
                let w = self.weights.entry(key).or_insert(0.0);
                *w += eta;
                *w = w.clamp(-1.0, 1.0);
            }
        }
    }
}

fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}
