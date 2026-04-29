use crate::types::AlgorithmOutput;

pub struct SemanticCompletenessScorer;

impl SemanticCompletenessScorer {
    pub fn score(out: &AlgorithmOutput) -> (f32, Vec<String>) {
        let mut score: f32 = 10.0;
        let mut penalties = Vec::new();
        
        let schema = &out.resolved_schema;
        
        // Penalty -3.3 for every missing field out of the 3 explicit fields
        if schema.role.is_none() { 
            score -= 3.3; 
            penalties.push("Missing role field".to_string());
        }
        if schema.context.is_none() { 
            score -= 3.3; 
            penalties.push("Missing context field".to_string());
        }
        if schema.task.is_none() { 
            score -= 3.3; 
            penalties.push("Missing task field".to_string());
        }

        (score.clamp(0.0, 10.0), penalties)

    }
}
