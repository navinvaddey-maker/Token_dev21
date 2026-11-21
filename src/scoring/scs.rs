//! Semantic Completeness Scorer (`ARCHITECTURE.md` §1.3, §4).
//!
//! Evaluates whether all user intent (constraints, deliverables, resolved ambiguities)
//! is preserved in the active output text and resolved schema, without falling back
//! to pre-sparse `clean_tokens`.

use crate::types::AlgorithmOutput;

/// Semantic Completeness Score (SCS) — v2 implementation.
///
/// Measures whether all user intent (explicit + implicit) is present in the output.
///
/// Formula:
/// ```text
///   SCS = (
///     constraints_preserved / constraints_total * 0.4 +
///     deliverables_present / deliverables_expected * 0.4 +
///     ambiguities_resolved / ambiguities_flagged * 0.2
///   ) * 10
/// ```
pub struct SemanticCompletenessScorer;

impl SemanticCompletenessScorer {
    /// Computes the Semantic Completeness Score in `[0.0, 10.0]` and any penalty explanations.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn score(out: &AlgorithmOutput) -> (f32, Vec<String>) {
        let mut penalties = Vec::new();

        // ── Dimension 1: Constraint Preservation (40% weight) ──────────────
        let constraints_total = out.constraint_locks.len();
        let constraint_ratio = if constraints_total > 0 {
            // ARCH: Balanced Mode Contract (E1) — evaluate active output, NEVER pre-sparse `clean_tokens`.
            // FIX: Removed fallback to `out.clean_tokens` which masked dropped constraints in Stage 1/6A.
            // When `force_compact_generation` is true, check against `rendered_output` (or `sparse_tokens`
            // + `resolved_schema.constraints`); when false, check against `rendered_output` or `schema_text`.
            let active_text = Self::active_output_text(out);

            let preserved = out
                .constraint_locks
                .iter()
                .filter(|c| {
                    let ct = c.text.to_lowercase();
                    Self::text_contains_constraint(&active_text, &ct)
                })
                .count();

            if preserved < constraints_total {
                penalties.push(format!(
                    "Constraint loss: {}/{} constraints preserved",
                    preserved, constraints_total
                ));
            }
            preserved as f32 / constraints_total as f32
        } else {
            1.0
        };

        // ── Dimension 2: Deliverable Presence (40% weight) ─────────────────
        let deliverables_expected = out.expected_deliverables.len();
        let deliverable_ratio = if deliverables_expected > 0 {
            let present = out.resolved_schema.output.len();
            if present < deliverables_expected {
                penalties.push(format!(
                    "Missing deliverables: {}/{} present",
                    present, deliverables_expected
                ));
            }
            (present as f32 / deliverables_expected as f32).min(1.0)
        } else {
            // If none expected, check if at least schema filling worked for core fields
            if out.resolved_schema.task.is_some() {
                1.0
            } else {
                0.5
            }
        };

        // ── Dimension 3: Ambiguity Resolution (20% weight) ─────────────────
        let ambiguities_total = out.ambiguity_register.len();
        let ambiguity_ratio = if ambiguities_total > 0 {
            let resolved = out
                .ambiguity_register
                .iter()
                .filter(|a| a.resolved_as.is_some() && a.confidence >= 0.7)
                .count();

            if resolved < ambiguities_total {
                penalties.push(format!(
                    "Unresolved ambiguities: {}/{} resolved",
                    resolved, ambiguities_total
                ));
            }
            resolved as f32 / ambiguities_total as f32
        } else {
            1.0
        };

        // ── Composite Score ────────────────────────────────────────────────
        let raw_score =
            (constraint_ratio * 0.4) + (deliverable_ratio * 0.4) + (ambiguity_ratio * 0.2);

        let score = (raw_score * 10.0).clamp(0.0, 10.0);

