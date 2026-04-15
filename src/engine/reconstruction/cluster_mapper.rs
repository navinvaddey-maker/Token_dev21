use crate::types::WeightedToken;
use std::collections::HashMap;

pub struct ClusterMapper;

impl ClusterMapper {
    pub fn new() -> Self {
        Self
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
        // Programmatic mock of semantic domains. In production, this would use an embedder or graph map.
        match text {
            "professional" | "nutritionist" | "specializing" | "expert" => "Role".to_string(),
            "athletes" | "marathon" | "runner" | "elite" => "Subject".to_string(),
            "create" | "plan" | "training" | "week" | "schedule" => "Task".to_string(),
            "low" | "fiber" | "nightshades" | "dairy" | "anti" | "inflammatory" | "carb" | "high" => "Constraints".to_string(),
            "macro" | "breakdowns" | "exact" | "options" | "list" => "Output".to_string(),
            _ => "General".to_string(),
        }
    }
}
