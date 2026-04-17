use crate::npae::schema::types::HallucinationGuardConfig;
use crate::npae::hallucination::guard::LayerReport;

pub fn check(output: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    let mut flags = Vec::new();

    if cfg.claim_verification_rules.contains(&"no_invented_apis".to_string()) {
        if output.contains("fake_api_call()") {
            flags.push("Constraint violation: fake_api_call() detected".into());
        }
    }

    Ok(LayerReport {
        layer_id: 3,
        passed: flags.is_empty(),
        flags,
    })
}
