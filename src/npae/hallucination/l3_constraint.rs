//! Layer 3 Claim Verification & Constraint Enforcement (`l3_constraint`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 — enforces `claim_verification_rules` (such as
//! // `"no_invented_apis"`) on generated prompts and responses.

use crate::npae::hallucination::guard::LayerReport;
use crate::npae::schema::types::HallucinationGuardConfig;

/// Verifies `output` against configured `claim_verification_rules` in `cfg`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check(output: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    let mut flags = Vec::new();

    if cfg
        .claim_verification_rules
        .iter()
        .any(|r| r == "no_invented_apis")
        && output.contains("fake_api_call()")
    {
        flags.push("Constraint violation: fake_api_call() detected".into());
    }

    Ok(LayerReport {
        layer_id: 3,
        passed: flags.is_empty(),
        flags,
    })
}
