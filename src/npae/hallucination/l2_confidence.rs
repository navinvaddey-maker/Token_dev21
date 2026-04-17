use crate::npae::schema::types::HallucinationGuardConfig;
use crate::npae::hallucination::guard::LayerReport;

pub fn check(output: &str, _cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    let mut flags = Vec::new();

    // Mock token-level uncertainty scoring wrapper logic
    // We simulate failure if output contains specific unverified keywords
    if output.contains("magic_token") {
        flags.push(format!("low_confidence:0.43 on claim 'magic_token'"));
    }

    Ok(LayerReport {
        layer_id: 2,
        passed: flags.is_empty(),
        flags,
    })
}
