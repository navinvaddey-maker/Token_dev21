use std::collections::{HashMap, HashSet};
use super::config::{ConstraintRule, DomainTaxonomy, WeightedWord};

struct ScoredConstraint<'a> {
    rule: &'a ConstraintRule,
    effective_priority: u32,
    total_weight: u32,
}

pub fn extract_constraints(
    raw: &str,
    domain_taxonomy: &[DomainTaxonomy],
    constraints: &[ConstraintRule]
) -> (Vec<String>, Vec<String>) {
    if raw.trim().is_empty() { return (vec![], vec![]); }

    let lower = raw.to_lowercase();
    let no_apos = lower.replace(['\'', '’'], "");
    let mut word_set: HashSet<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    for w in no_apos.split(|c: char| !c.is_alphanumeric() && c != '-').filter(|s| !s.is_empty()) {
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
        has(tw.word.as_str()) || tw.synonyms.as_ref()
            .map_or(false, |syns| syns.iter().any(|s| has(s.as_str())))
    };

    // Step 1: Taxonomy boosts
    let domain_boosts: HashMap<&str, u32> = domain_taxonomy.iter()
        .filter_map(|tax| {
            if tax.keywords.iter().any(|kw| has(kw.as_str())) {
                Some((tax.domain.as_str(), tax.boost))
            } else { None }
        })
        .collect();

    // Step 2: Score each constraint rule
    let mut inclusions: Vec<ScoredConstraint> = vec![];
    let mut forbidden: Vec<ScoredConstraint> = vec![];

    for rule in constraints {
        let best_group_weight = rule.trigger.iter()
            .filter_map(|group| {
                let all_match = group.iter().all(|tw| word_matches(tw));
                if all_match {
                    Some(group.iter().map(|tw| tw.weight).sum::<u32>())
                } else { None }
            })
            .max();

        if let Some(total_weight) = best_group_weight {
            let boost = domain_boosts.get(rule.domain.as_str()).copied().unwrap_or(0);
            let scored = ScoredConstraint {
                rule,
                effective_priority: rule.priority + boost,
                total_weight,
            };
            if rule.is_forbidden { forbidden.push(scored); }
            else { inclusions.push(scored); }
        }
    }

    // Step 3: Sort each list: effective_priority DESC -> total_weight DESC
    let sort_fn = |list: &mut Vec<ScoredConstraint>| {
        list.sort_by(|a, b| {
            b.effective_priority.cmp(&a.effective_priority)
                .then_with(|| b.total_weight.cmp(&a.total_weight))
        });
    };
    sort_fn(&mut inclusions);
    sort_fn(&mut forbidden);

    (
        inclusions.iter().map(|s| s.rule.description.clone()).collect(),
        forbidden.iter().map(|s| s.rule.description.clone()).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_constraints_with_synonyms() {
        let taxonomy = vec![
            DomainTaxonomy {
                domain: "nutrition".to_string(),
                keywords: vec!["diet".to_string(), "meal".to_string()],
                boost: 2,
                persona_template: None,
                phase_templates: None,
            }
        ];

        let constraints = vec![
            ConstraintRule {
                id: "no_gluten".to_string(),
                domain: "nutrition".to_string(),
                priority: 3,
                is_forbidden: true,
                description: "Gluten containing grains".to_string(),
                trigger: vec![
                    vec![
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
                    ]
                ],
            }
        ];

        // Exact match
        let (_, f1) = extract_constraints("Please make a meal with no gluten", &taxonomy, &constraints);
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
