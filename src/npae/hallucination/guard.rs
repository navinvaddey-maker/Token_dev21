//! Tri-Layer Hallucination Guard & Deterministic Remediation (`guard`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 (G-09) — orchestrates L1 (self-critique),
//! // L2 (confidence), and L3 (constraint) checks and applies targeted remediation while
//! // preserving verbatim user task blocks (`# TASK / INSTRUCTION:` and `<task>...</task>`).

use crate::npae::hallucination::l1_critique;
use crate::npae::hallucination::l2_confidence;
use crate::npae::hallucination::l3_constraint;
use crate::npae::schema::types::HallucinationGuardConfig;
use serde::{Deserialize, Serialize};

/// Diagnostic outcome for a single layer (`1`, `2`, or `3`) of `HallucinationGuard`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayerReport {
    /// Layer identifier (`1`, `2`, or `3`).
    pub layer_id: u8,
    /// `true` iff zero flags were raised by this layer.
    pub passed: bool,
    /// Human-readable violation flags raised by this layer.
    pub flags: Vec<String>,
}

/// Combined tri-layer verification report returned by [`run_tri_layer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HallucinationReport {
    /// `true` iff all 3 layers passed.
    pub passed: bool,
    /// Per-layer diagnostic reports (`[L1, L2, L3]`).
    pub layers: [LayerReport; 3],
    /// Summary remediation guidance when `passed == false`.
    pub remediation: Option<String>,
}

/// Runs all three hallucination guard layers (`L1`, `L2`, `L3`) on `output`.
///
/// # Errors
/// Returns `Err` if any underlying layer check fails.
///
/// # Panics
/// Never.
pub fn run_tri_layer(
    output: &str,
    original_prompt: &str,
    cfg: &HallucinationGuardConfig,
) -> Result<HallucinationReport, String> {
    let l1 = l1_critique::check(output, original_prompt, cfg)?;
    let l2 = l2_confidence::check_with_source(output, original_prompt, cfg)?;
    let l3 = l3_constraint::check_with_source(output, original_prompt, cfg)?;

    let passed = l1.passed && l2.passed && l3.passed;

    let remediation = if !passed {
        Some(derive_remediation(&l1, &l2, &l3))
    } else {
        None
    };

    Ok(HallucinationReport {
        passed,
        layers: [l1, l2, l3],
        remediation,
    })
}

fn derive_remediation(l1: &LayerReport, l2: &LayerReport, l3: &LayerReport) -> String {
    if !l1.passed {
        return "Contradiction detected in claims. Re-evaluate logical consistency.".into();
    }
    if !l2.passed {
        return "Low confidence on certain claims. Wrap with [UNCERTAIN] marker.".into();
    }
    if !l3.passed {
        return "Constraint violation detected. Check output formatting or forbidden topics."
            .into();
    }
    "Unknown".into()
}

/// Strips `{` and `}` from non-task scaffolding only, preserving the user's verbatim task text.
fn strip_braces_outside_task(output: &str) -> String {
    if let Some(task_idx) = output.find("# TASK / INSTRUCTION:") {
        let prefix = output[..task_idx]
            .replace(['{', '}'], "")
            .replace("Output Format: json", "Output Format: markdown")
            .replace("Output Format: JSON", "Output Format: markdown");
        let rest = &output[task_idx..];
        if let Some(delim_idx) = rest.find("\n\n---\n") {
            let task_part = &rest[..delim_idx];
            let suffix = rest[delim_idx..].replace(['{', '}'], "");
            return format!("{prefix}{task_part}{suffix}");
        }
        return format!("{prefix}{rest}");
    }

    if let Some(task_start) = output.find("<task>") {
        let prefix = output[..task_start]
            .replace(['{', '}'], "")
            .replace("Output Format: json", "Output Format: markdown")
            .replace("Output Format: JSON", "Output Format: markdown");
        let rest = &output[task_start..];
        if let Some(end_idx) = rest.find("</task>") {
            let split_at = end_idx + "</task>".len();
            let task_part = &rest[..split_at];
            let suffix = rest[split_at..].replace(['{', '}'], "");
            return format!("{prefix}{task_part}{suffix}");
        }
        return format!("{prefix}{rest}");
    }

    output.replace(['{', '}'], "")
}

