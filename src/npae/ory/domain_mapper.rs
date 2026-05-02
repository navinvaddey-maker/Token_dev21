//! Domain Mapper — Maps Ory's inferred domain names to Aggressive engine domains
//!
//! Solves GAP-ORY-05: "Endurance Athletics" from Ory won't match "sports-nutrition"
//! in unified.json. This module provides synonym tables, hierarchy relationships,
//! and fuzzy matching to bridge the gap.

use crate::npae::aggressive::config::UnifiedConfig;

/// Map an Ory-inferred domain to the closest Aggressive engine domain
/// Returns the matched domain name and confidence score
pub fn map_domain(ory_domain: &str, config: &UnifiedConfig) -> DomainMapping {
    let lower = ory_domain.to_lowercase();
    
    // 1. Check exact match against unified.json domain taxonomy
    for dt in &config.domain_taxonomy {
        if dt.domain.to_lowercase() == lower {
            return DomainMapping {
                matched_domain: Some(dt.domain.clone()),
                method: MatchMethod::ExactMatch,
                confidence: 1.0,
            };
        }
    }
    
    // 2. Check synonym table
    if let Some(canonical) = lookup_synonym(&lower) {
        // Verify the canonical domain exists in config
        for dt in &config.domain_taxonomy {
            if dt.domain.to_lowercase() == canonical.to_lowercase() {
                return DomainMapping {
                    matched_domain: Some(dt.domain.clone()),
                    method: MatchMethod::SynonymMatch,
                    confidence: 0.9,
                };
            }
        }
        // Synonym exists but domain not in config — still useful info
        return DomainMapping {
            matched_domain: Some(canonical.to_string()),
            method: MatchMethod::SynonymMatch,
            confidence: 0.7,
        };
    }
    
    // 3. Check hierarchy relationships (parent/child domains)
    if let Some((parent, conf)) = lookup_hierarchy(&lower) {
        for dt in &config.domain_taxonomy {
            if dt.domain.to_lowercase() == parent.to_lowercase() {
                return DomainMapping {
                    matched_domain: Some(dt.domain.clone()),
                    method: MatchMethod::HierarchyMatch,
                    confidence: conf,
                };
            }
        }
    }
    
    // 4. Fuzzy match using edit distance against all config domains
    let mut best_match: Option<(String, usize)> = None;
    for dt in &config.domain_taxonomy {
        let dist = edit_distance::edit_distance(&lower, &dt.domain.to_lowercase());
        if dist <= 3 {
            if best_match.is_none() || dist < best_match.as_ref().unwrap().1 {
                best_match = Some((dt.domain.clone(), dist));
            }
        }
    }
    
    if let Some((domain, dist)) = best_match {
        let confidence = match dist {
            0 => 1.0,
            1 => 0.85,
            2 => 0.7,
            3 => 0.55,
            _ => 0.3,
        };
        return DomainMapping {
            matched_domain: Some(domain),
            method: MatchMethod::FuzzyMatch,
            confidence,
        };
    }
    
    // 5. Partial keyword overlap — check if Ory domain words appear in any config domain keywords
    for dt in &config.domain_taxonomy {
        let ory_words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
        let keyword_hits = dt.keywords.iter()
            .filter(|kw| ory_words.iter().any(|w| kw.to_lowercase().contains(w) || w.contains(&kw.to_lowercase())))
            .count();
        if keyword_hits >= 1 {
            return DomainMapping {
                matched_domain: Some(dt.domain.clone()),
                method: MatchMethod::KeywordOverlap,
                confidence: (keyword_hits as f32 * 0.25).min(0.75),
            };
        }
    }
    
    // No match found
    DomainMapping {
        matched_domain: None,
        method: MatchMethod::NoMatch,
        confidence: 0.0,
    }
}

/// Check if a domain has templates in the Aggressive engine's structurer
pub fn has_structurer_templates(domain: &str) -> TemplateCoverage {
    // These domains have explicit match arms in structurer.rs
    let domains_with_execution_phases = [
        "business-strategy", "software-engineering", "devops-infra",
        "sports-nutrition", "health-fitness", "workplace-productivity", "education",
    ];
    let domains_with_validation_steps = [
        "software-engineering", "devops-infra", "business-strategy",
    ];
    let domains_with_success_criteria = [
        "business-strategy", "finance", "software-engineering", "devops-infra",
        "ai-ml", "data-science", "medical", "pharma", "sports-nutrition",
        "health-fitness", "workplace-productivity", "education",
    ];
    
    let lower = domain.to_lowercase();
    
    TemplateCoverage {
        has_execution_phases: domains_with_execution_phases.iter().any(|d| *d == lower),
        has_validation_steps: domains_with_validation_steps.iter().any(|d| *d == lower),
        has_success_criteria: domains_with_success_criteria.iter().any(|d| *d == lower),
        has_role_template: true,  // role.rs handles all domains with fallback
        has_constraint_rules: true, // constraints.rs handles all domains
    }
}

