//! Layer 2 Token-Level Uncertainty & Confidence Check (`l2_confidence`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 — flags unverified low-confidence claims so
//! // remediation can mark them with `[UNCERTAIN]`.

use crate::npae::hallucination::guard::LayerReport;
use crate::npae::hallucination::l1_critique::extract_non_task_scaffolding;
use crate::npae::schema::types::HallucinationGuardConfig;

/// Checks `output` for unhedged absolute claims or ungrounded numeric claims against `original_prompt`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check_with_source(
    output: &str,
    original_prompt: &str,
    cfg: &HallucinationGuardConfig,
) -> Result<LayerReport, String> {
    let mut flags = Vec::new();
    let lower = output.to_lowercase();
    let has_marker = cfg
        .uncertainty_markers
        .iter()
        .any(|m| output.contains(m.as_str()));

    if !has_marker
        && (lower.contains("guaranteed 100%")
            || lower.contains("100% certain")
            || lower.contains("zero risk"))
    {
        flags
            .push("low_confidence: unhedged absolute claim without uncertainty marker".to_string());
    }

    // AUD-09: Detect quantified claims (currency, percentages, or numeric metrics) in generated
    // scaffolding that are absent from `original_prompt` and lack an uncertainty marker.
    if !has_marker {
        let prompt_lower = original_prompt.to_lowercase();
        let target_text =
            extract_non_task_scaffolding(output).unwrap_or_else(|| output.to_string());
        for raw_tok in target_text.split_whitespace() {
            let tok = raw_tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '%' && c != '$');
            let is_quantified = (tok.starts_with('$') && tok.chars().any(|c| c.is_ascii_digit()))
                || (tok.ends_with('%') && tok.chars().any(|c| c.is_ascii_digit()));
            if is_quantified && !prompt_lower.contains(&tok.to_lowercase()) {
                flags.push(format!(
                    "low_confidence: ungrounded quantified claim '{tok}' without source support"
                ));
            }
        }
    }

    Ok(LayerReport {
        layer_id: 2,
        passed: flags.is_empty(),
        flags,
    })
}

/// Checks `output` for low-confidence or unverified claims against `cfg`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check(output: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    check_with_source(output, "", cfg)
}
