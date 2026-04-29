use crate::scoring::weights::IssueWeights;
use crate::types::FieldValidationIssue;

/// SemanticFidelityScorer calculates the Schema Fidelity Score (SFS) based on field validation issues.
pub struct SemanticFidelityScorer;

impl SemanticFidelityScorer {
    /// Creates a new SemanticFidelityScorer instance
    pub fn new() -> Self {
        Self
    }

    /// Calculates the SFS score in 0.0–10.0 range.
    ///
    /// @param field_issues - Slice of field validation issues.
    /// @returns (sfs_score, sfs_issues) — scaled score and passthrough issues.
    pub fn score(field_issues: &[FieldValidationIssue]) -> (f32, Vec<FieldValidationIssue>) {
        let issue_weights = IssueWeights::sfs_weights();
        let total_weight = issue_weights.calculate_total_weight(field_issues);
        // SFS is inversely related to the total weight of issues
        // Using exponential decay: e^(-total_weight) maps [0, inf) to (0, 1]
        // Scale to 0-10 range
        let sfs = ((-total_weight).exp() * 10.0).clamp(0.0, 10.0);
        (sfs, field_issues.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FieldValidationIssue;

    #[test]
    fn test_sfs_no_issues() {
        let (score, _) = SemanticFidelityScorer::score(&[]);
        assert_eq!(score, 10.0); // No issues means perfect fidelity (10.0)
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
                field_name: "context".to_string(),
                issue_type: "missing_required".to_string(),
                description: "Context field is required but missing".to_string(),
                severity: "error".to_string(),
            },
        ];
        let (score, _) = SemanticFidelityScorer::score(&field_issues);
        // Should be less than 10.0 due to penalty from issues
        assert!(score < 10.0);
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
                field_name: "context".to_string(),
                issue_type: "invalid_value".to_string(),
                description: "Context field contains invalid format".to_string(),
                severity: "error".to_string(),
            },
            FieldValidationIssue {
                field_name: "context".to_string(),
                issue_type: "type_mismatch".to_string(),
                description: "Context field expects Object but got Array".to_string(),
                severity: "warning".to_string(),
            },
        ];
        let (score, _) = SemanticFidelityScorer::score(&field_issues);
        // Should be lower than with fewer issues
        let fewer_issues = vec![FieldValidationIssue {
            field_name: "task".to_string(),
            issue_type: "type_mismatch".to_string(),
            description: "Task field expects String but got Number".to_string(),
            severity: "error".to_string(),
        }];
        let (score_fewer, _) = SemanticFidelityScorer::score(&fewer_issues);
        assert!(score < score_fewer);
    }
}
