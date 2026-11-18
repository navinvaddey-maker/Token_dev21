//! Layer 1 Self-Critique & Contradiction Check (`l1_critique`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 (G-09) — evaluates whether engine-generated
//! // scaffolding contradicts user format/constraint directives without flagging or mutating
//! // user-supplied task examples.

use crate::npae::hallucination::guard::LayerReport;
use crate::npae::schema::types::HallucinationGuardConfig;

/// Extracts the non-task scaffolding from a structured Aggressive prompt (`Crisp` or `Harness`).
///
/// Returns `None` if `output` is not a structured Aggressive prompt (i.e. lacks both
/// `"# TASK / INSTRUCTION:"` and `"<task>"`), so raw string tests operate on the full string.
///
/// # Panics
/// Never.
#[must_use]
pub fn extract_non_task_scaffolding(output: &str) -> Option<String> {
    if let Some(task_idx) = output.find("# TASK / INSTRUCTION:") {
        let prefix = &output[..task_idx];
        let rest = &output[task_idx + "# TASK / INSTRUCTION:".len()..];
        let suffix = rest
            .find("\n\n---\n")
            .map_or("", |end_idx| &rest[end_idx..]);
        return Some(format!("{prefix}\n{suffix}"));
    }

    if let Some(task_start) = output.find("<task>") {
        let prefix = &output[..task_start];
        let rest = &output[task_start + "<task>".len()..];
        let suffix = rest
            .find("</task>")
            .map_or("", |end_idx| &rest[end_idx + "</task>".len()..]);
        return Some(format!("{prefix}\n{suffix}"));
    }

    None
}

/// Checks `output` for logical or format contradictions against `original_prompt`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check(
    output: &str,
    original_prompt: &str,
    cfg: &HallucinationGuardConfig,
) -> Result<LayerReport, String> {
    if !cfg.self_critique_enabled {
        return Ok(LayerReport {
            layer_id: 1,
            passed: true,
            flags: vec![],
        });
    }

    let mut flags = Vec::new();
    let prompt_forbids_json = original_prompt.to_lowercase().contains("no json");

    // FIX: G-09 — when `output` is a structured Aggressive prompt (`# TASK / INSTRUCTION:` or
    // `<task>...</task>`), inspect only the non-task scaffolding (or an explicit `Output Format: json`
    // directive) so user-quoted JSON snippets inside the task are never falsely flagged.
    if prompt_forbids_json {
        let has_json_contradiction = if let Some(scaffolding) = extract_non_task_scaffolding(output)
        {
            let scaffolding_lower = scaffolding.to_lowercase();
            scaffolding.contains('{') || scaffolding_lower.contains("output format: json")
        } else {
            output.contains('{')
        };

        if has_json_contradiction {
            flags.push("Contradiction: Prompt forbids JSON but output contains JSON syntax".into());
        }
    }

    Ok(LayerReport {
        layer_id: 1,
        passed: flags.is_empty(),
        flags,
    })
}
