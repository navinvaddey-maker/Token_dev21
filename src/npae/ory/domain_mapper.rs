//! Domain Mapper — Maps Ory's inferred domain names to Aggressive engine canonical domains.
//!
//! // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — bridges Ory's inferred domain names, synonyms,
//! // and taxonomy aliases to the canonical 22-domain taxonomy in `config/unified.json`.

use crate::npae::aggressive::config::UnifiedConfig;

/// Maps an Ory-inferred domain to the closest Aggressive engine canonical domain.
///
/// # Arguments
/// * `ory_domain` - Domain string produced by `OryLearner::learn`.
/// * `config` - Unified configuration containing `domain_taxonomy`.
///
/// # Panics
/// Never.
pub fn map_domain(ory_domain: &str, config: &UnifiedConfig) -> DomainMapping {
    let lower = ory_domain.to_lowercase();

    // 1. Check exact match or alias match against unified.json domain taxonomy
    // ARCH: ARCHITECTURE.md §1.4 — resolve both canonical `dt.domain` and `dt.aliases` to `dt.domain`.
    for dt in &config.domain_taxonomy {
        if dt.domain.eq_ignore_ascii_case(&lower)
            || dt.aliases.iter().any(|a| a.eq_ignore_ascii_case(&lower))
        {
            return DomainMapping {
                matched_domain: Some(dt.domain.clone()),
                method: MatchMethod::ExactMatch,
                confidence: 1.0,
            };
        }
    }

    // 2. Check synonym table
    if let Some(canonical) = lookup_synonym(&lower) {
        // Verify the canonical domain (or an alias of it) exists in config
        for dt in &config.domain_taxonomy {
            if dt.domain.eq_ignore_ascii_case(canonical)
                || dt.aliases.iter().any(|a| a.eq_ignore_ascii_case(canonical))
            {
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
            if dt.domain.eq_ignore_ascii_case(parent)
                || dt.aliases.iter().any(|a| a.eq_ignore_ascii_case(parent))
            {
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
            let replace = match &best_match {
                None => true,
                Some((_, best_dist)) => dist < *best_dist,
            };
            if replace {
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
        let ory_words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        let keyword_hits = dt
            .keywords
            .iter()
            .filter(|kw| {
                ory_words
                    .iter()
                    .any(|w| kw.to_lowercase().contains(w) || w.contains(&kw.to_lowercase()))
            })
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

/// Checks if a domain has templates in the Aggressive engine's structurer or `UnifiedConfig`.
///
/// # Panics
/// Never.
pub fn has_structurer_templates(domain: &str) -> TemplateCoverage {
    has_structurer_templates_with_config(domain, None)
}

/// Checks if a domain has templates in the Aggressive engine's structurer or `config.domain_taxonomy`
/// (`phase_templates.is_some()`).
///
/// // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — checks canonical domains, legacy aliases, and
/// // `config.domain_taxonomy` (`phase_templates.is_some()`).
///
/// # Panics
/// Never.
pub fn has_structurer_templates_with_config(
    domain: &str,
    config: Option<&UnifiedConfig>,
) -> TemplateCoverage {
    let domains_with_execution_phases = [
        "computers",
        "science",
        "scientific-research",
        "health",
        "medical",
        "pharma",
        "finance",
        "software",
        "software-engineering",
        "business",
        "business-strategy",
        "nutrition",
        "sports-nutrition",
        "health-fitness",
        "real-estate",
        "travel",
        "legal",
        "data-science",
        "ai-ml",
        "devops",
        "devops-infra",
        "cybersecurity",
        "education",
        "marketing",
        "ecommerce",
        "workplace-productivity",
        "creative",
        "creative-writing",
        "career-growth",
    ];
    let domains_with_validation_steps = [
        "software",
        "software-engineering",
        "devops",
        "devops-infra",
        "computers",
        "business",
        "business-strategy",
        "finance",
        "legal",
        "medical",
        "health",
        "pharma",
        "nutrition",
        "sports-nutrition",
        "data-science",
        "ai-ml",
        "cybersecurity",
        "education",
        "travel",
        "real-estate",
        "marketing",
        "ecommerce",
        "science",
        "scientific-research",
    ];
    let domains_with_success_criteria = [
        "business",
        "business-strategy",
        "finance",
        "software",
        "software-engineering",
        "devops",
        "devops-infra",
        "computers",
        "ai-ml",
        "data-science",
        "medical",
        "health",
        "pharma",
        "nutrition",
        "sports-nutrition",
        "health-fitness",
        "workplace-productivity",
        "education",
        "travel",
        "legal",
        "real-estate",
        "cybersecurity",
        "marketing",
        "ecommerce",
        "science",
        "scientific-research",
        "career-growth",
    ];

    let lower = domain.to_lowercase();
    let config_has_phases = config
        .map(|cfg| {
            let norm =
                crate::npae::aggressive::config::normalize_domain(&lower, &cfg.domain_taxonomy);
            cfg.domain_taxonomy.iter().any(|dt| {
                (dt.domain.eq_ignore_ascii_case(norm)
                    || dt.aliases.iter().any(|a| a.eq_ignore_ascii_case(&lower)))
                    && dt.phase_templates.is_some()
            })
        })
        .unwrap_or(false);

    TemplateCoverage {
        has_execution_phases: config_has_phases
            || domains_with_execution_phases.iter().any(|d| *d == lower),
        has_validation_steps: domains_with_validation_steps.iter().any(|d| *d == lower),
        has_success_criteria: domains_with_success_criteria.iter().any(|d| *d == lower),
        has_role_template: true, // role.rs handles all domains with fallback
        has_constraint_rules: true, // constraints.rs handles all domains
    }
}

/// Result of mapping an Ory domain to a canonical taxonomy domain.
#[derive(Debug, Clone)]
pub struct DomainMapping {
    pub matched_domain: Option<String>,
    pub method: MatchMethod,
    pub confidence: f32,
}

/// Method by which a domain was matched in `map_domain`.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchMethod {
    ExactMatch,
    SynonymMatch,
    HierarchyMatch,
    FuzzyMatch,
    KeywordOverlap,
    NoMatch,
}

/// Coverage breakdown of structurer templates for a given domain.
#[derive(Debug, Clone)]
pub struct TemplateCoverage {
    pub has_execution_phases: bool,
    pub has_validation_steps: bool,
    pub has_success_criteria: bool,
    pub has_role_template: bool,
    pub has_constraint_rules: bool,
}

impl TemplateCoverage {
    /// Calculates overall template coverage score (`0.0..=1.0`).
    ///
    /// # Panics
    /// Never.
    pub fn score(&self) -> f32 {
        let mut total = 0.0;
        if self.has_execution_phases {
            total += 0.3;
        }
        if self.has_validation_steps {
            total += 0.2;
        }
        if self.has_success_criteria {
            total += 0.25;
        }
        if self.has_role_template {
            total += 0.15;
        }
        if self.has_constraint_rules {
            total += 0.1;
        }
        total
    }
}

// ============================================================================
// Synonym Table
// ============================================================================

/// Lookup synonym table: maps alternative domain names to canonical names.
fn lookup_synonym(domain: &str) -> Option<&'static str> {
    match domain {
        // Ory learner names & legacy aliases → canonical Aggressive engine names
        "endurance athletics" | "athletics" | "running" | "marathon" | "sports-nutrition" => {
            Some("nutrition")
        }
        "high-growth business"
        | "entrepreneurship"
        | "startups"
        | "venture capital"
        | "business-strategy" => Some("business"),
        "theoretical science" | "physics" | "quantum physics" | "scientific-research" => {
            Some("science")
        }
        "web development"
        | "app development"
        | "programming"
        | "coding"
        | "software-engineering"
        | "tech" => Some("software"),
        "artificial intelligence" | "machine learning" | "deep learning" | "nlp" => Some("ai-ml"),
        "medicine" | "clinical" | "healthcare" | "pharma" | "pharmaceutical" => Some("medical"),
        "investing" | "wealth management" | "personal finance" | "fintech" => Some("finance"),
        "infosec" | "information security" | "pentesting" | "ethical hacking" => {
            Some("cybersecurity")
        }
        "online retail" | "digital commerce" | "online store" => Some("ecommerce"),
        "content marketing" | "digital marketing" | "growth hacking" => Some("marketing"),
        "pedagogy" | "teaching" | "e-learning" | "online learning" | "tutoring" => {
            Some("education")
        }
        "fitness" | "exercise science" | "personal training" | "bodybuilding"
        | "health-fitness" => Some("health"),
        "devops" | "sre" | "platform engineering" | "cloud infrastructure" | "devops-infra" => {
            Some("devops")
        }
        "data engineering" | "analytics" | "big data" | "data analysis" => Some("data-science"),
        "corporate law" | "intellectual property" | "regulatory" => Some("legal"),
        "fiction writing" | "screenwriting" | "storytelling" | "copywriting"
        | "creative-writing" | "technical-writing" | "documentation" | "tech writing"
        | "api documentation" => Some("creative"),
        "work from home" | "remote work" | "hybrid work" | "office productivity" => {
            Some("workplace-productivity")
        }
        "real estate" | "property management" | "housing" => Some("real-estate"),
        "travel" | "tourism" | "itinerary" | "vacation" | "holiday planning" => Some("travel"),
        _ => None,
    }
}

/// Lookup hierarchy: find parent canonical domain for specialized sub-domains.
fn lookup_hierarchy(domain: &str) -> Option<(&'static str, f32)> {
    match domain {
        // Sub-domains of software
        "frontend" | "backend" | "fullstack" | "mobile development" => Some(("software", 0.8)),
        "api design" | "system design" | "distributed systems" => Some(("software", 0.75)),

        // Sub-domains of ai-ml
        "computer vision" | "natural language processing" | "reinforcement learning" => {
            Some(("ai-ml", 0.8))
        }
        "llm fine-tuning" | "prompt engineering" | "model training" => Some(("ai-ml", 0.85)),

        // Sub-domains of business
        "product management" | "business development" | "sales strategy" => {
            Some(("business", 0.75))
        }

        // Sub-domains of medical
        "nutrition science" | "pharmacology" | "public health" => Some(("medical", 0.7)),

        // Sub-domains of finance
        "cryptocurrency" | "forex" | "derivatives" => Some(("finance", 0.8)),

        _ => None,
    }
}
