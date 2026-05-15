use crate::scoring::weights::IssueWeights;
use crate::types::{AlgorithmOutput, FieldValidationIssue};

/// TokenEfficiencyScorer (TES) — Measures how efficiently the engine compresses
/// while preserving task-essential content.
///
/// v2 Fix (GAP-A01): The v1 scorer was INVERTED — it rewarded no-compression
/// (output/input = 1.0 → score 10.0) and penalized good compression (0.5 → 5.0).
///
/// v2 scoring logic (Refined):
/// - Measures compression savings: `savings = 1.0 - (output / input)`
/// - Rewards efficient compression while penalizing both under-compression
///   (wasted tokens) and extreme over-compression (likely content loss).
/// - Sweet spot: 30-60% savings → score 8.0-9.0.
/// - Preservation factor: Scaled by field validation issues.
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
    /// Scoring formula (v2):
    ///   base = f(compression_savings, preservation_factor)
    ///   tes  = base * (1.0 - issue_penalty)
    ///
    /// Where:
    ///   compression_savings = 1.0 - (output_tokens / input_tokens)
    ///   preservation_factor = bonus for keeping content within useful range
    ///   issue_penalty = field validation issues reduce score
    pub fn score(output: &AlgorithmOutput, field_issues: &[FieldValidationIssue]) -> (f32, Vec<FieldValidationIssue>) {
        let raw = Self::calculate_raw(
            output.input_token_count,
            output.output_token_count,
            field_issues,
        );
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
        
        // Savings = 1.0 - ratio. Expansion (ratio > 1.0) results in 0 savings.
        let savings = (1.0 - compression_ratio).max(0.0);

        // Expert Scoring Curve (v3):
        // Rewards compression savings using a power-law curve: savings^0.32
        // This ensures 50% savings scores ~0.8 (8.0/10.0) as per GAP-A01 requirements.
        // - 0% savings (ratio=1.0) -> 0.0^0.32 = 0.0
        // - 50% savings (ratio=0.5) -> 0.5^0.32 ≈ 0.8
        // - 90% savings (ratio=0.1) -> 0.9^0.32 ≈ 0.96
        let base = savings.powf(0.32);

        // Apply field issue penalty as preservation_factor
        let issue_weights = IssueWeights::tes_weights();
        let total_weight = issue_weights.calculate_total_weight(field_issues);
        let preservation_factor = 1.0 - (total_weight / (total_weight + 1.0));

        base * preservation_factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FieldValidationIssue;

    #[test]
    fn test_tes_good_compression() {
        // 50% compression — sweet spot
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 50,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // 50% savings → base = 0.5^0.32 ≈ 0.8 → score = 8.0
        assert!((score - 8.0).abs() < 0.1, "50% compression should score ~8.0, got {}", score);
    }

    #[test]
    fn test_tes_no_compression_penalized() {
        // No compression — should score zero in v3
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 100,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // 0% savings → base = 0.0 → score = 0.0
        assert_eq!(score, 0.0, "No compression should score 0.0, got {}", score);
    }

    #[test]
    fn test_tes_expansion_penalized() {
        // Expansion — output larger than input
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 150,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // Expansion -> 0 savings -> score 0.0
        assert_eq!(score, 0.0, "Expansion should score 0.0, got {}", score);
    }

    #[test]
    fn test_tes_light_compression() {
        // 20% compression
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 80,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // 20% savings → base = 0.2^0.32 ≈ 0.597 → score ≈ 5.97
        assert!((score - 5.97).abs() < 0.01, "20% compression should score ~5.97, got {}", score);
    }

    #[test]
    fn test_tes_high_compression() {
        // 90% compression — rewarded if preservation is high
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 10,
            ..Default::default()
        };
        let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // 90% savings → base = 0.9^0.32 ≈ 0.966 → score ≈ 9.66
        assert!(score > 9.0, "High compression should be rewarded if no issues, got {}", score);
    }

    #[test]
    fn test_tes_with_field_issues() {
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
        ];
        let (score, _) = TokenEfficiencyScorer::score(&output, &field_issues);
        let (clean_score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // Score with issues should be lower than without
        assert!(score < clean_score, "Issues should reduce score: {} vs {}", score, clean_score);
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

    #[test]
    fn test_tes_ordering() {
        // Verify correct ordering: high compression > good > light > none
        let make_output = |inp: u32, out: u32| AlgorithmOutput {
            input_token_count: inp,
            output_token_count: out,
            ..Default::default()
        };
        let (high, _) = TokenEfficiencyScorer::score(&make_output(100, 10), &[]);     // 90% savings
        let (good, _) = TokenEfficiencyScorer::score(&make_output(100, 50), &[]);     // 50% savings
        let (light, _) = TokenEfficiencyScorer::score(&make_output(100, 80), &[]);    // 20% savings
        let (none, _) = TokenEfficiencyScorer::score(&make_output(100, 100), &[]);    // 0% savings

        assert!(high > good, "High > Good: {} > {}", high, good);
        assert!(good > light, "Good > Light: {} > {}", good, light);
        assert!(light > none, "Light > None: {} > {}", light, none);
    }
}
