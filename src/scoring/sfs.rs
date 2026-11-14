//! Schema Fidelity Scorer (`ARCHITECTURE.md` §1.3, §4).
//!
//! Computes SFS from field validation issues and verifies that negation/bound
//! constraint locks and explicit input negations are preserved in the active output.

use crate::scoring::scs::SemanticCompletenessScorer;
use crate::scoring::weights::IssueWeights;
use crate::types::{AlgorithmOutput, FieldValidationIssue};

/// Negation and strict boundary keywords whose loss inverts or corrupts prompt fidelity.
const NEGATION_BOUND_KEYWORDS: &[&str] = &[
    "no", "not", "never", "without", "under", "avoid", "exclude", "dont", "don't", "cannot",
    "can't",
];

/// `SemanticFidelityScorer` calculates the Schema Fidelity Score (SFS) based on field validation
/// issues and negation/constraint preservation in the active output.
pub struct SemanticFidelityScorer;

impl SemanticFidelityScorer {
    /// Creates a new `SemanticFidelityScorer` instance.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Calculates the SFS score in `0.0..=10.0` range from field validation issues alone.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn score(field_issues: &[FieldValidationIssue]) -> (f32, Vec<FieldValidationIssue>) {
        let issue_weights = IssueWeights::sfs_weights();
        let total_weight = issue_weights.calculate_total_weight(field_issues);
        // SFS is inversely related to the total weight of issues
        // Using exponential decay: e^(-total_weight) maps [0, inf) to (0, 1]
        // Scale to 0-10 range
        let sfs = ((-total_weight).exp() * 10.0).clamp(0.0, 10.0);
        (sfs, field_issues.to_vec())
    }

    /// Calculates the SFS score in `0.0..=10.0` range, incorporating both field validation
    /// issues and negation/constraint lock preservation against `output`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn score_with_output(
        output: &AlgorithmOutput,
        field_issues: &[FieldValidationIssue],
    ) -> (f32, Vec<FieldValidationIssue>) {
        let issue_weights = IssueWeights::sfs_weights();
        let mut total_weight = issue_weights.calculate_total_weight(field_issues);
        let mut all_issues = field_issues.to_vec();

        // ARCH: Balanced Mode Contract (E2) — SFS must penalize negation or constraint loss in active output.
        // FIX: Previously SFS only inspected `field_issues` (and `IssueWeights` only weighted "task" and
        // "context"), giving 10.0 even when a negation ("do not speculate", "no dairy") was dropped.
        let active_text = SemanticCompletenessScorer::active_output_text(output);
        let active_words: Vec<&str> = active_text
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|s| !s.is_empty())
            .collect();

        // 1. Check constraint_locks containing negation/bound keywords
        for lock in &output.constraint_locks {
            let lock_lower = lock.text.to_lowercase();
            let lock_words: Vec<&str> = lock_lower
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|s| !s.is_empty())
                .collect();

            let has_negation = lock_words
                .iter()
                .any(|w| NEGATION_BOUND_KEYWORDS.contains(w));

            let is_present = if lock_words.is_empty() {
                true
            } else if lock_words.len() == 1 {
                active_words.contains(&lock_words[0]) || active_text.contains(&lock_lower)
            } else {
                active_text.contains(&lock_lower)
                    || active_words
                        .windows(lock_words.len())
                        .any(|w| w == lock_words.as_slice())
            };

            if !is_present {
                if has_negation {
                    total_weight += 0.6;
                    all_issues.push(FieldValidationIssue {
                        field_name: "constraints".to_string(),
                        issue_type: "negation_loss".to_string(),
                        description: format!(
                            "Negation/bound constraint lock '{}' missing from active output",
                            lock.text
                        ),
                        severity: "error".to_string(),
                    });
                } else {
                    total_weight += 0.25;
                }
            }
        }

        // 2. Also check normalized input for explicit negation tokens that vanished completely from active output
        if let Some(norm) = &output.normalization {
            let input_lower = norm.normalized_text.to_lowercase();
            let input_words: Vec<&str> = input_lower
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|s| !s.is_empty())
                .collect();

            for &neg_kw in &["no", "not", "never", "without", "under"] {
                if input_words.contains(&neg_kw) && !active_words.contains(&neg_kw) {
                    total_weight += 0.6;
                    all_issues.push(FieldValidationIssue {
                        field_name: "task".to_string(),
                        issue_type: "negation_loss".to_string(),
                        description: format!(
                            "Input negation/bound token '{}' missing from active output",
                            neg_kw
                        ),
                        severity: "error".to_string(),
                    });
                }
            }
        }

        let sfs = ((-total_weight).exp() * 10.0).clamp(0.0, 10.0);
        (sfs, all_issues)
    }
}

impl Default for SemanticFidelityScorer {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn test_sfs_penalizes_negation_constraint_loss() {
        use crate::types::{CompressionSchema, ConstraintToken, NormalizationResult};

        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                task: Some("Summarize report about Q4 numbers".into()),
                ..Default::default()
            },
            normalization: Some(NormalizationResult {
                normalized_text: "Summarize report, do not speculate about Q4 numbers".into(),
                normalization_score: 1.0,
                applied_rules: vec![],
            }),
            constraint_locks: vec![ConstraintToken {
                text: "not speculate".into(),
                weight: 1.0,
            }],
            rendered_output: Some("summarize report about q4 numbers".into()),
            ..Default::default()
        };

        let (score, issues) = SemanticFidelityScorer::score_with_output(&output, &[]);
        assert!(
            score < 6.0,
            "Dropping negation constraint must penalize SFS below 6.0, got {}",
            score
        );
        assert!(issues.iter().any(|i| i.issue_type == "negation_loss"));
    }
}
