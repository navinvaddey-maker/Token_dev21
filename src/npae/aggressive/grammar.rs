use std::sync::Arc;
use lazy_static::lazy_static;
use regex::Regex;
use harper_core::linting::{Linter, LintGroup, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Document, Dialect};
use crate::types::TextCorrection;


lazy_static! {
    /// Regex pattern to detect numbers, decimals, percentages, currencies, dates, and units that MUST NEVER BE TOUCHED
    static ref PROTECTED_RE: Regex = Regex::new(
        r"(?x)
        (
            \$[0-9]+(?:\.[0-9]+)?[kKmMbBtT]?             | # Currency USD ($12M, $5.5k)
            ₹[0-9]+(?:\.[0-9]+)?[kKmLcC]?             | # Currency INR (₹5L, ₹10k)
            €[0-9]+(?:\.[0-9]+)?[kKmMbBtT]?             | # Currency EUR (€100)
            £[0-9]+(?:\.[0-9]+)?[kKmMbBtT]?             | # Currency GBP (£50)
            \b[0-9]{4}-[0-9]{2}-[0-9]{2}\b             | # ISO Date (2026-10-03)
            \bQ[1-4]\b                                 | # Quarter (Q3)
            \b[0-9]+(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\b | # Date (3rd Oct)
            \b[0-9]+-(?:month|year|day|week|hr|min)\b   | # Duration unit (18-month)
            \b[0-9]+(?:\.[0-9]+)?(?:%|kg|g|mg|m|cm|mm|km|MB|GB|TB|kbps|Mbps|Gbps|x|x)\b | # Quantities/Units
            \b[0-9]+(?:\.[0-9]+)?\b                      # Standard isolated numbers / floats
        )
        "
    ).unwrap();
}

pub struct GrammarCorrector;

impl GrammarCorrector {
    /// Corrects spelling and grammatical errors in a prompt while strictly preserving numbers, dates, currency, and units.
    /// Also enforces a 30% maximum edit distance safety threshold.
    pub fn correct(raw_prompt: &str) -> (String, Vec<TextCorrection>) {
        let trimmed = raw_prompt.trim();
        if trimmed.is_empty() {
            return (raw_prompt.to_string(), vec![]);
        }

        // 1. Mask protected patterns with unique placeholders
        let mut masked = raw_prompt.to_string();
        let mut placeholders: Vec<(String, String)> = Vec::new();
        let mut idx = 0;

        for cap in PROTECTED_RE.captures_iter(raw_prompt) {
            let mat = cap.get(0).unwrap().as_str();
            let key = format!("__NUM_PROT_{}__", idx);
            idx += 1;
            masked = masked.replacen(mat, &key, 1);
            placeholders.push((key, mat.to_string()));
        }

        // 2. Parse text with harper_core Document and Linter
        let dict = Arc::new(FstDictionary::curated());

        let mut lint_group = LintGroup::new_curated(dict.clone(), Dialect::American);
        let doc = Document::new_plain_english(&masked, dict.as_ref());
        let lints = lint_group.lint(&doc);

        let mut corrections = Vec::new();
        let mut chars: Vec<char> = masked.chars().collect();

        // Sort lints in reverse order of span so character replacements don't shift earlier offsets
        let mut sorted_lints = lints;
        sorted_lints.sort_by(|a, b| b.span.start.cmp(&a.span.start));

        for lint in sorted_lints {
            if let Some(first_suggestion) = lint.suggestions.first() {
                let replacement_str = match first_suggestion {
                    Suggestion::ReplaceWith(tokens) => tokens.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(""),
                    Suggestion::Remove => "".to_string(),
                    _ => continue,
                };


                let start = lint.span.start;
                let end = lint.span.end;

                if start <= end && end <= chars.len() {
                    let original_span: String = chars[start..end].iter().collect();

                    // Skip replacing if span contains placeholder markers
                    if original_span.contains("__NUM_PROT_") || replacement_str.contains("__NUM_PROT_") {
                        continue;
                    }

                    chars.splice(start..end, replacement_str.chars());

                    corrections.push(TextCorrection {
                        original: original_span,
                        corrected: replacement_str,
                        confidence: 0.90,
                        correction_type: format!("{:?}", lint.lint_kind),
                    });
                }
            }
        }

        let mut corrected_masked: String = chars.into_iter().collect();

        // 3. Unmask placeholders
        for (key, original_val) in &placeholders {
            corrected_masked = corrected_masked.replace(key, original_val);
        }

        // 3.5. Domain typo corrections (e.g., dharma/dhrama -> pharma)
        let domain_typos = [
            ("dharma", "pharma"),
            ("dhrama", "pharma"),
            ("billionaier", "billionaire"),
            ("billionaiers", "billionaires"),
            ("farmaceutical", "pharmaceutical"),
            ("biotak", "biotech"),
            ("softwear", "software"),
        ];

        for (typo, fixed) in domain_typos {
            let lower = corrected_masked.to_lowercase();
            if lower.contains(typo) {
                let pattern = format!(r"(?i)\b{}\b", regex::escape(typo));
                if let Ok(re) = Regex::new(&pattern) {
                    if re.is_match(&corrected_masked) {
                        let orig = typo.to_string();
                        corrected_masked = re.replace_all(&corrected_masked, fixed).to_string();
                        corrections.push(TextCorrection {
                            original: orig,
                            corrected: fixed.to_string(),
                            confidence: 0.95,
                            correction_type: "domain_typo".to_string(),
                        });
                    }
                }
            }
        }

        // 4. Enforce 30% max edit distance safety threshold
        let orig_len = raw_prompt.chars().count();
        if orig_len > 0 {
            let dist = edit_distance::edit_distance(raw_prompt, &corrected_masked);
            let max_allowed_edits = ((orig_len as f64) * 0.30).ceil() as usize;

            if dist > max_allowed_edits {
                tracing::warn!(
                    "Grammar correction exceeded 30% edit distance threshold (dist={}, max={}). Reverting to original.",
                    dist, max_allowed_edits
                );
                return (raw_prompt.to_string(), vec![]);
            }
        }

        (corrected_masked, corrections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_typo_correction() {
        let (corrected, _) = GrammarCorrector::correct("I want to become wealthiest men in the dharma industry. How where can I start from");
        assert!(corrected.contains("pharma industry"), "Expected dharma to be corrected to pharma, got: {}", corrected);

        let (corrected2, _) = GrammarCorrector::correct("Become the richest mogul in dhrama");
        assert!(corrected2.contains("pharma"), "Expected dhrama to be corrected to pharma, got: {}", corrected2);
    }
}
