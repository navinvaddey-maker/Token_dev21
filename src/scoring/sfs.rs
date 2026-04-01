use crate::scoring::weights::IssueWeights;
use crate::types::FieldValidationIssue;

/// SemanticFidelityScorer calculates the Schema Fidelity Score (SFS) based on field validation issues.
pub struct SemanticFidelityScorer;

impl SemanticFidelityScorer {
    /// Creates a new SemanticFidelityScorer instance
    pub fn new() -> Self {
        Self
    }

    /// Calculates the SFS score.
    ///
    /// # Arguments
    ///
    /// * `field_issues` - Slice of field validation issues found in the output.
    ///
    /// # Returns
    ///
    /// A score between 0.0 and 1.0, where higher is better (fewer/severe issues).
    pub fn score(field_issues: &[FieldValidationIssue]) -> f32 {
        let issue_weights = IssueWeights::sfs_weights();
        let total_weight = issue_weights.calculate_total_weight(field_issues);
        // SFS is inversely related to the total weight of issues
        // Using exponential decay: e^(-total_weight) maps [0, inf) to (0, 1]
        // This ensures score is 1.0 when no issues, and approaches 0 as issues increase
        (-total_weight).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FieldValidationIssue;

    #[test]
    fn test_sfs_no_issues() {
        let score = SemanticFidelityScorer::score(&[]);
        assert_eq!(score, 1.0); // No issues means perfect fidelity
    }

    #[test]
    fn test_sfs_with_issues() {
        let field_issues = vec![
            FieldValidationIssue {
                field_name: "task".to_string(),
                issue_type: "type_mismatch".to_string(),
                description: "Task field expects String but got Number".to_string(),
                severity: "error".to_string(),
            },
            FieldValidationIssue {
                field_name: "deliverable".to_string(),
                issue_type: "missing_required".to_string(),
                description: "Deliverable field is required but missing".to_string(),
                severity: "error".to_string(),
            },
        ];
        let score = SemanticFidelityScorer::score(&field_issues);
        // Should be less than 1.0 due to penalty from issues
        assert!(score < 1.0);
        // Should still be positive
        assert!(score > 0.0);
    }

    #[test]
    fn test_sfs_many_issues() {
        let field_issues = vec![
            FieldValidationIssue {
                field_name: "task".to_string(),
                issue_type: "type_mismatch".to_string(),
                description: "Task field expects String but got Number".to_string(),
                severity: "error".to_string(),
            },
            FieldValidationIssue {
                field_name: "deliverable".to_string(),
                issue_type: "missing_required".to_string(),
                description: "Deliverable field is required but missing".to_string(),
                severity: "error".to_string(),
            },
            FieldValidationIssue {
                field_name: "constraints".to_string(),
                issue_type: "invalid_value".to_string(),
                description: "Constraints field contains invalid JSON".to_string(),
                severity: "error".to_string(),
            },
            FieldValidationIssue {
                field_name: "context".to_string(),
                issue_type: "type_mismatch".to_string(),
                description: "Context field expects Object but got Array".to_string(),
                severity: "warning".to_string(),
            },
        ];
        let score = SemanticFidelityScorer::score(&field_issues);
        // Should be lower than with fewer issues
        let fewer_issues = vec![FieldValidationIssue {
            field_name: "task".to_string(),
            issue_type: "type_mismatch".to_string(),
            description: "Task field expects String but got Number".to_string(),
            severity: "error".to_string(),
        }];
        let score_fewer = SemanticFidelityScorer::score(&fewer_issues);
        assert!(score < score_fewer);
    }
}
