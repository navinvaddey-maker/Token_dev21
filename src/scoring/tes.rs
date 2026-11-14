//! Token Efficiency Scorer (`ARCHITECTURE.md` §1.3, §4).
//!
//! Measures compression efficiency in consistent token units while penalizing
//! structural validation issues.

use crate::scoring::weights::IssueWeights;
use crate::types::{AlgorithmOutput, FieldValidationIssue};
use crate::utils::tokens::estimate_tokens;

/// `TokenEfficiencyScorer` (TES) — Measures how efficiently the engine compresses
/// while preserving task-essential content.
///
/// Scoring logic:
/// - Normalizes input and output token counts into identical units (`estimate_tokens`)
/// - Measures compression savings: `savings = 1.0 - (output / input)`
/// - Rewards efficient compression while penalizing under-compression or expansion.
/// - Preservation factor: Scaled by field validation issues.
pub struct TokenEfficiencyScorer;

impl TokenEfficiencyScorer {
    /// Creates a new `TokenEfficiencyScorer` instance.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for TokenEfficiencyScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenEfficiencyScorer {
    /// Calculates the TES score in `0.0..=10.0` range.
    ///
    /// Scoring formula:
    /// ```text
    ///   base = f(compression_savings, preservation_factor)
    ///   tes  = base * (1.0 - issue_penalty)
    /// ```
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn score(
        output: &AlgorithmOutput,
        field_issues: &[FieldValidationIssue],
    ) -> (f32, Vec<FieldValidationIssue>) {
        // ARCH: Balanced Mode Contract (E3) — TES must compare input and output in identical token units.
        // FIX: `lexical.rs` sets `input_token_count = original_word_count`, whereas `stage6b.rs` sets
        // `output_token_count = estimate_tokens(&final_response)`. When `normalization` is present,
        // compute input tokens via `estimate_tokens(&norm.normalized_text)` so both sides use `estimate_tokens`.
        let effective_input_tokens = output
            .normalization
            .as_ref()
            .filter(|n| !n.normalized_text.trim().is_empty())
            .map(|n| estimate_tokens(&n.normalized_text))
            .unwrap_or(output.input_token_count);

        let effective_output_tokens = output
            .rendered_output
            .as_ref()
            .filter(|r| !r.trim().is_empty())
            .map(|r| estimate_tokens(r))
            .unwrap_or(output.output_token_count);

        let raw = Self::calculate_raw(
            effective_input_tokens,
            effective_output_tokens,
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
        assert!(
            (score - 8.0).abs() < 0.1,
            "50% compression should score ~8.0, got {}",
            score
        );
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
        assert!(
            (score - 5.97).abs() < 0.01,
            "20% compression should score ~5.97, got {}",
            score
        );
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
        assert!(
            score > 9.0,
            "High compression should be rewarded if no issues, got {}",
            score
        );
    }

    #[test]
    fn test_tes_with_field_issues() {
        let output = AlgorithmOutput {
            input_token_count: 100,
            output_token_count: 50,
            ..Default::default()
        };
        let field_issues = vec![FieldValidationIssue {
            field_name: "task".to_string(),
            issue_type: "type_mismatch".to_string(),
            description: "Task field expects String but got Number".to_string(),
            severity: "error".to_string(),
        }];
        let (score, _) = TokenEfficiencyScorer::score(&output, &field_issues);
        let (clean_score, _) = TokenEfficiencyScorer::score(&output, &[]);
        // Score with issues should be lower than without
        assert!(
            score < clean_score,
            "Issues should reduce score: {} vs {}",
            score,
            clean_score
        );
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
        let (high, _) = TokenEfficiencyScorer::score(&make_output(100, 10), &[]); // 90% savings
        let (good, _) = TokenEfficiencyScorer::score(&make_output(100, 50), &[]); // 50% savings
        let (light, _) = TokenEfficiencyScorer::score(&make_output(100, 80), &[]); // 20% savings
        let (none, _) = TokenEfficiencyScorer::score(&make_output(100, 100), &[]); // 0% savings

        assert!(high > good, "High > Good: {} > {}", high, good);
        assert!(good > light, "Good > Light: {} > {}", good, light);
        assert!(light > none, "Light > None: {} > {}", light, none);
    }
}
