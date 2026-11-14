//! Perspective-Aware Role Synthesis for the NPAE Aggressive Path.
//!
//! // ARCH: ARCHITECTURE.md §1.4 & §6 — maps `(Domain x ActorPerspective x IntentClass)` and
//! // configured `RoleRule` weights into domain- and perspective-accurate role titles.

use super::config::{DomainTaxonomy, RoleRule};
use super::intent::{detect_actor_perspective, ActorPerspective, IntentClass, IntentProfile};
use std::collections::{HashMap, HashSet};

/// Intermediate candidate score for a configured `RoleRule`.
struct ScoredRole<'a> {
    rule: &'a RoleRule,
    effective_priority: u32,
    total_weight: u32,
    match_count: usize,
}

/// Generates a perspective-specific role title when `perspective` is `Consumer` or `Investor`
/// in a domain with dedicated consumer/investor advisory personas, or delegates to the
/// practitioner rule scorer otherwise.
///
/// # Panics
/// Never.
#[must_use]
pub fn generate_role_for_perspective(
    raw: &str,
    domain: &str,
    perspective: ActorPerspective,
    rules: &[RoleRule],
) -> String {
    if let Some(override_title) = perspective_role_override(domain, perspective) {
        return override_title.to_string();
    }

    let lower = raw.to_lowercase();
    let word_set: HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();

    let mut scored: Vec<ScoredRole> = rules
        .iter()
        .filter_map(|rule| {
            let (total_weight, match_count) =
                rule.verbs.iter().fold((0u32, 0usize), |(w, c), verb| {
                    if word_set.contains(verb.word.as_str()) {
                        (w + verb.weight, c + 1)
                    } else {
                        (w, c)
                    }
                });
            if match_count == 0 {
                return None;
            }
            let domain_bonus = if rule.domain.eq_ignore_ascii_case(domain) {
                3
            } else {
                0
            };
            Some(ScoredRole {
                rule,
                effective_priority: rule.priority + domain_bonus,
                total_weight,
                match_count,
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.effective_priority
            .cmp(&a.effective_priority)
            .then_with(|| b.total_weight.cmp(&a.total_weight))
            .then_with(|| b.match_count.cmp(&a.match_count))
    });

    scored
        .first()
        .map_or_else(
            || domain_anchored_fallback(domain, &IntentClass::Build),
            |s| s.rule.base_title.as_str(),
        )
        .to_string()
}

/// Returns an explicit consumer or investor role title when `(domain, perspective)` maps to a
/// dedicated non-practitioner persona.
fn perspective_role_override(domain: &str, perspective: ActorPerspective) -> Option<&'static str> {
    match (perspective, domain) {
        (ActorPerspective::Consumer, "real-estate" | "real estate") => {
            Some("Residential Home-Buying & Mortgage Advisor")
        }
        (ActorPerspective::Consumer, "legal") => Some("Consumer Legal Rights & Dispute Advisor"),
        (ActorPerspective::Consumer, "medical" | "health") => {
            Some("Clinical Health & Patient Care Educator")
        }
        (ActorPerspective::Consumer, "finance") => {
            Some("Personal Finance & Household Budget Advisor")
        }
        (ActorPerspective::Consumer, "education") => Some("Personal Learning & Mastery Coach"),
        (ActorPerspective::Investor, "real-estate" | "real estate") => {
            Some("Real Estate Investment & Underwriting Strategist")
        }
        _ => None,
    }
}

/// Synthesizes the primary role title (and optional focus suffix) for a prompt given its
/// `IntentProfile`, `DomainTaxonomy`, and `RoleRule` table.
///
/// # Panics
/// Never.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn generate_role(
    profile: &IntentProfile,
    raw: &str,
    domain_taxonomy: &[DomainTaxonomy],
    roles: &[RoleRule],
) -> String {
    if let Some(verdict) = crate::classifier::intent_rule::resolve_prompt_rules(raw) {
        return verdict.primary_role.to_string();
    }
    if let Some(ref composed) = profile.composed_primary_role {
        return composed.clone();
    }

    // Resolve any alias → canonical domain name so all downstream lookups are consistent.
    let canonical_domain = super::config::normalize_domain(&profile.domain, domain_taxonomy);
    let lower = raw.to_lowercase();

    // Determine effective actor perspective: honor explicit non-default perspective on profile,
    // or detect from raw text + canonical domain.
    let effective_perspective = if profile.perspective == ActorPerspective::Practitioner {
        detect_actor_perspective(&lower, canonical_domain)
    } else {
        profile.perspective
    };

    if perspective_role_override(canonical_domain, effective_perspective).is_some() {
        let base_role =
            generate_role_for_perspective(raw, canonical_domain, effective_perspective, roles);
        let mut topics = extract_focus_topics(raw, domain_taxonomy, canonical_domain);
        return if topics.is_empty() {
            base_role
        } else {
            let focus_str = match topics.len() {
                1 => topics[0].clone(),
                2 => format!("{} and {}", topics[0], topics[1]),
                _ => {
                    if let Some(last) = topics.pop() {
                        format!("{}, and {last}", topics.join(", "))
                    } else {
                        String::new()
                    }
                }
            };
            format!("{base_role} (focus: {focus_str})")
        };
    }

    let word_set: HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();

    // Step 1: Taxonomy boosts
    let domain_boosts: HashMap<&str, u32> = domain_taxonomy
        .iter()
        .filter_map(|tax| {
            let hit = tax.keywords.iter().any(|kw| word_set.contains(kw.as_str()));
            if hit {
                Some((tax.domain.as_str(), tax.boost))
            } else {
                None
            }
        })
        .collect();

    // Step 2: Score all role rules
    let mut scored: Vec<ScoredRole> = roles
        .iter()
        .filter_map(|rule| {
            let (total_weight, match_count) =
                rule.verbs.iter().fold((0u32, 0usize), |(w, c), verb| {
                    if word_set.contains(verb.word.as_str()) {
                        (w + verb.weight, c + 1)
                    } else {
                        (w, c)
                    }
                });

            if match_count == 0 {
                return None;
            }

            let boost = domain_boosts
                .get(rule.domain.as_str())
                .copied()
                .unwrap_or(0);
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
        b.effective_priority
            .cmp(&a.effective_priority)
            .then_with(|| b.total_weight.cmp(&a.total_weight))
            .then_with(|| b.match_count.cmp(&a.match_count))
    });

    // Step 4: Determine winner — domain-anchored fallbacks, NOT generic "Expert Builder"
    let base_title = scored
        .first()
        .map(|s| s.rule.base_title.as_str())
        .unwrap_or_else(|| domain_anchored_fallback(canonical_domain, &profile.primary_intent));

    let domain_name = if profile.domain != "general" {
        to_title_case(canonical_domain)
    } else if let Some(ref subject) = profile.dynamic_subject {
        subject.clone()
    } else {
        extract_domain_noun(raw).unwrap_or_else(|| "Systems".to_string())
    };

    let is_self_contained_role = base_title.split_whitespace().count() >= 2
        || base_title.ends_with("er")
        || base_title.ends_with("or")
        || base_title.ends_with("ist")
        || base_title.ends_with("ian");

    let base_role =
        if base_title.contains(&domain_name) || is_self_contained_role || domain_name.is_empty() {
            base_title.to_string()
        } else {
            format!("{} {}", domain_name, base_title)
        };

    // Step 5: Focus extraction
    let mut topics = extract_focus_topics(raw, domain_taxonomy, canonical_domain);
    if topics.is_empty() {
        base_role
    } else {
        let focus_str = match topics.len() {
            1 => topics[0].clone(),
            2 => format!("{} and {}", topics[0], topics[1]),
            _ => {
                if let Some(last) = topics.pop() {
                    format!("{}, and {}", topics.join(", "), last)
                } else {
                    String::new()
                }
            }
        };
        format!("{} (focus: {})", base_role, focus_str)
    }
}

