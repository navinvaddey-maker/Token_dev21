use super::config::{ConstraintRule, DomainTaxonomy, WeightedWord};
use std::collections::{HashMap, HashSet};

struct ScoredConstraint<'a> {
    rule: &'a ConstraintRule,
    effective_priority: u32,
    total_weight: u32,
}

pub fn extract_constraints(
    raw: &str,
    domain_taxonomy: &[DomainTaxonomy],
    constraints: &[ConstraintRule],
) -> (Vec<String>, Vec<String>) {
    if raw.trim().is_empty() {
        return (vec![], vec![]);
    }

    let lower = raw.to_lowercase();
    let no_apos = lower.replace(['\'', '’'], "");
    let mut word_set: HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    for w in no_apos
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
    {
        word_set.insert(w);
    }

    let has = |w: &str| -> bool {
        if w.contains(' ') || !w.chars().all(|c| c.is_alphanumeric() || c == '-') {
            lower.contains(w) || no_apos.contains(w)
        } else {
            word_set.contains(w)
        }
    };

    let word_matches = |tw: &WeightedWord| -> bool {
        has(tw.word.as_str())
            || tw
                .synonyms
                .as_ref()
                .map_or(false, |syns| syns.iter().any(|s| has(s.as_str())))
    };

    // Step 1: Taxonomy boosts
    let domain_boosts: HashMap<&str, u32> = domain_taxonomy
        .iter()
        .filter_map(|tax| {
            if tax.keywords.iter().any(|kw| has(kw.as_str())) {
                Some((tax.domain.as_str(), tax.boost))
            } else {
                None
            }
        })
        .collect();

    // Step 2: Score each constraint rule
    let mut inclusions: Vec<ScoredConstraint> = vec![];
    let mut forbidden: Vec<ScoredConstraint> = vec![];

    for rule in constraints {
        let best_group_weight = rule
            .trigger
            .iter()
            .filter_map(|group| {
                let all_match = group.iter().all(|tw| word_matches(tw));
                if all_match {
                    Some(group.iter().map(|tw| tw.weight).sum::<u32>())
                } else {
                    None
                }
            })
            .max();

        if let Some(total_weight) = best_group_weight {
            let boost = domain_boosts
                .get(rule.domain.as_str())
                .copied()
                .unwrap_or(0);
            let scored = ScoredConstraint {
                rule,
                effective_priority: rule.priority + boost,
                total_weight,
            };
            if rule.is_forbidden {
                forbidden.push(scored);
            } else {
                inclusions.push(scored);
            }
        }
    }

    // Step 3: Sort each list: effective_priority DESC -> total_weight DESC
    let sort_fn = |list: &mut Vec<ScoredConstraint>| {
        list.sort_by(|a, b| {
            b.effective_priority
                .cmp(&a.effective_priority)
                .then_with(|| b.total_weight.cmp(&a.total_weight))
        });
    };
    sort_fn(&mut inclusions);
    sort_fn(&mut forbidden);

    let mut inc_out: Vec<String> = inclusions
        .iter()
        .map(|s| s.rule.description.clone())
        .collect();
    let mut forb_out: Vec<String> = forbidden
        .iter()
        .map(|s| s.rule.description.clone())
        .collect();

    // Step 4 (Step 3.1): Extract verbatim user constraints (negations/exclusions and
    // numeric quantity+unit spans) that are not already covered by a canonical rule.
    let (verbatim_inc, verbatim_forb) = extract_verbatim_spans(raw);
    for vf in verbatim_forb {
        let vf_low = vf.to_lowercase();
        let already_covered = forb_out
            .iter()
            .any(|existing| spans_overlap_semantically(existing, &vf_low));
        if !already_covered {
            forb_out.push(vf);
        }
    }
    for vi in verbatim_inc {
        let vi_low = vi.to_lowercase();
        let already_covered = inc_out
            .iter()
            .any(|existing| spans_overlap_semantically(existing, &vi_low));
        if !already_covered {
            inc_out.push(vi);
        }
    }

    (inc_out, forb_out)
}

fn spans_overlap_semantically(existing: &str, candidate_lower: &str) -> bool {
    let ex_low = existing.to_lowercase();
    if ex_low.contains(candidate_lower) || candidate_lower.contains(&ex_low) {
        return true;
    }
    // Check if any content token (>=4 chars) in the candidate already appears as a whole word in existing rule description
    let ex_words: HashSet<&str> = ex_low
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    candidate_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            w.len() >= 4
                && !w.chars().all(|c| c.is_ascii_digit())
                && !matches!(
                    *w,
                    "without"
                        | "avoid"
                        | "avoiding"
                        | "exclude"
                        | "excluding"
                        | "under"
                        | "within"
                        | "target"
                        | "bound"
                )
        })
        .any(|w| ex_words.contains(w) || (w.ends_with('s') && ex_words.contains(&w[..w.len() - 1])))
}

