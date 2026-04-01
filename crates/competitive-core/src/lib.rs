pub mod centroid;

use centroid::{Centroid, CentroidSnapshot};
use schema_engine::TokenActivation;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClusterAssignment {
    pub winner_id: usize,
    pub similarity: f32,
    pub drift: f32,
    pub is_new_cluster: bool,
}

pub struct CompetitiveNet {
    pub centroids: Vec<Centroid>,
    pub learning_rate: f32,
    pub novelty_threshold: f32,
    pub max_clusters: usize,
}

impl CompetitiveNet {
    pub fn new(n_clusters: usize, learning_rate: f32, novelty_threshold: f32) -> Self {
        Self {
            centroids: (0..n_clusters).map(Centroid::new).collect(),
            learning_rate,
            novelty_threshold,
            max_clusters: n_clusters * 2,
        }
    }

    /// Accepts the same TokenActivation as HebbianNet — same boundary contract.
    pub fn update(&mut self, activation: &TokenActivation) -> ClusterAssignment {
        if activation.blocked || activation.tokens.is_empty() {
            return self.soft_assign(&activation.tokens);
        }

        let tokens = &activation.tokens;
        let (winner_idx, best) = self.find_winner(tokens);

        let (final_idx, is_new) =
            if best < self.novelty_threshold && self.centroids.len() < self.max_clusters {
                let id = self.centroids.len();
                self.centroids.push(Centroid::new(id));
                (id, true)
            } else {
                (winner_idx, false)
            };

        let lr = self.learning_rate * activation.weight_hint;
        let drift = self.centroids[final_idx].update(tokens, lr);

        ClusterAssignment {
            winner_id: final_idx,
            similarity: best,
            drift,
            is_new_cluster: is_new,
        }
    }

    /// Top-N vocab for a cluster → prepended to next prompt by the optimizer.
    pub fn cluster_vocabulary(&self, cluster_id: usize, top_n: usize) -> Vec<(String, f32)> {
        let Some(c) = self.centroids.get(cluster_id) else {
            return vec![];
        };
        let mut pairs: Vec<_> = c.vector.iter().map(|(k, v)| (k.clone(), *v)).collect();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        pairs.truncate(top_n);
        pairs
    }

    /// Snapshot all centroids for DB persistence.
    pub fn snapshot(&self) -> Vec<CentroidSnapshot> {
        self.centroids
            .iter()
            .map(|c| {
                let mut top: Vec<_> = c.vector.iter().map(|(k, v)| (k.clone(), *v)).collect();
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                top.truncate(30);
                CentroidSnapshot {
                    id: c.id,
                    label: c.label.clone(),
                    top_tokens: top,
                    hits: c.hits,
                }
            })
            .collect()
    }

    fn find_winner(&self, tokens: &[String]) -> (usize, f32) {
        self.centroids
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.similarity(tokens)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap_or((0, 0.0))
    }

    fn soft_assign(&self, tokens: &[String]) -> ClusterAssignment {
        let (winner_idx, best) = self.find_winner(tokens);
        ClusterAssignment {
            winner_id: winner_idx,
            similarity: best,
            drift: 0.0,
            is_new_cluster: false,
        }
    }
}