        (score, penalties)
    }

    /// Constructs the lowercased active output text against which constraint preservation is verified.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn active_output_text(out: &AlgorithmOutput) -> String {
        if let Some(ref rendered) = out.rendered_output {
            if !rendered.trim().is_empty() {
                return rendered.to_lowercase();
            }
        }

        if out.force_compact_generation {
            let sparse_part = out
                .sparse_tokens
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let constraints_part = out
                .resolved_schema
                .constraints
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            format!("{sparse_part} {constraints_part}").to_lowercase()
        } else {
            format!(
                "{} {} {} {} {}",
                out.resolved_schema.role.as_deref().unwrap_or(""),
                out.resolved_schema.context.as_deref().unwrap_or(""),
                out.resolved_schema.task.as_deref().unwrap_or(""),
                out.resolved_schema
                    .constraints
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                out.resolved_schema
                    .output
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .to_lowercase()
        }
    }

    /// Checks whether `active_text` (already lowercased) contains `constraint_lower` either as
    /// a substring or as a contiguous word sequence (ignoring punctuation).
    fn text_contains_constraint(active_text: &str, constraint_lower: &str) -> bool {
        if active_text.contains(constraint_lower) {
            return true;
        }
        let target_words: Vec<&str> = constraint_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if target_words.is_empty() {
            return true;
        }
        let active_words: Vec<&str> = active_text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if target_words.len() == 1 {
            return active_words.contains(&target_words[0]);
        }
        active_words
            .windows(target_words.len())
            .any(|w| w == target_words.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        AlgorithmOutput, AmbiguityFlag, CompressionSchema, ConstraintToken, Deliverable,
    };

    #[test]
    fn test_scs_perfect_no_constraints() {
        // All schema fields present, no constraints or ambiguities
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: Some("Expert".into()),
                context: Some("Test context".into()),
                task: Some("Build something".into()),
                constraints: Vec::new(),
                output: Vec::new(),
            },
            ..Default::default()
        };
        let (score, penalties) = SemanticCompletenessScorer::score(&output);
        assert_eq!(
            score, 10.0,
            "Perfect schema with no constraints should score 10.0"
        );
        assert!(penalties.is_empty());
    }

    #[test]
    fn test_scs_missing_task_no_deliverables() {
        // Missing task, no deliverables expected
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: None,
                context: Some("Some context".into()),
                task: None,
                constraints: Vec::new(),
                output: Vec::new(),
            },
            ..Default::default()
        };
        let (score, _penalties) = SemanticCompletenessScorer::score(&output);
        // deliverable_ratio = 0.5 (task missing), constraints = 1.0, ambiguity = 1.0
        // score = (1.0*0.4 + 0.5*0.4 + 1.0*0.2) * 10 = 8.0
        assert_eq!(
            score, 8.0,
            "Missing task should reduce score to 8.0, got {}",
            score
        );
    }

    #[test]
    fn test_scs_with_unresolved_ambiguities() {
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: Some("Expert".into()),
                context: Some("Test".into()),
                task: Some("Build".into()),
                constraints: Vec::new(),
                output: Vec::new(),
            },
            ambiguity_register: vec![
                AmbiguityFlag {
                    text: "high conditions".into(),
                    reason: "incomplete phrase".into(),
                    resolved_as: None,
                    confidence: 0.3,
                },
                AmbiguityFlag {
                    text: "dair".into(),
                    reason: "truncated".into(),
                    resolved_as: Some("dairy".into()),
                    confidence: 0.94,
                },
            ],
            ..Default::default()
        };
        let (score, _penalties) = SemanticCompletenessScorer::score(&output);
        // ambiguity_ratio = 1/2 = 0.5
        // score = (1.0*0.4 + 1.0*0.4 + 0.5*0.2) * 10 = 9.0
        assert!(
            score < 10.0,
            "Unresolved ambiguity should penalize, got {}",
            score
        );
        assert!(
            score > 8.0,
            "One resolved ambiguity should still be decent, got {}",
            score
        );
    }

    #[test]
    fn test_scs_with_lost_constraints() {
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: Some("Nutritionist".into()),
                context: Some("Marathon runner needs meal plan".into()),
                task: Some("Create plan".into()),
                constraints: Vec::new(),
                output: Vec::new(),
            },
            constraint_locks: vec![
                ConstraintToken {
                    text: "low fiber".into(),
                    weight: 1.0,
                },
                ConstraintToken {
                    text: "no dairy".into(),
                    weight: 1.0,
                },
                ConstraintToken {
                    text: "anti-inflammatory".into(),
                    weight: 1.0,
                },
            ],
            // Only "low fiber" is NOT in the schema text, but the others aren't either
            // since schema text is "nutritionist marathon runner needs meal plan create plan"
            ..Default::default()
        };
        let (score, penalties) = SemanticCompletenessScorer::score(&output);
        // All 3 constraints are lost from the schema text
        // constraint_ratio = 0/3 = 0.0
        // score = (0.0*0.4 + 1.0*0.4 + 1.0*0.2) * 10 = 6.0
        assert!(
            score < 8.0,
            "Lost constraints should heavily penalize, got {}",
            score
        );
        assert!(penalties.iter().any(|p| p.contains("Constraint loss")));
    }

    #[test]
    fn test_scs_missing_deliverables() {
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: Some("Expert".into()),
                context: Some("Test".into()),
                task: Some("Build".into()),
                constraints: Vec::new(),
                output: vec![Deliverable {
                    name: "code".into(),
                }],
            },
            expected_deliverables: vec!["code".into(), "tests".into()],
            ..Default::default()
        };
        let (score, penalties) = SemanticCompletenessScorer::score(&output);
        // deliverable_ratio = 1/2 = 0.5
        // score = (1.0*0.4 + 0.5*0.4 + 1.0*0.2) * 10 = 8.0
        assert_eq!(score, 8.0);
        assert!(penalties.iter().any(|p| p.contains("Missing deliverables")));
    }

    #[test]
    fn test_scs_all_empty() {
        let output = AlgorithmOutput::default();
        let (score, _) = SemanticCompletenessScorer::score(&output);
        // task is None, so deliverable_ratio = 0.5
        // score = (1.0*0.4 + 0.5*0.4 + 1.0*0.2) * 10 = 8.0
        assert_eq!(score, 8.0);
    }

    #[test]
    fn test_scs_does_not_credit_clean_tokens_when_missing_from_active_output() {
        let output = AlgorithmOutput {
            resolved_schema: CompressionSchema {
                role: Some("Expert".into()),
                context: Some("CSV parser".into()),
                task: Some("Write function".into()),
                constraints: Vec::new(),
                output: Vec::new(),
            },
            clean_tokens: vec!["must".into(), "not".into(), "pandas".into()],
            sparse_tokens: vec![],
            constraint_locks: vec![ConstraintToken {
                text: "no pandas".into(),
                weight: 1.0,
            }],
            force_compact_generation: true,
            ..Default::default()
        };
        let (score, penalties) = SemanticCompletenessScorer::score(&output);
        assert!(
            score <= 6.0,
            "Constraint present only in clean_tokens must NOT be credited, got {}",
            score
        );
        assert!(penalties.iter().any(|p| p.contains("Constraint loss")));
    }
}
