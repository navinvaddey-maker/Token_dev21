use crate::types::AlgorithmOutput;

pub struct SemanticCompletenessScorer;

impl SemanticCompletenessScorer {
    pub fn score(out: &AlgorithmOutput) -> (f32, Vec<String>) {
        let mut score: f32 = 10.0;
        let mut penalties = Vec::new();
        
        let schema = &out.resolved_schema;
        
        // Base starts at 10.0
        // Penalty -2.0 for every missing field out of the 5 explicit fields
        if schema.role.is_none() { 
            score -= 2.0; 
            penalties.push("Missing role field".to_string());
        }
        if schema.context.is_none() { 
            score -= 2.0; 
            penalties.push("Missing context field".to_string());
        }
        if schema.task.is_none() { 
            score -= 2.0; 
            penalties.push("Missing task field".to_string());
        }
        if schema.constraints.is_empty() { 
            score -= 2.0; 
            penalties.push("Missing constraints field".to_string());
        }
        if schema.output.is_empty() { 
            score -= 2.0; 
            penalties.push("Missing output deliverables field".to_string());
        }

        (score.clamp(0.0, 10.0), penalties)
    }
}
