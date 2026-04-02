pub mod sfs;
pub mod tes;
pub mod weights;

use crate::types::{AlgorithmOutput, DualScore, FieldValidationIssue};

/// Computes the complete DualScore by evaluating TES and SFS.
///
/// Weighting: TES matters less (0.45) than Semantic Fidelity (0.55).
///
/// @param output - Algorithm output context.
/// @param issues - Field validation issues.
/// @returns DualScore containing individual scores and weighted overall.
pub fn compute_dual_score(output: &AlgorithmOutput, issues: &[FieldValidationIssue]) -> DualScore {
    let (tes, _) = tes::TokenEfficiencyScorer::score(output, issues);
    let (sfs, _) = sfs::SemanticFidelityScorer::score(issues);
    
    let overall = (tes * 0.45 + sfs * 0.55).clamp(0.0, 10.0);
    
    DualScore { tes, sfs, overall }
}
