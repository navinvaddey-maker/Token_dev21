use crate::npae::schema::types::HallucinationGuardConfig;
use crate::npae::hallucination::guard::LayerReport;

pub fn check(output: &str, original_prompt: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    if !cfg.self_critique_enabled {
        return Ok(LayerReport { layer_id: 1, passed: true, flags: vec![] });
    }

    let mut flags = Vec::new();

    // Mock simple factual contradiction evaluation loop
    // Check if output specifically contradicts the prompt's constraints
    if original_prompt.contains("no json") && output.contains("{") {
        flags.push("Contradiction: Prompt forbids JSON but output contains JSON syntax".into());
    }

    Ok(LayerReport {
        layer_id: 1,
        passed: flags.is_empty(),
        flags,
    })
}