/// Extract core focus topics from the raw input based on domain taxonomy
fn extract_focus_topics(raw: &str, taxonomy: &[DomainTaxonomy], domain: &str) -> Vec<String> {
    let lower = raw.to_lowercase();
    let mut topics = Vec::new();

    if let Some(tax) = taxonomy.iter().find(|t| t.domain == domain) {
        for kw in &tax.keywords {
            // Basic phrase matching
            if lower.contains(&kw.to_lowercase()) {
                topics.push(kw.clone());
            }
        }
    }

    // Sort by length DESC to prefer specific phrases over generic words
    topics.sort_by(|a, b| b.len().cmp(&a.len()));
    topics.dedup();

    // Filter out topics that are substrings of longer selected topics
    let mut filtered_topics: Vec<String> = Vec::new();
    for topic in topics {
        if !filtered_topics
            .iter()
            .any(|existing| existing.to_lowercase().contains(&topic.to_lowercase()))
        {
            filtered_topics.push(topic);
        }
    }

    filtered_topics.into_iter().take(3).collect()
}

/// Domain-anchored fallback roles — produces specific titles, never "Expert Builder"
fn domain_anchored_fallback(domain: &str, intent: &IntentClass) -> &'static str {
    match (domain, intent) {
        // Business  (was "business-strategy")
        ("business", IntentClass::Build) => "Business Strategist",
        ("business", IntentClass::Analyze) => "Business Analyst",
        ("business", IntentClass::Explain) => "Business Consultant",
        ("business", _) => "Business Advisor",

        // Computers
        ("computers", IntentClass::Build) => "Computer Systems Architect",
        ("computers", IntentClass::Analyze) => "Systems Performance Analyst",
        ("computers", IntentClass::Explain) => "Computer Science Educator",
        ("computers", _) => "Computer Systems Engineer",

        // Science
        ("science", IntentClass::Build) => "Lead Research Scientist",
        ("science", IntentClass::Analyze) => "Scientific Data Analyst",
        ("science", IntentClass::Explain) => "Science Communicator",
        ("science", _) => "Research Scientist",

        // Health
        ("health", IntentClass::Build) => "Clinical Health Specialist",
        ("health", IntentClass::Analyze) => "Health Data Analyst",
        ("health", IntentClass::Explain) => "Health & Wellness Educator",
        ("health", _) => "Healthcare Professional",

        // Software  (was "software-engineering")
        ("software", IntentClass::Build) => "Software Architect",
        ("software", IntentClass::Debug) => "Systems Debugger",
        ("software", IntentClass::Analyze) => "Code Analyst",
        ("software", IntentClass::Transform) => "Refactoring Specialist",
        ("software", _) => "Software Engineer",

        // DevOps  (was "devops-infra")
        ("devops", IntentClass::Build) => "Infrastructure Architect",
        ("devops", IntentClass::Debug) => "Site Reliability Engineer",
        ("devops", _) => "DevOps Engineer",

        // Nutrition  (was "sports-nutrition")
        ("nutrition", IntentClass::Build) => "Registered Dietician",
        ("nutrition", IntentClass::Analyze) => "Clinical Nutritionist",
        ("nutrition", IntentClass::Explain) => "Nutrition Educator",
        ("nutrition", _) => "Nutrition Specialist",

        // Health
        ("health-fitness", IntentClass::Build) => "Fitness Program Designer",
        ("health-fitness", _) => "Health & Fitness Specialist",

        // Data Science
        ("data-science", IntentClass::Build) => "ML Engineer",
        ("data-science", IntentClass::Analyze) => "Data Scientist",
        ("data-science", _) => "Data Analyst",

        // Education
        ("education", IntentClass::Build) => "Curriculum Designer",
        ("education", IntentClass::Analyze) => "Learning Strategist",
        ("education", IntentClass::Explain) => "Technical Instructor",
        ("education", _) => "Education Specialist",

        // Creative  (was "creative-writing")
        ("creative", IntentClass::Build) => "Creative Writer",
        ("creative", IntentClass::Transform) => "Content Editor",
        ("creative", _) => "Writing Specialist",

        // Legal
        ("legal", IntentClass::Analyze) => "Legal Analyst",
        ("legal", IntentClass::Build) => "Legal Strategist",
        ("legal", _) => "Legal Advisor",

        // Marketing
        ("marketing", IntentClass::Build) => "Marketing Strategist",
        ("marketing", IntentClass::Analyze) => "Marketing Analyst",
        ("marketing", _) => "Marketing Specialist",

        // Finance
        ("finance", IntentClass::Build) => "Financial Planner",
        ("finance", IntentClass::Analyze) => "Financial Analyst",
        ("finance", _) => "Finance Advisor",

        // AI/ML
        ("ai-ml", IntentClass::Build) => "AI Systems Architect",
        ("ai-ml", IntentClass::Explain) => "AI Systems Educator",
        ("ai-ml", IntentClass::Analyze) => "AI Researcher",
        ("ai-ml", IntentClass::Debug) => "AI Alignment Specialist",
        ("ai-ml", _) => "AI Specialist",

        // Medical
        ("medical", IntentClass::Analyze) => "Medical Researcher",
        ("medical", IntentClass::Explain) => "Medical Educator",
        ("medical", IntentClass::Debug) => "Diagnostic Specialist",
        ("medical", _) => "Healthcare Professional",

        // Scientific Research
        ("scientific-research", IntentClass::Analyze) => "Research Analyst",
        ("scientific-research", IntentClass::Build) => "Experimental Designer",
        ("scientific-research", _) => "Research Scientist",

        // Cybersecurity
        ("cybersecurity", IntentClass::Debug) => "Incident Responder",
        ("cybersecurity", IntentClass::Build) => "Security Architect",
        ("cybersecurity", _) => "Security Analyst",

        // Ecommerce
        ("ecommerce", IntentClass::Build) => "Store Architect",
        ("ecommerce", IntentClass::Analyze) => "Marketplace Analyst",
        ("ecommerce", _) => "Ecommerce Specialist",

        // Real Estate
        ("real-estate", IntentClass::Analyze) => "Property Analyst",
        ("real-estate", IntentClass::Build) => "Real Estate Strategist",
        ("real-estate", _) => "Real Estate Professional",

        // Travel & Hospitality
        ("travel", IntentClass::Build) => "Travel Planner",
        ("travel", IntentClass::Analyze) => "Travel & Tourism Analyst",
        ("travel", IntentClass::Explain) => "Travel Guide & Advisor",
        ("travel", _) => "Itinerary Specialist",

        // Workplace Productivity
        ("workplace-productivity", IntentClass::Analyze) => "Organizational Analyst",
        ("workplace-productivity", IntentClass::Build) => "Workplace Strategist",
        ("workplace-productivity", IntentClass::Explain) => "Workplace Consultant",
        ("workplace-productivity", _) => "Productivity Specialist",

        // General fallbacks — still domain-aware via intent
        ("general", IntentClass::Build) => "Solutions Architect",
        ("general", IntentClass::Explain) => "Technical Instructor",
        ("general", IntentClass::Debug) => "Systems Specialist",
        ("general", IntentClass::Analyze) => "Senior Analyst",
        ("general", IntentClass::Transform) => "Transformation Specialist",

        // Ultimate fallback — should rarely hit
        (_, IntentClass::Build) => "Solutions Architect",
        (_, IntentClass::Explain) => "Subject Matter Expert",
        (_, IntentClass::Debug) => "Diagnostic Specialist",
        (_, IntentClass::Analyze) => "Research Analyst",
        (_, IntentClass::Transform) => "Transformation Specialist",
    }
}