#[derive(Debug, Clone)]
pub struct DomainMapping {
    pub matched_domain: Option<String>,
    pub method: MatchMethod,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchMethod {
    ExactMatch,
    SynonymMatch,
    HierarchyMatch,
    FuzzyMatch,
    KeywordOverlap,
    NoMatch,
}

#[derive(Debug, Clone)]
pub struct TemplateCoverage {
    pub has_execution_phases: bool,
    pub has_validation_steps: bool,
    pub has_success_criteria: bool,
    pub has_role_template: bool,
    pub has_constraint_rules: bool,
}

impl TemplateCoverage {
    /// Calculate overall template coverage score (0.0 - 1.0)
    pub fn score(&self) -> f32 {
        let mut total = 0.0;
        if self.has_execution_phases { total += 0.3; }
        if self.has_validation_steps { total += 0.2; }
        if self.has_success_criteria { total += 0.25; }
        if self.has_role_template { total += 0.15; }
        if self.has_constraint_rules { total += 0.1; }
        total
    }
}

// ============================================================================
// Synonym Table
// ============================================================================

/// Lookup synonym table: maps alternative domain names to canonical names
fn lookup_synonym(domain: &str) -> Option<&'static str> {
    match domain {
        // Ory learner names → Aggressive engine names
        "endurance athletics" | "athletics" | "running" | "marathon" 
            => Some("sports-nutrition"),
        "high-growth business" | "entrepreneurship" | "startups" | "venture capital"
            => Some("business-strategy"),
        "theoretical science" | "physics" | "quantum physics"
            => Some("scientific-research"),
        "web development" | "app development" | "programming" | "coding"
            => Some("software-engineering"),
        "artificial intelligence" | "machine learning" | "deep learning" | "nlp"
            => Some("ai-ml"),
        "medicine" | "clinical" | "healthcare" | "pharma" | "pharmaceutical"
            => Some("medical"),
        "investing" | "wealth management" | "personal finance" | "fintech"
            => Some("finance"),
        "infosec" | "information security" | "pentesting" | "ethical hacking"
            => Some("cybersecurity"),
        "online retail" | "digital commerce" | "online store"
            => Some("ecommerce"),
        "content marketing" | "digital marketing" | "growth hacking"
            => Some("marketing"),
        "pedagogy" | "teaching" | "e-learning" | "online learning" | "tutoring"
            => Some("education"),
        "fitness" | "exercise science" | "personal training" | "bodybuilding"
            => Some("health-fitness"),
        "devops" | "sre" | "platform engineering" | "cloud infrastructure"
            => Some("devops-infra"),
        "data engineering" | "analytics" | "big data" | "data analysis"
            => Some("data-science"),
        "corporate law" | "intellectual property" | "regulatory"
            => Some("legal"),
        "fiction writing" | "screenwriting" | "storytelling" | "copywriting"
            => Some("creative-writing"),
        "documentation" | "tech writing" | "api documentation"
            => Some("technical-writing"),
        "work from home" | "remote work" | "hybrid work" | "office productivity"
            => Some("workplace-productivity"),
        "real estate" | "property management" | "housing"
            => Some("real-estate"),
        _ => None,
    }
}

/// Lookup hierarchy: find parent domain for specialized sub-domains
fn lookup_hierarchy(domain: &str) -> Option<(&'static str, f32)> {
    match domain {
        // Sub-domains of software-engineering
        "frontend" | "backend" | "fullstack" | "mobile development"
            => Some(("software-engineering", 0.8)),
        "api design" | "system design" | "distributed systems"
            => Some(("software-engineering", 0.75)),
        
        // Sub-domains of ai-ml
        "computer vision" | "natural language processing" | "reinforcement learning"
            => Some(("ai-ml", 0.8)),
        "llm fine-tuning" | "prompt engineering" | "model training"
            => Some(("ai-ml", 0.85)),
        
        // Sub-domains of business-strategy
        "product management" | "business development" | "sales strategy"
            => Some(("business-strategy", 0.75)),
        
        // Sub-domains of medical
        "nutrition science" | "pharmacology" | "public health"
            => Some(("medical", 0.7)),
        
        // Sub-domains of finance
        "cryptocurrency" | "forex" | "derivatives"
            => Some(("finance", 0.8)),
        
        _ => None,
    }
}
