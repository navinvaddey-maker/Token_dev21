use crate::types::WeightedToken;
use crate::rag::embeddings::{EmbeddingEngine, cosine_similarity};
use std::collections::HashMap;

pub struct ClusterWeights {
    pub role_weight: f32,
    pub subject_weight: f32,
    pub task_weight: f32,
    pub constraints_weight: f32,
    pub output_weight: f32,
}

impl Default for ClusterWeights {
    fn default() -> Self {
        Self {
            role_weight: 1.0,
            subject_weight: 1.0,
            task_weight: 1.0,
            constraints_weight: 1.0,
            output_weight: 1.0,
        }
    }
}

impl ClusterWeights {
    pub fn apply_intent_class_bias(&mut self, intent_class: &str) {
        match intent_class {
            "wealth-building" => {
                self.role_weight = 1.2;
                self.task_weight = 1.5;
                self.constraints_weight = 0.8;
            }
            "compliance" => {
                self.constraints_weight = 1.8;
                self.role_weight = 1.0;
                self.task_weight = 0.9;
            }
            "debugging" => {
                self.task_weight = 1.6;
                self.constraints_weight = 1.3;
                self.role_weight = 0.7;
            }
            "brand-building" => {
                self.role_weight = 1.3;
                self.task_weight = 1.3;
                self.constraints_weight = 0.9;
            }
            _ => {}
        }
    }
}

pub struct ClusterMapper {
    embedder: EmbeddingEngine,
    centroids: HashMap<String, Vec<f32>>,
}

impl ClusterMapper {
    pub fn new() -> Self {
        let embedder = EmbeddingEngine::new();
        let mut centroids = HashMap::new();

        // Define semantic centroids for core slots
        let cluster_definitions = [
            ("Role", "expert professional specialist consultant authority persona"),
            ("Subject", "topic area domain subject entity object focus"),
            ("Task", "create design build develop generate implement action goal"),
            ("Constraints", "limit restriction rule boundary requirement constraint forbidden"),
            ("Output", "format list report table summary result deliverable"),
        ];

        for (name, phrases) in cluster_definitions {
            centroids.insert(name.to_string(), embedder.embed(phrases));
        }

        Self {
            embedder,
            centroids,
        }
    }

    pub fn group_into_clusters(
        &self,
        tokens: &[WeightedToken],
        weights: &ClusterWeights,
    ) -> HashMap<String, Vec<WeightedToken>> {
        let mut clusters: HashMap<String, Vec<WeightedToken>> = HashMap::new();

        for token in tokens {
            let group = self.map_token_to_cluster(&token.text, weights);
            clusters.entry(group).or_default().push(token.clone());
        }

        clusters
    }

    fn map_token_to_cluster(&self, text: &str, weights: &ClusterWeights) -> String {
        let token_vec = self.embedder.embed(text);
        let mut best_cluster = "General".to_string();
        let mut max_sim = 0.3; // Minimum similarity threshold to avoid noise

        for (name, centroid_vec) in &self.centroids {
            let sim = cosine_similarity(&token_vec, centroid_vec);

            let weight = match name.as_str() {
                "Role" => weights.role_weight,
                "Subject" => weights.subject_weight,
                "Task" => weights.task_weight,
                "Constraints" => weights.constraints_weight,
                "Output" => weights.output_weight,
                _ => 1.0,
            };

            let weighted_sim = sim * weight;
            if weighted_sim > max_sim {
                max_sim = weighted_sim;
                best_cluster = name.clone();
            }
        }

        best_cluster
    }
}

