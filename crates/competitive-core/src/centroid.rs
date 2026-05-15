use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Centroid {
    pub id: usize,
    pub label: Option<String>,
    pub vector: HashMap<String, f32>,
    pub hits: u32,
}

impl Centroid {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            label: None,
            vector: HashMap::new(),
            hits: 0,
        }
    }

    /// Cosine similarity between this centroid and an incoming token bag.
    pub fn similarity(&self, tokens: &[String]) -> f32 {
        let dot: f32 = tokens
            .iter()
            .map(|t| self.vector.get(t).copied().unwrap_or(0.0))
            .sum();
        let cn: f32 = self.vector.values().map(|v| v * v).sum::<f32>().sqrt();
        let tn: f32 = (tokens.len() as f32).sqrt();
        if cn == 0.0 || tn == 0.0 {
            return 0.0;
        }
        (dot / (cn * tn)).clamp(0.0, 1.0)
    }

    /// Winner-takes-all update: Δc_i = η · (x - c_i)
    pub fn update(&mut self, tokens: &[String], learning_rate: f32) -> f32 {
        let mut drift = 0.0_f32;
        let len = tokens.len().max(1) as f32;

        let mut input: HashMap<&str, f32> = HashMap::new();
        for t in tokens {
            *input.entry(t.as_str()).or_insert(0.0) += 1.0 / len;
        }

        for (&tok, &xi) in &input {
            let ci = self.vector.entry(tok.to_string()).or_insert(0.0);
            let d = learning_rate * (xi - *ci);
            *ci += d;
            drift += d.abs();
        }
        for (tok, ci) in self.vector.iter_mut() {
            if !input.contains_key(tok.as_str()) {
                let d = learning_rate * (0.0 - *ci);
                *ci += d;
                drift += d.abs();
            }
        }
        self.hits += 1;
        drift
    }

    /// Anti-Hebbian update: move away from the input vector
    pub fn penalize(&mut self, tokens: &[String], learning_rate: f32) -> f32 {
        // We reuse the update logic but with a negative learning rate to "push" away
        self.update(tokens, -learning_rate)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CentroidSnapshot {
    pub id: usize,
    pub label: Option<String>,
    pub top_tokens: Vec<(String, f32)>,
    pub hits: u32,
}
