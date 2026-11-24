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
        // `# TASK / INSTRUCTION:` may appear near the top (followed by `\n# <NEXT SECTION>`)
        // or near the bottom (followed by `\n\n---\n`); take whichever delimiter comes first.
        let next_header = rest.find("\n# ");
        let next_rule = rest.find("\n\n---\n");
        let end_idx = match (next_header, next_rule) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let suffix = end_idx.map_or("", |idx| &rest[idx..]);
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

    // FIX: Medium #1 — for `# REPAIR DIRECTIVE` prompts, exclude the diagnostic
    // `# FAILED / MISSING ITEMS TO REPAIR:` section so caller failure metrics (e.g. `50%`) or
    // URLs inside diagnostics do not trigger false L2/L3 `[UNCERTAIN]` prefixes.
    if let Some(diag_idx) = output.find("# FAILED / MISSING ITEMS TO REPAIR:") {
        let prefix = &output[..diag_idx];
        let rest = &output[diag_idx + "# FAILED / MISSING ITEMS TO REPAIR:".len()..];
        let suffix = rest.find("\n# ").map_or("", |idx| &rest[idx..]);
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
    let prompt_lower = original_prompt.to_lowercase();
    let prompt_forbids_json =
        prompt_lower.contains("no json") || prompt_lower.contains("without json");
    let prompt_forbids_markdown =
        prompt_lower.contains("no markdown") || prompt_lower.contains("plain text only");

    // FIX: G-09 — when `output` is a structured Aggressive prompt (`# TASK / INSTRUCTION:` or
    // `<task>...</task>`), inspect only the non-task scaffolding (or an explicit `Output Format:`
    // directive) so user-quoted snippets inside the task are never falsely flagged.
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

    if cfg.contradiction_check && prompt_forbids_markdown {
        let has_md_contradiction = if let Some(scaffolding) = extract_non_task_scaffolding(output) {
            let scaffolding_lower = scaffolding.to_lowercase();
            scaffolding_lower.contains("output format: markdown")
        } else {
            output.contains("```")
        };

        if has_md_contradiction {
            flags.push(
                "Contradiction: Prompt forbids markdown but output specifies markdown".into(),
            );
        }
    }

    Ok(LayerReport {
        layer_id: 1,
        passed: flags.is_empty(),
        flags,
    })
}
