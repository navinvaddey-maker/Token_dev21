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

    // Step 4: Determine winner — domain-anchored fallbacks, NOT generic "Expert Builder"
    let base_title = scored.first()
        .map(|s| s.rule.base_title.as_str())
        .unwrap_or_else(|| domain_anchored_fallback(&profile.domain, &profile.primary_intent));

    let domain_name = if profile.domain != "general" {
        to_title_case(&profile.domain)
    } else if let Some(ref subject) = profile.dynamic_subject {
        subject.clone()
    } else {
        extract_domain_noun(raw).unwrap_or_else(|| "Systems".to_string())
    };

    let base_role = if base_title.contains(&domain_name) {
        base_title.to_string()
    } else {
        format!("{} {}", domain_name, base_title)
    };

    // Step 5: Focus extraction
    let mut topics = extract_focus_topics(raw, domain_taxonomy, &profile.domain);
    if topics.is_empty() {
        base_role
    } else {
        let focus_str = match topics.len() {
            1 => topics[0].clone(),
            2 => format!("{} and {}", topics[0], topics[1]),
            _ => {
                let last = topics.pop().unwrap();
                format!("{}, and {}", topics.join(", "), last)
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

    // Sort by length DESC to prefer specific phrases over generic words, then dedup
    topics.sort_by(|a, b| b.len().cmp(&a.len()));
    topics.dedup();
    
    topics.into_iter().take(3).collect()
}

/// Domain-anchored fallback roles — produces specific titles, never "Expert Builder"
fn domain_anchored_fallback(domain: &str, intent: &IntentClass) -> &'static str {
    match (domain, intent) {
        // Business
        ("business-strategy", IntentClass::Build) => "Business Strategist",
        ("business-strategy", IntentClass::Analyze) => "Business Analyst",
        ("business-strategy", IntentClass::Explain) => "Business Consultant",
        ("business-strategy", _) => "Business Advisor",
        
        // Software
        ("software-engineering", IntentClass::Build) => "Software Architect",
        ("software-engineering", IntentClass::Debug) => "Systems Debugger",
        ("software-engineering", IntentClass::Analyze) => "Code Analyst",
        ("software-engineering", IntentClass::Transform) => "Refactoring Specialist",
        ("software-engineering", _) => "Software Engineer",
        
        // DevOps
        ("devops-infra", IntentClass::Build) => "Infrastructure Architect",
        ("devops-infra", IntentClass::Debug) => "Site Reliability Engineer",
        ("devops-infra", _) => "DevOps Engineer",
        
        // Nutrition
        ("sports-nutrition", IntentClass::Build) => "Registered Dietician",
        ("sports-nutrition", IntentClass::Analyze) => "Clinical Nutritionist",
        ("sports-nutrition", IntentClass::Explain) => "Nutrition Educator",
        ("sports-nutrition", _) => "Nutrition Specialist",
        
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
        
        // Creative
        ("creative-writing", IntentClass::Build) => "Creative Writer",
        ("creative-writing", IntentClass::Transform) => "Content Editor",
        ("creative-writing", _) => "Writing Specialist",
        
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
        "The", "A", "An", "This", "That", "It", "I", "You", "He", "She", "We", "They", 
        "Is", "Are", "Was", "Were", "Will", "Would", "Can", "Could", "To", "And", "Or"
    ].iter().copied().collect();
    
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() > 1 {
        for word in words.iter().skip(1) {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if !clean_word.is_empty() {
                let first_char = clean_word.chars().next().unwrap();
                if first_char.is_uppercase() && !stop_words.contains(clean_word) {
                    return Some(clean_word.to_string());
                }
            }
        }
    }
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
