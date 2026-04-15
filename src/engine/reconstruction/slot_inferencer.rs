use crate::types::{ConstraintToken, SlotType, WeightedToken};
use std::collections::HashMap;

pub struct SlotInferencer;

impl SlotInferencer {
    pub fn new() -> Self {
        Self
    }

    pub fn infer_slots(
        &self,
        clusters: &HashMap<String, Vec<WeightedToken>>,
    ) -> (HashMap<SlotType, Vec<WeightedToken>>, Vec<ConstraintToken>) {
        let mut slot_map = HashMap::new();
        let mut locks = Vec::new();

        for (cluster_name, tokens) in clusters {
            let slot_type = match cluster_name.as_str() {
                "Role" => SlotType::Role,
                "Subject" => SlotType::Context,
                "Task" | "General" => SlotType::Task,
                "Constraints" => SlotType::Constraint,
                "Output" => SlotType::Output,
                _ => SlotType::Task,
            };

            slot_map.insert(slot_type.clone(), tokens.clone());

            // Build constraint locks
            if slot_type == SlotType::Constraint {
                for token in tokens {
                    locks.push(ConstraintToken {
                        text: token.text.clone(),
                        weight: token.weight,
                    });
                }
            }
        }

        (slot_map, locks)
    }
}