fn extract_domain_noun(raw: &str) -> Option<String> {
    let stop_words: HashSet<&str> = [
        "The", "A", "An", "This", "That", "It", "I", "You", "He", "She", "We", "They", "Is", "Are",
        "Was", "Were", "Will", "Would", "Can", "Could", "To", "And", "Or", "No", "Not", "Never",
        "Without", "Avoid", "Since", "Under", "Over", "Only", "Day", "Days", "Year", "Years",
    ]
    .iter()
    .copied()
    .collect();

    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() > 1 {
        for word in words.iter().skip(1) {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if let Some(first_char) = clean_word.chars().next() {
                if first_char.is_uppercase()
                    && !clean_word.chars().any(|c| c.is_numeric())
                    && !stop_words.contains(clean_word)
                    && clean_word.len() > 2
                {
                    return Some(clean_word.to_string());
                }
            }
        }
    }
    None
}

pub(crate) fn to_title_case(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_role_for_perspective_consumer_and_investor_overrides() {
        assert_eq!(
            generate_role_for_perspective(
                "Plan for buying a house in Austin with a $500k budget",
                "real-estate",
                ActorPerspective::Consumer,
                &[]
            ),
            "Residential Home-Buying & Mortgage Advisor"
        );
        assert_eq!(
            generate_role_for_perspective(
                "Analyze multifamily cap rate and NOI",
                "real-estate",
                ActorPerspective::Investor,
                &[]
            ),
            "Real Estate Investment & Underwriting Strategist"
        );
        assert_eq!(
            generate_role_for_perspective(
                "My landlord kept my security deposit",
                "legal",
                ActorPerspective::Consumer,
                &[]
            ),
            "Consumer Legal Rights & Dispute Advisor"
        );
        assert_eq!(
            generate_role_for_perspective(
                "I have knee pain and want to understand my symptoms",
                "medical",
                ActorPerspective::Consumer,
                &[]
            ),
            "Clinical Health & Patient Care Educator"
        );
        assert_eq!(
            generate_role_for_perspective(
                "Help me manage my paycheck and credit card debt",
                "finance",
                ActorPerspective::Consumer,
                &[]
            ),
            "Personal Finance & Household Budget Advisor"
        );
        assert_eq!(
            generate_role_for_perspective(
                "Help me learn calculus and study for my exam",
                "education",
                ActorPerspective::Consumer,
                &[]
            ),
            "Personal Learning & Mastery Coach"
        );
    }
}
