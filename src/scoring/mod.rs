pub mod scs;
pub mod sfs;
pub mod tes;
pub mod weights;

use crate::types::{AlgorithmOutput, FieldValidationIssue, ScoreAxis, ScoringResult};

/// Computes the complete ScoringResult by evaluating TES, SFS, and SCS.
///
/// Weighting: No aggregate. Each axis evaluated independently.
///
/// @param output - Algorithm output context.
/// @param issues - Field validation issues.
/// @returns ScoringResult containing independent scores and correction trigger.
pub fn compute_scoring_result(
    output: &AlgorithmOutput,
    issues: &[FieldValidationIssue],
) -> ScoringResult {
    let (tes, _) = tes::TokenEfficiencyScorer::score(output, issues);
    let (sfs, _) = sfs::SemanticFidelityScorer::score(issues);
    let (scs, _) = scs::SemanticCompletenessScorer::score(output);

    let threshold = 6.0;
    let mut correction_needed = false;
    let mut lowest = 10.0;
    let mut correction_axis = None;

    if tes < threshold && tes < lowest {
        lowest = tes;
        correction_needed = true;
        correction_axis = Some(ScoreAxis::TaskEssential);
    }
    if sfs < threshold && sfs < lowest {
        lowest = sfs;
        correction_needed = true;
        correction_axis = Some(ScoreAxis::SchemaFidelity);
    }
    if scs < threshold && scs < lowest {
        correction_needed = true;
        correction_axis = Some(ScoreAxis::SemanticCompleteness);
    }

    ScoringResult {
        tes,
        sfs,
        scs,
        correction_needed,
        correction_axis,
    }
}
