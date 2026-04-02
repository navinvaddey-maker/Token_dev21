use crate::scoring::weights::IssueWeights;
use crate::types::{AlgorithmOutput, FieldValidationIssue};

/// TokenEfficiencyScorer calculates the Token-Essential Score (TES) based on token efficiency metrics and field validation issues.
pub struct TokenEfficiencyScorer;

impl TokenEfficiencyScorer {
    /// Creates a new TokenEfficiencyScorer instance
    pub fn new() -> Self {
        Self
    }
}

impl TokenEfficiencyScorer {
    /// Calculates the TES score in 0.0–10.0 range.
    ///
    /// @param output - AlgorithmOutput with input/output token counts.
    /// @param field_issues - Slice of field validation issues.
    /// @returns (tes_score, tes_issues) — score clamped 0.0..10.0, issues passthrough.
    pub fn score(output: &AlgorithmOutput, field_issues: &[FieldValidationIssue]) -> (f32, Vec<FieldValidationIssue>) {
        let raw = Self::calculate_raw(
            output.input_token_count,
            output.output_token_count,
            field_issues,
        );
        // Scale to 0–10: perfect compression (ratio≤0.5) scores 10; no compression scores 0.
        let tes = (raw * 10.0).clamp(0.0, 10.0);
        (tes, field_issues.to_vec())
    }

    fn calculate_raw(
        input_tokens: u32,
        output_tokens: u32,
        field_issues: &[FieldValidationIssue],
    ) -> f32 {
        if input_tokens == 0 {
            return 0.0;
        }

        let compression_ratio = output_tokens as f32 / input_tokens as f32;
        let issue_weights = IssueWeights::tes_weights();
        let total_weight = issue_weights.calculate_total_weight(field_issues);
        let penalty = total_weight / (total_weight + 1.0); // maps [0, inf) to [0, 1)
        compression_ratio * (1.0 - penalty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FieldValidationIssue;

    #[test]
    fn test_tes_no_issues() {
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 50,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        assert_eq!(score, 5.0); // 50/100 = 0.5 ratio -> scaled to 5.0
    }

    #[test]
    fn test_tes_with_issues() {
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 50,
            ..Default::default()
        };
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
        let (score, _) = TokenEfficiencyScorer::score(&output, &field_issues);
        // Should be less than 5.0 due to penalty from issues
        assert!(score < 5.0);
        // Should still be positive
        assert!(score > 0.0);
    }

    #[test]
    fn test_tes_perfect_compression() {
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 100,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        assert_eq!(score, 10.0); // 100/100 = 1.0 ratio -> scaled to 10.0
    }

    #[test]
    fn test_tes_expansion() {
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 150,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        assert_eq!(score, 10.0); // 150/100 ratio clamped to 10.0 in scaled output
    }

    #[test]
    fn test_tes_zero_input() {
        let output = AlgorithmOutput {
            input_token_count: 0,
            output_token_count: 50,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        assert_eq!(score, 0.0); // Handle division by zero
    }
}
