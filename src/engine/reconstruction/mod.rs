pub mod deduplicator;
pub mod cluster_mapper;
pub mod slot_inferencer;
pub mod ambiguity_register;

use crate::types::ReconstructedInput;

pub struct TokenReconstructor {
    pub deduplicator: deduplicator::Deduplicator,
    pub cluster_mapper: cluster_mapper::ClusterMapper,
    pub slot_inferencer: slot_inferencer::SlotInferencer,
    pub ambiguity_register: ambiguity_register::AmbiguityRegister,
}

impl TokenReconstructor {
    pub fn new() -> Self {
        Self {
            deduplicator: deduplicator::Deduplicator::new(),
            cluster_mapper: cluster_mapper::ClusterMapper::new(),
            slot_inferencer: slot_inferencer::SlotInferencer::new(),
            ambiguity_register: ambiguity_register::AmbiguityRegister::new(),
        }
    }

    pub fn run(&self, input: &str) -> ReconstructedInput {
        // Step 1: Repetition scoring
        let deduplicated = self.deduplicator.process(input);
        
        // Steps 2 & 3: Typo correction & Truncation completion
        let (resolved_tokens, ambiguities) = self.ambiguity_register.resolve(&deduplicated);
        
        // Step 4: Semantic clustering
        let clusters = self.cluster_mapper.group_into_clusters(&resolved_tokens);
        
        // Step 5: Schema slot inference
        let (slot_map, locks) = self.slot_inferencer.infer_slots(&clusters);
        
        ReconstructedInput {
            clusters: slot_map,
            constraint_locks: locks,
            ambiguity_register: ambiguities,
            input_structure_score: self.calculate_structure_score(&resolved_tokens),
        }
    }

    fn calculate_structure_score(&self, tokens: &[crate::types::WeightedToken]) -> f32 {
        // Simple heuristic for structure score: proportion of tokens with normal weights 
        // implies less repetition and more structure. Return 0.5 as a baseline.
        if tokens.is_empty() {
            return 0.0;
        }
        let normal_tokens = tokens.iter().filter(|t| t.weight <= 1.0).count();
        normal_tokens as f32 / tokens.len() as f32
    }
}

impl Default for TokenReconstructor {
    fn default() -> Self {
        Self::new()
    }
}
