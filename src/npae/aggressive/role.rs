use std::collections::{HashMap, HashSet};
use super::config::{RoleRule, DomainTaxonomy};
use super::intent::{IntentProfile, IntentClass};

// NOTE: We'll need specialized structs for scoring
struct ScoredRole<'a> {
    rule: &'a RoleRule,
    effective_priority: u32,
    total_weight: u32,
    match_count: usize,
}

pub fn generate_role(
    profile: &IntentProfile,
    raw: &str,
    domain_taxonomy: &[DomainTaxonomy],
    roles: &[RoleRule]
) -> String {
    let lower = raw.to_lowercase();
    let word_set: HashSet<&str> = lower.split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();

    // Step 1: Taxonomy boosts
    let domain_boosts: HashMap<&str, u32> = domain_taxonomy.iter()
        .filter_map(|tax| {
            let hit = tax.keywords.iter().any(|kw| word_set.contains(kw.as_str()));
            if hit { Some((tax.domain.as_str(), tax.boost)) } else { None }
        })
        .collect();

    // Step 2: Score all role rules
    let mut scored: Vec<ScoredRole> = roles.iter()
        .filter_map(|rule| {
            let (total_weight, match_count) = rule.verbs.iter().fold((0u32, 0usize), |(w, c), verb| {
                if word_set.contains(verb.word.as_str()) {
                    (w + verb.weight, c + 1)
                } else { (w, c) }
            });

            if match_count == 0 { return None; }

            let boost = domain_boosts.get(rule.domain.as_str()).copied().unwrap_or(0);
            Some(ScoredRole {
                rule,
                effective_priority: rule.priority + boost,
                total_weight,
                match_count,
            })
        })
        .collect();

    // Step 3: Sort: effective_priority DESC -> total_weight DESC -> match_count DESC
    scored.sort_by(|a, b| {
        b.effective_priority.cmp(&a.effective_priority)
            .then_with(|| b.total_weight.cmp(&a.total_weight))
            .then_with(|| b.match_count.cmp(&a.match_count))
    });

    // Step 4: Determine winner
    let base_title = scored.first()
        .map(|s| s.rule.base_title.as_str())
        .unwrap_or_else(|| match profile.primary_intent {
            IntentClass::Build     => "Expert Builder",
            IntentClass::Explain   => "Technical Instructor",
            IntentClass::Debug     => "Systems Specialist",
            IntentClass::Analyze   => "Senior Analyst",
            IntentClass::Transform => "Refactoring Expert",
        });

    let domain = extract_domain_noun(raw)
        .unwrap_or_else(|| to_title_case(&profile.domain));

    format!("{} {}", domain, base_title)
}

fn extract_domain_noun(_raw: &str) -> Option<String> {
    // Simple heuristic: if we find capitalized domain-looking words, use them
    // Otherwise return None
    None 
}

fn to_title_case(s: &str) -> String {
    s.split(|c: char| c == '-' || c == '_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
