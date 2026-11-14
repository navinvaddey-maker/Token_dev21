//! Stage 6B: Targeted Quality Correction Loop (`ARCHITECTURE.md` §1.2, §1.3, §4).
//!
//! Iteratively corrects the lowest failing score axis (TES, SFS, SCS), remediates
//! hallucination guard flags inside the loop, normalizes input/output token counts
//! into identical units (`estimate_tokens`), and tracks the best candidate across
//! correction cycles so a later cycle cannot degrade the shipped response.

use crate::{
    algorithms::field_validator::FieldTypeValidator,
    correction::{apply_targeted_correction, MAX_CORRECTION_CYCLES},
    npae::schema::types::HallucinationGuardConfig,
    pipeline::{stage4::Stage4, stage5::Stage5, stage6a::Stage6a},
    scoring::compute_scoring_result,
    types::{
        AlgorithmOutput, CompressionSchema, CorrectionCycle, FieldValidationIssue, ScoreAxis,
        ScoringResult, TextCorrection,
    },
    utils::tokens::estimate_tokens,
};

/// Handles Stage 6B: Targeted Quality Correction Loop.
#[derive(Default)]
pub struct Stage6b;

/// Packaged result of Stage 6B correction and verification.
pub struct Stage6bOutput {
    pub final_response: String,
    pub guard_report: crate::npae::hallucination::guard::HallucinationReport,
    pub corrections_applied: Vec<TextCorrection>,
    pub field_issues: Vec<FieldValidationIssue>,
    pub scoring_result: ScoringResult,
    pub correction_cycle: CorrectionCycle,
}

/// Snapshot of a candidate generated during a Stage 6B cycle.
#[derive(Clone)]
struct CandidateSnapshot {
    response: String,
    guard_report: crate::npae::hallucination::guard::HallucinationReport,
    field_issues: Vec<FieldValidationIssue>,
    scoring_result: ScoringResult,
    resolved_schema: CompressionSchema,
    force_compact: bool,
    output_tokens: u32,
}

impl CandidateSnapshot {
    /// Minimum score across all three axes (`TES`, `SFS`, `SCS`).
    fn min_axis(&self) -> f32 {
        self.scoring_result
            .tes
            .min(self.scoring_result.sfs)
            .min(self.scoring_result.scs)
    }

    /// Mean score across all three axes (`TES`, `SFS`, `SCS`).
    fn avg_score(&self) -> f32 {
        (self.scoring_result.tes + self.scoring_result.sfs + self.scoring_result.scs) / 3.0
    }

    /// Returns `true` if `self` is a strictly better candidate than `other` given `input_tokens`.
    ///
    /// Ranking order:
    /// 1. Non-expanding candidate (`output_tokens <= input_tokens`) beats expanding candidate.
    /// 2. Passed `HallucinationGuard` beats failed guard when both have comparable compression.
    /// 3. Higher minimum axis score (`min(tes, sfs, scs)`) wins.
    /// 4. Higher average score (`(tes + sfs + scs) / 3.0`) breaks ties.
    fn is_better_than(&self, other: &Self, input_tokens: u32) -> bool {
        let self_compressed = input_tokens == 0 || self.output_tokens <= input_tokens;
        let other_compressed = input_tokens == 0 || other.output_tokens <= input_tokens;

        if self_compressed != other_compressed {
            return self_compressed;
        }
        if self.guard_report.passed != other.guard_report.passed {
            return self.guard_report.passed;
        }
        let self_min = self.min_axis();
        let other_min = other.min_axis();
        if (self_min - other_min).abs() > 0.05 {
            return self_min > other_min;
        }
        self.avg_score() > other.avg_score()
    }
}

