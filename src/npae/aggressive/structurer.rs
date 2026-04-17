use crate::npae::schema::types::{StructuredPrompt, PromptRole, PromptContext, PromptConstraints, HallucinationGuardConfig, LengthBound, ClarifyingQuestion};
use super::intent::IntentProfile;

pub fn render_crisp_prompt(prompt: &StructuredPrompt, questions: &[ClarifyingQuestion]) -> String {
    let mut out = String::new();
    
    out.push_str(&format!("# ROLE: {}\n", prompt.role.primary));
    out.push_str(&format!("Expert Domains: {}\n\n", prompt.role.expertise_domains.join(", ")));
    
    out.push_str(&format!("# CONTEXT: {}\n", prompt.context.description));
    out.push_str(&format!("User Level: {}\n", prompt.context.user_knowledge_level));
    out.push_str(&format!("Domain: {}\n", prompt.context.domain));
    out.push_str(&format!("Background: {}\n\n", prompt.context.background));
    
    out.push_str("# CONSTRAINTS:\n");
    for inc in &prompt.constraints.required_inclusions {
        out.push_str(&format!("- [REQUIRED] {}\n", inc));
    }
    for exc in &prompt.constraints.forbidden_topics {
        out.push_str(&format!("- [AVOID] {}\n", exc));
    }
    out.push_str(&format!("Tone: {}\n\n", prompt.constraints.tone));

    if !questions.is_empty() {
        out.push_str("# CLARIFYING QUESTIONS:\n");
        for q in questions {
            out.push_str(&format!("- {}\n", q.question));
        }
        out.push_str("\n");
    }

    out.push_str("# FINAL PROMPT:\n");
    out.push_str("Please execute the task described above with high fidelity and strict adherence to the specified role and constraints.\n");

    out
}

pub fn build(profile: &IntentProfile, raw: &str) -> Result<StructuredPrompt, String> {
    let role_primary = generate_role(profile);
    
    let user_level_str = match profile.user_knowledge {
        super::intent::KnowledgeLevel::Novice => "novice",
        super::intent::KnowledgeLevel::Intermediate => "intermediate",
        super::intent::KnowledgeLevel::Expert => "expert",
    }.to_string();

    let (inclusions, forbidden) = extract_constraints(raw);

    Ok(StructuredPrompt {
        role: PromptRole {
            primary: role_primary,
            expertise_domains: vec![profile.domain.clone()],
            persona_constraints: vec!["no_external_api_calls".into(), "production_grade_only".into()],
        },
        context: PromptContext {
            domain: profile.domain.clone(),
            description: generate_description(profile, raw),
            background: "Inferred from provided token-soup input.".into(),
            user_knowledge_level: user_level_str,
            temporal_scope: profile.temporal_scope.clone(),
            intent_vector: vec![profile.confidence],
        },
        constraints: PromptConstraints {
            output_format: profile.output_preference.clone(),
            length_bound: LengthBound { min: 200, max: 2000 },
            forbidden_topics: forbidden,
            required_inclusions: inclusions,
            tone: "technical-precise".into(),
        },
        hallucination_guard: HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into(), "cite_uncertainty".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
        }
    })
}

fn generate_role(profile: &IntentProfile) -> String {
    let base_title = match profile.primary_intent {
        super::intent::IntentClass::Build => "Expert Builder",
        super::intent::IntentClass::Explain => "Technical Instructor",
        super::intent::IntentClass::Debug => "Systems Specialist",
        super::intent::IntentClass::Analyze => "Senior Analyst",
        super::intent::IntentClass::Transform => "Refactoring Expert",
    };

    // Dynamic Domain Formatting: kebab-case/snake_case -> Title Case
    let formatted_domain = profile.domain
        .split(|c: char| c == '-' || c == '_')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ");

    format!("{} {}", formatted_domain, base_title)
}

fn generate_description(profile: &IntentProfile, _raw: &str) -> String {
    format!(
        "Constructing a high-fidelity {} framework for an {} user specializing in {}.",
        match profile.primary_intent {
            super::intent::IntentClass::Build => "plan",
            super::intent::IntentClass::Explain => "explanation",
            _ => "analysis"
        },
        match profile.user_knowledge {
            super::intent::KnowledgeLevel::Expert => "expert",
            _ => "standard"
        },
        profile.domain
    )
}

fn extract_constraints(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut inclusions = Vec::new();
    let mut forbidden = Vec::new();
    let lower = raw.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let has = |w: &str| words.contains(&w);

    // Dynamic Rule Definition
    struct Rule {
        trigger: Vec<Vec<&'static str>>, // OR of ANDs: (A and B) OR (C and D)
        description: &'static str,
        is_forbidden: bool,
    }

    let rules = vec![
        // Inclusions
        Rule { trigger: vec![vec!["high", "carb"], vec!["high", "carbohydrate"]], description: "High carbohydrate loading (race protocol)", is_forbidden: false },
        Rule { trigger: vec![vec!["low", "fiber"], vec!["low", "residue"]], description: "Low fiber/residue intake", is_forbidden: false },
        Rule { trigger: vec![vec!["anti", "inflammatory"]], description: "Anti-inflammatory focused ingredients", is_forbidden: false },
        Rule { trigger: vec![vec!["performance"], vec!["performance", "optimization"], vec!["elite"]], description: "Elite performance optimization standards", is_forbidden: false },
        
        // Forbidden
        Rule { trigger: vec![vec!["avoid", "nightshades"], vec!["no", "nightshades"], vec!["avoid", "nightshade"], vec!["no", "nightshade"]], description: "Nightshades (solanaceae family)", is_forbidden: true },
        Rule { trigger: vec![vec!["avoid", "dairy"], vec!["no", "dairy"], vec!["dairy", "free"]], description: "Dairy and lactose-based products", is_forbidden: true },
        Rule { trigger: vec![vec!["avoid", "gluten"], vec!["no", "gluten"], vec!["gluten", "free"]], description: "Gluten containing grains", is_forbidden: true },
        Rule { trigger: vec![vec!["avoid", "soy"], vec!["no", "soy"], vec!["soy", "free"]], description: "Soy-based derivatives", is_forbidden: true },
    ];

    for rule in rules {
        // Evaluate trigger: any group of ANDs matching?
        let matched = rule.trigger.iter().any(|group| {
            group.iter().all(|&word| has(word))
        });

        if matched {
            if rule.is_forbidden {
                forbidden.push(rule.description.to_string());
            } else {
                inclusions.push(rule.description.to_string());
            }
        }
    }

    (inclusions, forbidden)
}
