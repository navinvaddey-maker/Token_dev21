//! Ory Outcome Evaluator — Closes the learning loop
//!
//! After a blueprint is executed and scores are produced, the evaluator
//! assesses how well the blueprint performed and generates improvement notes.

use crate::npae::ory::types::{LearnedIntent, DynamicBlueprint, PatternOutcome};
use crate::types::ScoringResult;

pub struct OutcomeEvaluator;

impl OutcomeEvaluator {
    /// Evaluate blueprint effectiveness based on scoring results
    pub fn evaluate(
        intent: &LearnedIntent,
        blueprint: &DynamicBlueprint,
        scores: &ScoringResult,
    ) -> PatternOutcome {
        let quality_score = (scores.tes + scores.sfs + scores.scs) / 3.0;
        let success = quality_score >= 6.0;

        let mut notes = Vec::new();

        // Analyze which scores were weak
        if scores.tes < 6.0 {
            notes.push(format!(
                "Low TES ({:.1}): Blueprint phases may not preserve task-essential tokens",
                scores.tes
            ));
        }
        if scores.sfs < 6.0 {
            notes.push(format!(
                "Low SFS ({:.1}): Blueprint structure doesn't match schema expectations",
                scores.sfs
            ));
        }
        if scores.scs < 6.0 {
            notes.push(format!(
                "Low SCS ({:.1}): Semantic completeness gap — check constraint coverage",
                scores.scs
            ));
        }

        // Check if complexity was underestimated
        if intent.complexity_score > 7.0 && quality_score < 7.0 {
            notes.push(format!(
                "Complex request ({:.1}) yielded moderate quality ({:.1}) — consider deeper phases",
                intent.complexity_score, quality_score
            ));
        }

        // Check phase count adequacy
        if blueprint.phases.len() <= 3 && intent.hidden_dependencies.len() > 2 {
            notes.push(format!(
                "Only {} phases for {} dependencies — may need more granular breakdown",
                blueprint.phases.len(), intent.hidden_dependencies.len()
            ));
        }

        // Positive feedback
        if success && quality_score >= 8.0 {
            notes.push("High-quality pattern — candidate for promotion to permanent config".into());
        }

        PatternOutcome {
            pattern_id: blueprint.architecture_id.clone(),
            quality_score,
            success,
            improvement_notes: notes,
        }
    }
}
