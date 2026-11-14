//! Quality Scoring Module (`ARCHITECTURE.md` §1.3, §4).
//!
//! Coordinates independent evaluation across TES (Token Efficiency), SFS (Schema Fidelity),
//! and SCS (Semantic Completeness) and identifies the lowest failing axis for Stage 6B.

pub mod scs;
pub mod sfs;
pub mod tes;
pub mod weights;

use crate::types::{AlgorithmOutput, FieldValidationIssue, ScoreAxis, ScoringResult};

/// Computes the complete `ScoringResult` by evaluating TES, SFS, and SCS independently.
///
/// # Panics
/// Never.
#[must_use]
pub fn compute_scoring_result(
    output: &AlgorithmOutput,
    issues: &[FieldValidationIssue],
) -> ScoringResult {
    // ARCH: Balanced Mode Contract (E1, E2, E3) — all three scorers evaluate the active output.
    let (tes, _) = tes::TokenEfficiencyScorer::score(output, issues);
    let (sfs, _) = sfs::SemanticFidelityScorer::score_with_output(output, issues);
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
        let _lowest = scs;
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
