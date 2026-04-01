use crate::types::FieldValidationIssue;
use serde::{Deserialize, Serialize};

/// Weights for Task-Essential Score (TES) calculation.
/// These weights determine how much each field contributes to assessing
/// whether the essential task requirements are met.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssueWeights {
    /// Weight for task field issues (0.0-1.0)
    pub task: f32,

    /// Weight for deliverable field issues (0.0-1.0)
    pub deliverable: f32,

    /// Weight for constraints field issues (0.0-1.0)
    pub constraints: f32,

    /// Weight for context field issues (0.0-1.0)
    pub context: f32,
}

impl IssueWeights {
    /// Returns the default weights for TES calculation.
    /// Task and deliverable fields are weighted higher as they are essential.
    pub fn tes_weights() -> Self {
        Self {
            task: 0.3,
            deliverable: 0.3,
            constraints: 0.2,
            context: 0.2,
        }
    }

    /// Returns the default weights for SFS calculation.
    /// All fields are weighted equally for schema fidelity assessment.
    pub fn sfs_weights() -> Self {
        Self {
            task: 0.25,
            deliverable: 0.25,
            constraints: 0.25,
            context: 0.25,
        }
    }

    /// Normalizes the weights so they sum to 1.0.
    /// Useful when weights have been modified and need to be rescaled.
    pub fn normalize(&mut self) {
        let total = self.task + self.deliverable + self.constraints + self.context;
        if total > 0.0 {
            self.task /= total;
            self.deliverable /= total;
            self.constraints /= total;
            self.context /= total;
        }
    }

    /// Creates a new IssueWeights with validation that all weights are in [0,1] range.
    pub fn new(task: f32, deliverable: f32, constraints: f32, context: f32) -> Self {
        Self {
            task: task.clamp(0.0, 1.0),
            deliverable: deliverable.clamp(0.0, 1.0),
            constraints: constraints.clamp(0.0, 1.0),
            context: context.clamp(0.0, 1.0),
        }
    }

    /// Calculates the total weight of field validation issues.
    /// Each issue contributes the weight of its corresponding field.
    ///
    /// # Arguments
    ///
    /// * `field_issues` - Slice of field validation issues
    ///
    /// # Returns
    ///
    /// Sum of weights for all issues (higher means more severe issues)
    pub fn calculate_total_weight(&self, field_issues: &[FieldValidationIssue]) -> f32 {
        let mut total = 0.0;
        for issue in field_issues {
            match issue.field_name.as_str() {
                "task" => total += self.task,
                "deliverable" => total += self.deliverable,
                "constraints" => total += self.constraints,
                "context" => total += self.context,
                _ => {} // Unknown field, no weight contribution
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tes_weights_defaults() {
        let weights = IssueWeights::tes_weights();
        assert_eq!(weights.task, 0.3);
        assert_eq!(weights.deliverable, 0.3);
        assert_eq!(weights.constraints, 0.2);
        assert_eq!(weights.context, 0.2);

        // Should sum to 1.0
        let total = weights.task + weights.deliverable + weights.constraints + weights.context;
        assert!((total - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sfs_weights_defaults() {
        let weights = IssueWeights::sfs_weights();
        assert_eq!(weights.task, 0.25);
        assert_eq!(weights.deliverable, 0.25);
        assert_eq!(weights.constraints, 0.25);
        assert_eq!(weights.context, 0.25);

        // Should sum to 1.0
        let total = weights.task + weights.deliverable + weights.constraints + weights.context;
        assert!((total - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalize() {
        let mut weights = IssueWeights::new(0.5, 0.5, 0.0, 0.0);
        weights.normalize();

        // Should now be 0.5, 0.5, 0.0, 0.0 normalized to sum to 1.0
        assert!((weights.task - 0.5).abs() < f32::EPSILON);
        assert!((weights.deliverable - 0.5).abs() < f32::EPSILON);
        assert!((weights.constraints - 0.0).abs() < f32::EPSILON);
        assert!((weights.context - 0.0).abs() < f32::EPSILON);

        let total = weights.task + weights.deliverable + weights.constraints + weights.context;
        assert!((total - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clamp_values() {
        let weights = IssueWeights::new(-0.5, 1.5, 0.8, -0.2);
        assert_eq!(weights.task, 0.0); // Clamped to 0.0
        assert_eq!(weights.deliverable, 1.0); // Clamped to 1.0
        assert_eq!(weights.constraints, 0.8); // Unchanged
        assert_eq!(weights.context, 0.0); // Clamped to 0.0
    }
}
