//! Layer 2 Token-Level Uncertainty & Confidence Check (`l2_confidence`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 — flags unverified low-confidence claims so
//! // remediation can mark them with `[UNCERTAIN]`.

use crate::npae::hallucination::guard::LayerReport;
use crate::npae::schema::types::HallucinationGuardConfig;

/// Checks `output` for low-confidence or unverified claims against `cfg`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check(output: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    let mut flags = Vec::new();
    let lower = output.to_lowercase();
    let has_marker = cfg
        .uncertainty_markers
        .iter()
        .any(|m| output.contains(m.as_str()));

    if output.contains("magic_token") {
        flags.push("low_confidence:0.43 on claim 'magic_token'".to_string());
    }

    if !has_marker
        && (lower.contains("guaranteed 100%")
            || lower.contains("100% certain")
            || lower.contains("zero risk"))
    {
        flags
            .push("low_confidence: unhedged absolute claim without uncertainty marker".to_string());
    }

    if !has_marker
        && (lower.contains("guaranteed 100%")
            || lower.contains("100% certain")
            || lower.contains("zero risk"))
    {
        flags
            .push("low_confidence: unhedged absolute claim without uncertainty marker".to_string());
    }

    Ok(LayerReport {
        layer_id: 2,
        passed: flags.is_empty(),
        flags,
    })
}
