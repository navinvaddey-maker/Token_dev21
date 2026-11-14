//! Stage -1 Semantic Cluster Mapper (`ARCHITECTURE.md` §1.2, §1.4).
//!
//! Groups weighted tokens into core semantic slots (`Role`, `Subject`, `Task`,
//! `Constraints`, `Output`) using cosine similarity against slot centroids and
//! intent/domain bias weights from `ScenarioSignal`.

use crate::rag::embeddings::{cosine_similarity, EmbeddingEngine};
use crate::types::WeightedToken;
use std::collections::HashMap;

/// Per-slot weighting multipliers used when assigning tokens to semantic clusters.
#[derive(Debug, Clone)]
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
    /// Applies slot-weight biases based on `intent_class`, canonical task type, or domain.
    ///
    /// # Panics
    /// Never.
    pub fn apply_intent_class_bias(&mut self, intent_class: &str) {
        let lower = intent_class.to_lowercase();
        match lower.as_str() {
            // Goal intents & legacy intent classes
            "wealth-building" | "business" | "business-strategy" | "real-estate" | "finance" => {
                self.role_weight = 1.2;
                self.task_weight = 1.5;
                self.constraints_weight = 0.9;
                self.output_weight = 1.2;
            }
            "compliance" | "legal-query" | "legal" | "cybersecurity" | "medical" | "health"
            | "pharma" | "nutrition" | "sports-nutrition" => {
                self.constraints_weight = 1.8;
                self.role_weight = 1.1;
                self.task_weight = 1.0;
                self.output_weight = 1.1;
            }
            "debugging" | "troubleshooting" => {
                self.task_weight = 1.6;
                self.constraints_weight = 1.3;
                self.role_weight = 0.8;
            }
            "brand-building" | "marketing" | "ecommerce" | "creative" | "creative-writing" => {
                self.role_weight = 1.3;
                self.task_weight = 1.3;
                self.output_weight = 1.2;
                self.constraints_weight = 0.9;
            }
            // Canonical task types & technical/domain classes
            "creation"
            | "software"
            | "software-engineering"
            | "tech"
            | "devops"
            | "devops-infra"
            | "computers"
            | "ai-ml" => {
                self.task_weight = 1.5;
                self.constraints_weight = 1.2;
                self.output_weight = 1.2;
                self.role_weight = 1.0;
            }
            "planning" | "travel" | "education" | "workplace-productivity" | "career-growth" => {
                self.task_weight = 1.4;
                self.output_weight = 1.3;
                self.constraints_weight = 1.2;
                self.role_weight = 1.1;
            }
            "analysis" | "data-science" | "science" | "scientific-research" => {
                self.subject_weight = 1.3;
                self.task_weight = 1.3;
                self.output_weight = 1.2;
                self.constraints_weight = 1.1;
            }
            "optimization" => {
                self.task_weight = 1.5;
                self.constraints_weight = 1.4;
                self.output_weight = 1.1;
            }
            "explanation" => {
                self.subject_weight = 1.4;
                self.task_weight = 1.2;
                self.role_weight = 1.1;
            }
            _ => {}
        }
    }
}

/// Maps weighted tokens into semantic slot clusters using embedding similarity.
#[derive(Debug)]
pub struct ClusterMapper {
    embedder: EmbeddingEngine,
    centroids: HashMap<String, Vec<f32>>,
}

impl ClusterMapper {
    /// Creates a new `ClusterMapper` with pre-computed slot centroids.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        let embedder = EmbeddingEngine::new();
        let mut centroids = HashMap::new();

        // Define semantic centroids for core slots
        let cluster_definitions = [
            (
                "Role",
                "expert professional specialist consultant authority persona",
            ),
            ("Subject", "topic area domain subject entity object focus"),
            (
                "Task",
                "create design build develop generate implement action goal",
            ),
            (
                "Constraints",
                "limit restriction rule boundary requirement constraint forbidden",
            ),
            (
                "Output",
                "format list report table summary result deliverable",
            ),
        ];

        for (name, phrases) in cluster_definitions {
            centroids.insert(name.to_string(), embedder.embed(phrases));
        }

        Self {
            embedder,
            centroids,
        }
    }

    /// Groups tokens into semantic clusters according to weighted centroid similarity.
    ///
    /// # Panics
    /// Never.
    #[must_use]
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

impl Default for ClusterMapper {
    fn default() -> Self {
        Self::new()
    }
}
