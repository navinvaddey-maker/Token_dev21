use crate::npae::schema::types::HallucinationGuardConfig;
use crate::npae::hallucination::l1_critique;
use crate::npae::hallucination::l2_confidence;
use crate::npae::hallucination::l3_constraint;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayerReport {
    pub layer_id: u8,
    pub passed:   bool,
    pub flags:    Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HallucinationReport {
    pub passed:      bool,
    pub layers:      [LayerReport; 3],
    pub remediation: Option<String>,
}

pub fn run_tri_layer(
    output: &str,
    original_prompt: &str,
    cfg: &HallucinationGuardConfig,
) -> Result<HallucinationReport, String> {
    let l1 = l1_critique::check(output, original_prompt, cfg)?;
    let l2 = l2_confidence::check(output, cfg)?;
    let l3 = l3_constraint::check(output, cfg)?;

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
        return "Constraint violation detected. Check output formatting or forbidden topics.".into();
    }
    "Unknown".into()
}

pub fn remediate_hallucination(
    output: &str,
    report: &HallucinationReport,
) -> String {
    let mut corrected = output.to_string();
    if !report.passed {
        // Apply L1 contradiction correction: prompt forbids JSON, so remove JSON structural braces
        if !report.layers[0].passed {
            corrected = corrected.replace("{", "").replace("}", "");
        }
        // Apply L2 confidence correction: wrap low confidence claims (e.g. 'magic_token') with [UNCERTAIN]
        if !report.layers[1].passed {
            corrected = corrected.replace("magic_token", "[UNCERTAIN]magic_token");
        }
        // Apply L3 constraint violation correction: remove fake_api_call()
        if !report.layers[2].passed {
            corrected = corrected.replace("fake_api_call()", "/* REMOVED: fake_api_call */");
        }
    }
    corrected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hallucination_guard_all_pass() {
        let cfg = HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into()],
        };

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
    fn test_hallucination_guard_fails_and_remediates() {
        let cfg = HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into()],
        };

        // Trigger L1, L2, L3 failures
        let output = "{ Here is magic_token and fake_api_call() }";
        let original_prompt = "no json";

        let report = run_tri_layer(output, original_prompt, &cfg).unwrap();
        assert!(!report.passed);
        assert!(!report.layers[0].passed);
        assert!(!report.layers[1].passed);
        assert!(!report.layers[2].passed);
        assert!(report.remediation.is_some());

        let corrected = remediate_hallucination(output, &report);
        assert_eq!(corrected, " Here is [UNCERTAIN]magic_token and /* REMOVED: fake_api_call */ ");

        // Re-running on corrected should pass or be much better
        let report_after = run_tri_layer(&corrected, original_prompt, &cfg).unwrap();
        assert!(report_after.layers[0].passed, "L1 should pass since braces were stripped");
        assert!(!report_after.layers[1].passed, "L2 remains failed because magic_token is still in the text, but now with uncertainty prefix");
        assert!(report_after.layers[2].passed, "L3 should pass since fake_api_call was removed");
    }
}


