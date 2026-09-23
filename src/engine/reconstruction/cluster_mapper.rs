use crate::types::WeightedToken;
use crate::rag::embeddings::{EmbeddingEngine, cosine_similarity};
use std::collections::HashMap;

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

    pub fn group_into_clusters(&self, tokens: &[WeightedToken]) -> HashMap<String, Vec<WeightedToken>> {
        let mut clusters: HashMap<String, Vec<WeightedToken>> = HashMap::new();

        for token in tokens {
            let group = self.map_token_to_cluster(&token.text);
            clusters.entry(group).or_default().push(token.clone());
        }

        clusters
    }

    fn map_token_to_cluster(&self, text: &str) -> String {
        let token_vec = self.embedder.embed(text);
        let mut best_cluster = "General".to_string();
        let mut max_sim = 0.3; // Minimum similarity threshold to avoid noise

        for (name, centroid_vec) in &self.centroids {
            let sim = cosine_similarity(&token_vec, centroid_vec);
            if sim > max_sim {
                max_sim = sim;
                best_cluster = name.clone();
            }
        }

        best_cluster
    }
}