impl Stage6b {
    /// Creates a new `Stage6b` instance.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Executes Stage 6B targeted correction loop.
    ///
    /// Iterates until TES, SFS, and SCS score thresholds (`>= 6.0`) and `HallucinationGuard`
    /// are satisfied, or `MAX_CORRECTION_CYCLES` is reached, retaining the best candidate.
    ///
    /// # Errors
    /// Returns `Err` if `Stage6a::run` fails during regeneration.
    ///
    /// # Panics
    /// Never.
    pub async fn run(
        &self,
        input: &str,
        output: &mut AlgorithmOutput,
        guard_cfg: &HallucinationGuardConfig,
        field_validator: &FieldTypeValidator,
        stage4: &Stage4,
        stage5: &Stage5,
        stage6a: &Stage6a,
        initial_response: String,
        initial_report: crate::npae::hallucination::guard::HallucinationReport,
        initial_corrections: Vec<TextCorrection>,
    ) -> Result<Stage6bOutput, Box<dyn std::error::Error>> {
        // ARCH: Balanced Mode Contract (E3) — normalize `input_token_count` to `estimate_tokens`
        // so `input_token_count` and `output_token_count` use identical BPE-approximated units.
        // FIX: `lexical.rs` stored raw word count in `input_token_count` while `stage6b.rs` used
        // `estimate_tokens` (`words * 1.3`) for `output_token_count`.
        let normalized_input_tokens = if let Some(ref norm) = output.normalization {
            if !norm.normalized_text.trim().is_empty() {
                estimate_tokens(&norm.normalized_text)
            } else {
                estimate_tokens(input)
            }
        } else if !input.trim().is_empty() {
            estimate_tokens(input)
        } else {
            output.input_token_count
        };
        if normalized_input_tokens > 0 {
            output.input_token_count = normalized_input_tokens;
        }

        let mut final_response = initial_response;
        let mut guard_report = initial_report;
        let mut corrections_applied = initial_corrections;
        let mut correction_cycle = CorrectionCycle::new(1);
        let mut cycle_num = 0u32;

        output.rendered_output = Some(final_response.clone());
        output.output_token_count = estimate_tokens(&final_response);
        let mut field_issues = field_validator.validate_full(&output.resolved_schema, output);
        let mut scoring_result = compute_scoring_result(output, &field_issues);

        let mut best_candidate: Option<CandidateSnapshot> = None;

        loop {
            output.rendered_output = Some(final_response.clone());
            output.output_token_count = estimate_tokens(&final_response);
            field_issues = field_validator.validate_full(&output.resolved_schema, output);
            output.field_issues = field_issues.clone();

            let before_scoring = scoring_result.clone();
            scoring_result = compute_scoring_result(output, &field_issues);
            output.scoring_result = Some(scoring_result.clone());

            let current_snapshot = CandidateSnapshot {
                response: final_response.clone(),
                guard_report: guard_report.clone(),
                field_issues: field_issues.clone(),
                scoring_result: scoring_result.clone(),
                resolved_schema: output.resolved_schema.clone(),
                force_compact: output.force_compact_generation,
                output_tokens: output.output_token_count,
            };

            // ARCH: Balanced Mode Contract (E4) — track best candidate across correction cycles.
            // FIX: Previously whatever the last correction cycle produced was shipped, even if
            // fixing one axis degraded another or re-expanded the prompt.
            let update_best = match &best_candidate {
                None => true,
                Some(prev) => current_snapshot.is_better_than(prev, output.input_token_count),
            };
            if update_best {
                best_candidate = Some(current_snapshot);
            }

            let scores_ok = scoring_result.tes >= 6.0
                && scoring_result.sfs >= 6.0
                && scoring_result.scs >= 6.0
                && guard_report.passed;

            if scores_ok || cycle_num >= MAX_CORRECTION_CYCLES {
                if !scores_ok {
                    correction_cycle = correction_cycle.new_cycle_with_delta(
                        &final_response,
                        &field_issues,
                        &before_scoring,
                        &scoring_result,
                    );
                }
                break;
            }

            let Some(axis) = scoring_result.correction_axis.clone() else {
                break;
            };

            corrections_applied.extend(apply_targeted_correction(output, &axis));
            if axis != ScoreAxis::TaskEssential {
                stage4.run(output).await;
                stage5.run(output);
                // Re-apply constraint/deliverable merge after Stage 4 so locks aren't overwritten
                crate::correction::cycle::merge_reconstruction_into_schema(output);
            }

            let mut new_response = stage6a.run(output)?;
            let mut new_report =
                crate::npae::hallucination::guard::run_tri_layer(&new_response, input, guard_cfg)
                    .unwrap_or_else(|_| crate::npae::hallucination::guard::HallucinationReport {
                        passed: true,
                        layers: [
                            crate::npae::hallucination::guard::LayerReport {
                                layer_id: 1,
                                passed: true,
                                flags: vec![],
                            },
                            crate::npae::hallucination::guard::LayerReport {
                                layer_id: 2,
                                passed: true,
                                flags: vec![],
                            },
                            crate::npae::hallucination::guard::LayerReport {
                                layer_id: 3,
                                passed: true,
                                flags: vec![],
                            },
                        ],
                        remediation: None,
                    });

            // ARCH: Balanced Mode Contract (E5) — remediate hallucination flags inside Stage 6B loop.
            // FIX: Regenerated responses inside Stage 6B were previously never remediated when `!new_report.passed`.
            if !new_report.passed {
                for (layer_idx, layer) in new_report.layers.iter().enumerate() {
                    if !layer.passed {
                        for flag in &layer.flags {
                            corrections_applied.push(TextCorrection {
                                original: flag.clone(),
                                corrected: format!("[CORRECTED] {}", flag),
                                confidence: 0.9,
                                correction_type: format!("l{}_guard_flag", layer_idx + 1),
                            });
                        }
                    }
                }
                new_response = crate::npae::hallucination::guard::remediate_hallucination(
                    &new_response,
                    &new_report,
                );
                if let Ok(rechecked) = crate::npae::hallucination::guard::run_tri_layer(
                    &new_response,
                    input,
                    guard_cfg,
                ) {
                    new_report = rechecked;
                }
            }

            final_response = new_response;
            guard_report = new_report;
            cycle_num += 1;
        }

        // Restore best candidate if the final cycle degraded quality or expanded tokens
        if let Some(best) = best_candidate {
            final_response = best.response;
            guard_report = best.guard_report;
            field_issues = best.field_issues;
            scoring_result = best.scoring_result;
            output.resolved_schema = best.resolved_schema;
            output.force_compact_generation = best.force_compact;
            output.output_token_count = best.output_tokens;
            output.rendered_output = Some(final_response.clone());
            output.field_issues = field_issues.clone();
            output.scoring_result = Some(scoring_result.clone());
        }

        correction_cycle
            .corrections_applied
            .extend(corrections_applied.clone());
        output.correction_cycle = Some(correction_cycle.clone());

        Ok(Stage6bOutput {
            final_response,
            guard_report,
            corrections_applied,
            field_issues,
            scoring_result,
            correction_cycle,
        })
    }
}