/// Applies deterministic remediation to `output` for any failed layer in `report`.
///
/// // FIX: G-09 — never strips `{` or `}` from inside the user's own `# TASK / INSTRUCTION:` or
/// // `<task>...</task>` block when remediating an L1 `"no json"` contradiction.
///
/// # Panics
/// Never.
pub fn remediate_hallucination(output: &str, report: &HallucinationReport) -> String {
    let mut corrected = output.to_string();
    if !report.passed {
        // Apply L1 contradiction correction: remove JSON structural braces outside user task text,
        // and neutralize explicit markdown directives when markdown is forbidden.
        if !report.layers[0].passed {
            corrected = strip_braces_outside_task(&corrected);
            if report.layers[0]
                .flags
                .iter()
                .any(|f| f.contains("forbids markdown"))
            {
                corrected = corrected
                    .replace("Output Format: markdown", "Output Format: plain text")
                    .replace("Output Format: Markdown", "Output Format: plain text");
            }
        }
        // Apply L2 confidence correction: mark unverified quantified/absolute claims with `[UNCERTAIN]`.
        if !report.layers[1].passed && !corrected.contains("[UNCERTAIN]") {
            corrected = format!("[UNCERTAIN] {corrected}");
        }
        // Apply L3 constraint violation correction: strip ungrounded API/URL tokens and restore
        // any dropped exclusion constraints from flag metadata.
        if !report.layers[2].passed {
            for flag in &report.layers[2].flags {
                if let Some(rest) = flag.strip_prefix("Constraint violation: ungrounded API/URL '")
                {
                    if let Some((tok, _)) = rest.split_once('\'') {
                        corrected = corrected.replace(tok, "[REMOVED_UNGROUNDED_REF]");
                    }
                } else if let Some(rest) =
                    flag.strip_prefix("Constraint violation: negation dropped for '")
                {
                    if let Some((constraint, _)) = rest.split_once('\'') {
                        corrected.push_str(&format!("\n# CONSTRAINTS:\n- {constraint}"));
                    }
                }
            }
        }
    }
    corrected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cfg() -> HallucinationGuardConfig {
        HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into()],
        }
    }

    #[test]
    fn test_hallucination_guard_all_pass() {
        let cfg = test_cfg();

        let report = run_tri_layer("This output has no issues.", "no json", &cfg).unwrap();
        assert!(report.passed);
        assert!(report.layers[0].passed);
        assert!(report.layers[1].passed);
        assert!(report.layers[2].passed);
        assert!(report.remediation.is_none());

        let corrected = remediate_hallucination("This output has no issues.", &report);
        assert_eq!(corrected, "This output has no issues.");
    }

    #[test]
    fn test_hallucination_guard_fails_and_remediates_semantically() {
        let cfg = test_cfg();

        // Trigger L1 (JSON contradiction), L2 (invented percentage), and L3 (invented API + dropped negation)
        let output = "{ Guaranteeing 95% savings via helper_endpoint() and including dairy }";
        let original_prompt = "Build a meal plan with no json and no dairy";

        let report = run_tri_layer(output, original_prompt, &cfg).unwrap();
        assert!(!report.passed);
        assert!(!report.layers[0].passed);
        assert!(!report.layers[1].passed);
        assert!(!report.layers[2].passed);
        assert!(report.remediation.is_some());

        let corrected = remediate_hallucination(output, &report);
        assert!(corrected.contains("[UNCERTAIN]"));
        assert!(corrected.contains("[REMOVED_UNGROUNDED_REF]"));
        assert!(corrected.contains("no dairy"));

        let report_after = run_tri_layer(&corrected, original_prompt, &cfg).unwrap();
        assert!(
            report_after.layers[0].passed,
            "L1 should pass after brace strip"
        );
        assert!(
            report_after.layers[1].passed,
            "L2 should pass once [UNCERTAIN] marker is applied"
        );
        assert!(
            report_after.layers[2].passed,
            "L3 should pass once ungrounded ref is removed and negation restored"
        );
    }

    #[test]
    fn test_hallucination_guard_preserves_user_task_braces() {
        let cfg = test_cfg();
        let output =
            "# ROLE: Engineer\n# TASK / INSTRUCTION:\nParse `{key: value}` with no json output\n";
        let report =
            run_tri_layer(output, "Parse `{key: value}` with no json output", &cfg).unwrap();
        assert!(report.layers[0].passed);
    }
}
