//! Layer 3 Claim Verification & Constraint Enforcement (`l3_constraint`).
//!
//! // ARCH: ARCHITECTURE.md §1.2, §1.3 & §7.2 — enforces `claim_verification_rules` (such as
//! // `"no_invented_apis"`) on generated prompts and responses.

use crate::npae::hallucination::guard::LayerReport;
use crate::npae::hallucination::l1_critique::extract_non_task_scaffolding;
use crate::npae::schema::types::HallucinationGuardConfig;

const NEGATION_TRIGGERS: &[&str] = &["no", "without", "never", "avoid", "exclude"];

/// Extracts simple negative constraint pairs (`"<trigger> <target>"`) from `prompt`.
fn extract_negative_targets(prompt: &str) -> Vec<(String, String)> {
    let words: Vec<String> = prompt
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    let mut pairs = Vec::new();
    for i in 0..words.len() {
        let trig = words[i].as_str();
        if NEGATION_TRIGGERS.contains(&trig) {
            if let Some(next) = words.get(i + 1) {
                if next.len() >= 3 && !NEGATION_TRIGGERS.contains(&next.as_str()) {
                    pairs.push((trig.to_string(), next.clone()));
                }
            }
        } else if trig == "must" && words.get(i + 1).map(String::as_str) == Some("not") {
            if let Some(next) = words.get(i + 2) {
                if next.len() >= 3 {
                    pairs.push(("must not".to_string(), next.clone()));
                }
            }
        }
    }
    pairs
}

/// Returns `true` when `haystack` contains `pattern` bounded on both sides by non-alphanumeric chars.
fn contains_bounded_phrase(haystack: &str, pattern: &str) -> bool {
    let mut start = 0usize;
    while let Some(rel_idx) = haystack[start..].find(pattern) {
        let abs_start = start + rel_idx;
        let abs_end = abs_start + pattern.len();
        let left_ok = haystack[..abs_start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let right_ok = haystack[abs_end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if left_ok && right_ok {
            return true;
        }
        start = abs_end;
    }
    false
}

/// Returns `true` when an explicit override or revocation in `prompt_lower` supersedes
/// the earlier negative constraint `"{trig} {target}"`.
fn is_negation_overridden(prompt_lower: &str, trig: &str, target: &str) -> bool {
    let phrase = format!("{trig} {target}");
    // FIX: Medium #12 — check both `{trig} {target}` and bare `{target}` override/revocation forms.
    let revoke_patterns = [
        format!("allow {target}"),
        format!("allow {phrase}"),
        format!("remove {target}"),
        format!("remove {phrase}"),
        format!("drop {target}"),
        format!("drop {phrase}"),
        format!("revoke {target}"),
        format!("revoke {phrase}"),
        format!("replace {target} with"),
        format!("replace {phrase} with"),
        format!("instead of {target}"),
        format!("instead of {phrase}"),
        format!("rather than {target}"),
        format!("rather than {phrase}"),
        format!("switch from {target} to"),
        format!("switch from {phrase} to"),
        format!("change {target} to"),
        format!("change {phrase} to"),
        format!("no longer {target}"),
        format!("no longer {phrase}"),
    ];
    revoke_patterns
        .iter()
        .any(|p| contains_bounded_phrase(prompt_lower, p))
}

/// Verifies `output` against configured `claim_verification_rules` and `original_prompt` exclusions.
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
    let out_lower = output.to_lowercase();
    let prompt_lower = original_prompt.to_lowercase();

    // AUD-09: Verify negative/exclusion constraints from `original_prompt` are preserved in `output`.
    if !original_prompt.trim().is_empty() {
        for (trig, target) in extract_negative_targets(original_prompt) {
            if is_negation_overridden(&prompt_lower, &trig, &target) {
                continue;
            }
            // Skip if `output` is a very short unrelated stub that doesn't mention the domain,
            // UNLESS the target appears without its negation or the constraint was dropped.
            let has_negation = out_lower.contains(&format!("{trig} {target}"))
                || out_lower.contains("not ")
                || out_lower.contains("no ")
                || out_lower.contains("without ")
                || out_lower.contains("never ")
                || out_lower.contains("avoid ")
                || out_lower.contains("exclude ");

            if !has_negation && out_lower.contains(&target) {
                flags.push(format!(
                    "Constraint violation: negation dropped for '{trig} {target}'"
                ));
            }
        }
    }

    // AUD-09: Under `no_invented_apis`, reject ungrounded URLs or function-call syntax
    // introduced in generated scaffolding when absent from `original_prompt`.
    if cfg
        .claim_verification_rules
        .iter()
        .any(|r| r == "no_invented_apis")
    {
        let target_text =
            extract_non_task_scaffolding(output).unwrap_or_else(|| output.to_string());
        for tok in target_text.split_whitespace() {
            let lower_tok = tok.to_lowercase();
            let looks_like_url =
                lower_tok.starts_with("http://") || lower_tok.starts_with("https://");
            let looks_like_call = lower_tok.ends_with("()") && lower_tok.len() > 2;
            if (looks_like_url || looks_like_call) && !prompt_lower.contains(&lower_tok) {
                flags.push(format!(
                    "Constraint violation: ungrounded API/URL '{tok}' not present in prompt"
                ));
            }
        }
    }

    Ok(LayerReport {
        layer_id: 3,
        passed: flags.is_empty(),
        flags,
    })
}

/// Verifies `output` against configured `claim_verification_rules` in `cfg`.
///
/// # Errors
/// Never returns `Err` under current deterministic rules.
///
/// # Panics
/// Never.
pub fn check(output: &str, cfg: &HallucinationGuardConfig) -> Result<LayerReport, String> {
    check_with_source(output, "", cfg)
}