/// Deterministic span extractor for user-stated exclusions (`no X`, `without X`, `avoid X`,
/// `allergic to X`) and numeric target bounds (`1800 cal`, `500 mg`, `2000 usd`, `30000 inr`).
fn extract_verbatim_spans(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut inclusions = Vec::new();
    let mut forbidden = Vec::new();

    let stop_boundary: HashSet<&str> = [
        "for", "in", "on", "at", "to", "with", "and", "or", "but", "then", "while", "during",
        "because", "if", "when", "where", "that", "which", "who", "my", "our", "your", "the", "a",
        "an", "please", "only", "under", "within", "below", "above", "over", "around",
    ]
    .into_iter()
    .collect();

    // Clean punctuation into clause boundaries
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let n = tokens.len();
    let mut i = 0;

    while i < n {
        let clean = tokens[i]
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '$' && c != '%');
        let low = clean.to_lowercase();

        // 1. Negation / exclusion triggers: "no <noun>", "without <noun>", "avoid/avoiding <noun>"
        if matches!(
            low.as_str(),
            "no" | "without" | "avoid" | "avoiding" | "exclude" | "excluding"
        ) && i + 1 < n
        {
            let mut span_words = Vec::new();
            let mut j = i + 1;
            while j < n && span_words.len() < 3 {
                let w_raw = tokens[j];
                let w_clean = w_raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                let w_low = w_clean.to_lowercase();
                if w_clean.is_empty() || stop_boundary.contains(w_low.as_str()) {
                    break;
                }
                span_words.push(w_clean.to_string());
                if w_raw.ends_with([',', '.', ';', '!', '?']) {
                    break;
                }
                j += 1;
            }
            if !span_words.is_empty() {
                let phrase = format!("Exclude {}", span_words.join(" "));
                if !forbidden.contains(&phrase) {
                    forbidden.push(phrase);
                }
                i = j;
                continue;
            }
        }

        // 2. Quantitative + unit bounds: `<number> <unit>` (e.g., "1800 cal", "500 mg", "2000 usd")
        let is_number = !clean.is_empty()
            && clean
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ',' || c == '$' || c == '%');
        if is_number && i + 1 < n {
            let next_clean = tokens[i + 1]
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '/')
                .to_lowercase();
            let recognized_units = [
                "cal", "kcal", "calories", "g", "mg", "mcg", "kg", "lb", "lbs", "oz", "ml", "usd",
                "inr", "eur", "gbp", "dollars", "rupees", "lakh", "lakhs", "crore", "crores",
                "mg/dl", "mmol/l", "bpm",
            ];
            if recognized_units.contains(&next_clean.as_str()) {
                let bound = format!("Target/Bound: {} {}", clean, next_clean);
                if !inclusions.contains(&bound) {
                    inclusions.push(bound);
                }
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    (inclusions, forbidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_constraints_with_synonyms() {
        let taxonomy = vec![DomainTaxonomy {
            domain: "nutrition".to_string(),
            keywords: vec!["diet".to_string(), "meal".to_string()],
            boost: 2,
            aliases: vec![],
            persona_template: None,
            phase_templates: None,
        }];

        let constraints = vec![ConstraintRule {
            id: "no_gluten".to_string(),
            domain: "nutrition".to_string(),
            priority: 3,
            is_forbidden: true,
            description: "Gluten containing grains".to_string(),
            trigger: vec![vec![
                WeightedWord {
                    word: "no".to_string(),
                    weight: 5,
                    synonyms: Some(vec![
                        "avoid".to_string(),
                        "cant".to_string(),
                        "don't".to_string(),
                        "intolerant".to_string(),
                        "without".to_string(),
                        "free from".to_string(),
                    ]),
                },
                WeightedWord {
                    word: "gluten".to_string(),
                    weight: 10,
                    synonyms: None,
                },
            ]],
        }];

        // Exact match
        let (_, f1) =
            extract_constraints("Please make a meal with no gluten", &taxonomy, &constraints);
        assert_eq!(f1, vec!["Gluten containing grains"]);

        // Synonym match: "can't" / "cant"
        let (_, f2) = extract_constraints("I can't eat gluten in my diet", &taxonomy, &constraints);
        assert_eq!(f2, vec!["Gluten containing grains"]);

        // Synonym match: "intolerant"
        let (_, f3) = extract_constraints("I am gluten intolerant", &taxonomy, &constraints);
        assert_eq!(f3, vec!["Gluten containing grains"]);

        // Synonym match: multi-word "free from"
        let (_, f4) = extract_constraints("Make it free from gluten", &taxonomy, &constraints);
        assert_eq!(f4, vec!["Gluten containing grains"]);
    }
}
