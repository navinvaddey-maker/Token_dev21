use crate::types::{AmbiguityFlag, WeightedToken};

pub struct AmbiguityRegister;

impl AmbiguityRegister {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, tokens: &[WeightedToken]) -> (Vec<WeightedToken>, Vec<AmbiguityFlag>) {
        let mut resolved = Vec::new();
        let mut ambiguities = Vec::new();

        // Hardcoded DOMAIN_VOCAB mockup for constraint resolution
        let _valid_words = ["dairy", "fiber", "macro"];

        for token in tokens {
            let t = &token.text;
            
            // Phoneme truncation mock ("dair" -> "dairy")
            if t == "dair" {
                ambiguities.push(AmbiguityFlag {
                    text: t.clone(),
                    reason: "Truncation detected".to_string(),
                    resolved_as: Some("dairy".to_string()),
                    confidence: 0.94,
                });
                resolved.push(WeightedToken {
                    text: "dairy".to_string(),
                    weight: token.weight,
                });
            } else if t == "conditions" && resolved.last().map(|w| w.text.as_str()) == Some("high") {
                // Phrase sequence detection: "high" + "conditions"
                let prev = resolved.pop().unwrap();
                let combined = format!("{} {}", prev.text, t);
                ambiguities.push(AmbiguityFlag {
                    text: combined.clone(),
                    reason: "Incomplete phrase".to_string(),
                    resolved_as: None,
                    confidence: 0.0,
                });
                resolved.push(WeightedToken {
                    text: combined,
                    weight: token.weight,
                });
            } else {
                resolved.push(token.clone());
            }
        }

        (resolved, ambiguities)
    }
}
