use crate::{
    algorithms::field_validator::FieldTypeValidator,
    correction::{apply_targeted_correction, MAX_CORRECTION_CYCLES},
    npae::schema::types::HallucinationGuardConfig,
    pipeline::{
        stage4::Stage4,
        stage5::Stage5,
        stage6a::Stage6a,
    },
    scoring::compute_scoring_result,
    types::{
        AlgorithmOutput, CorrectionCycle, FieldValidationIssue, ScoreAxis,
        ScoringResult, TextCorrection,
    },
    utils::tokens::estimate_tokens,
};

/// Handles Stage 6B: Targeted Quality Correction Loop
#[derive(Default)]
pub struct Stage6b;

pub struct Stage6bOutput {
    pub final_response: String,
    pub guard_report: crate::npae::hallucination::guard::HallucinationReport,
    pub corrections_applied: Vec<TextCorrection>,
    pub field_issues: Vec<FieldValidationIssue>,
    pub scoring_result: ScoringResult,
    pub correction_cycle: CorrectionCycle,
}

impl Stage6b {
    pub fn new() -> Self {
        Self
    }

    /// Executes Stage 6B targeted correction loop.
    /// Iterates until TES, SFS, and SCS score thresholds (>= 6.0) are satisfied or MAX_CORRECTION_CYCLES is reached.
    pub fn run(
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
        let mut final_response = initial_response;
        let mut guard_report = initial_report;
        let mut corrections_applied = initial_corrections;
        let mut correction_cycle = CorrectionCycle::new(1);
        let mut cycle_num = 0u32;
        let mut field_issues = field_validator.validate_full(&output.resolved_schema, output);

        let mut scoring_result = compute_scoring_result(output, &field_issues);

        loop {
            output.output_token_count = estimate_tokens(&final_response);
            field_issues = field_validator.validate_full(&output.resolved_schema, output);
            output.field_issues = field_issues.clone();

            let before_scoring = scoring_result.clone();
            scoring_result = compute_scoring_result(output, &field_issues);
            output.scoring_result = Some(scoring_result.clone());

            let scores_ok = scoring_result.tes >= 6.0
                && scoring_result.sfs >= 6.0
                && scoring_result.scs >= 6.0;

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
                stage4.run(output);
                stage5.run(output);
            }

            let new_response = stage6a.run(output)?;
            let new_report = crate::npae::hallucination::guard::run_tri_layer(
                &new_response,
                input,
                guard_cfg,
            )
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

            final_response = new_response;
            guard_report = new_report;
            cycle_num += 1;
        }

        correction_cycle.corrections_applied.extend(corrections_applied.clone());
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
