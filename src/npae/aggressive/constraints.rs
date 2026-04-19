use std::collections::{HashMap, HashSet};
use super::config::{ConstraintRule, DomainTaxonomy};

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
    let word_set: HashSet<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    
    let has = |w: &str| word_set.contains(w);

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
                let all_match = group.iter().all(|tw| has(tw.word.as_str()));
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
