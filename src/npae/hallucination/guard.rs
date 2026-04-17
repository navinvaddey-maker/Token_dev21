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
