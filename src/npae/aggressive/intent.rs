use crate::npae::compression::types::CompressedRepr;

#[derive(Debug, Clone, PartialEq)]
pub enum IntentClass { Build, Explain, Debug, Analyze, Transform }

#[derive(Debug, Clone, PartialEq)]
pub enum KnowledgeLevel { Novice, Intermediate, Expert }

#[derive(Debug, Clone)]
pub struct IntentProfile {
    pub primary_intent:    IntentClass,
    pub domain:            String,
    pub user_knowledge:    KnowledgeLevel,
    pub temporal_scope:    String,
    pub output_preference: String,
    pub confidence:        f32,
    // New: execution-aware fields
    pub has_timeline:      bool,
    pub has_phases:        bool,
    pub has_validation:    bool,
    pub detected_geography: Option<String>,
    pub detected_risk:     Option<String>,
    pub detected_biz_type: Option<String>,
    pub detected_team:     Option<String>,
    pub dynamic_subject:   Option<String>,
}

pub fn extract(repr: &CompressedRepr, raw: &str) -> Result<IntentProfile, String> {
    if repr.intent_vec.is_empty() {
        return Err("intent_vec is empty".into());
    }

    let lower = raw.to_lowercase();

    // Dynamic domain detection (10+ domains)
    let domain = detect_domain(raw);

    // Simplistic mapping: locate max in intent_vec
    let (max_idx, max_val) = repr.intent_vec.iter().enumerate()
        .fold((0, 0.0f32), |(max_i, max_v), (i, &v)| {
            if v > max_v { (i, v) } else { (max_i, max_v) }
        });

    let mut primary_intent = match max_idx % 5 {
        0 => IntentClass::Build,
        1 => IntentClass::Explain,
        2 => IntentClass::Debug,
        3 => IntentClass::Analyze,
        _ => IntentClass::Transform,
    };

    // Heuristic: questions about "is X better" or "why X" are usually Analytical
    if lower.contains("is ") || lower.contains("why ") || lower.contains("actually") || lower.contains("feels like") || lower.ends_with('?') {
        if primary_intent == IntentClass::Build || primary_intent == IntentClass::Transform {
            primary_intent = IntentClass::Analyze;
        }
    }

    // New Heuristic: "List", "Identify", "Find" at start usually implies Analyze or Explain, not Build
    let start_words = ["list", "identify", "find", "show", "what are", "give me", "provide"];
    if start_words.iter().any(|&w| lower.starts_with(w)) {
        if primary_intent == IntentClass::Build {
            primary_intent = IntentClass::Analyze;
        }
    }

    // Knowledge level: check for domain-specific vocabulary density, not just intent_vec magnitude
    let user_knowledge = detect_knowledge_level(&lower, max_val);

    // Derive output_preference from input — NOT hardcoded
    let output_preference = detect_output_preference(&lower);

    // Derive temporal_scope from input — NOT hardcoded
    let (temporal_scope, has_timeline, has_phases) = detect_temporal_scope(&lower);

    // Detect validation/quality signals
    let has_validation = detect_validation_signals(&lower);

    // Detect business constraints metadata
    let detected_geography = detect_geography(&lower);
    let detected_risk = detect_risk_tolerance(&lower);
    let detected_biz_type = detect_business_type(&lower);
    let detected_team = detect_team_composition(&lower);
    let dynamic_subject = if domain == "general" { extract_subject(raw) } else { None };

    Ok(IntentProfile {
        primary_intent,
        domain,
        user_knowledge,
        temporal_scope,
        output_preference,
        confidence: max_val,
        has_timeline,
        has_phases,
        has_validation,
        detected_geography,
        detected_risk,
        detected_biz_type,
        detected_team,
        dynamic_subject,
    })
}

/// Dynamic domain detection — 10+ domains, keyword clusters with weighted scoring
fn detect_domain(raw: &str) -> String {
    let lower = raw.to_lowercase();

    struct DomainScore {
        domain: &'static str,
        keywords: &'static [&'static str],
    }

    let domains = [
        DomainScore { domain: "sports-nutrition", keywords: &["nutritionist", "dietician", "dietitian", "runner", "athletes", "meal", "diet", "macro", "carb", "protein", "fiber", "training", "marathon", "supplements", "keto", "vegan", "paleo", "fasting"] },
        DomainScore { domain: "software-engineering", keywords: &["code", "rust", "python", "bug", "api", "database", "backend", "frontend", "deploy", "microservice", "kubernetes", "docker"] },
        DomainScore { domain: "technical-writing", keywords: &["article", "blog", "writing", "documentation", "tutorial", "readme", "guide", "manual"] },
        DomainScore { domain: "business-strategy", keywords: &["business", "startup", "revenue", "market", "profit", "investor", "funding", "saas", "b2b", "b2c", "growth", "scale", "venture", "bootstrap", "monetize", "customer"] },
        DomainScore { domain: "data-science", keywords: &["data", "ml", "machine learning", "model", "dataset", "neural", "prediction", "classification", "regression", "pandas", "tensorflow", "pytorch"] },
        DomainScore { domain: "education", keywords: &["teach", "learn", "learning", "faster", "curriculum", "course", "student", "lecture", "training program", "certification", "assessment", "memorize", "retention", "pedagogy", "study", "skill"] },
        DomainScore { domain: "creative-writing", keywords: &["story", "novel", "character", "plot", "fiction", "screenplay", "narrative", "dialogue"] },
        DomainScore { domain: "health-fitness", keywords: &["workout", "exercise", "cardio", "strength", "weight loss", "body", "health", "wellness", "recovery", "sleep"] },
        DomainScore { domain: "legal", keywords: &["contract", "legal", "compliance", "regulation", "patent", "trademark", "liability", "attorney", "lawsuit"] },
        DomainScore { domain: "marketing", keywords: &["marketing", "brand", "campaign", "seo", "content", "social media", "conversion", "funnel", "audience", "engagement", "ads"] },
        DomainScore { domain: "finance", keywords: &["investment", "portfolio", "stock", "crypto", "trading", "banking", "loan", "interest", "dividend", "hedge"] },
        DomainScore { domain: "devops-infra", keywords: &["ci/cd", "pipeline", "infrastructure", "terraform", "ansible", "monitoring", "logging", "alerting", "cloud", "aws", "gcp", "azure"] },
        DomainScore { domain: "ai-ml", keywords: &["alignment", "safety", "llm", "transformer", "neural", "agi", "general intelligence", "machine learning", "rlhf", "inference", "deep learning", "alignment"] },
        DomainScore { domain: "medical", keywords: &["medical", "patient", "doctor", "hospital", "surgery", "healthcare", "diagnosis", "treatment", "anatomy", "physiology"] },
        DomainScore { domain: "scientific-research", keywords: &["research", "experiment", "laboratory", "hypothesis", "data analysis", "publication", "peer-review", "methodology"] },
        DomainScore { domain: "cybersecurity", keywords: &["security", "hacking", "firewall", "encryption", "vulnerability", "pentest", "soc", "malware", "threat"] },
        DomainScore { domain: "ecommerce", keywords: &["store", "shopping", "cart", "checkout", "inventory", "warehouse", "logistics", "marketplace"] },
        DomainScore { domain: "real-estate", keywords: &["property", "house", "apartment", "mortgage", "realtor", "appraisal", "listing"] },
        DomainScore { domain: "workplace-productivity", keywords: &["remote", "wfh", "productivity", "culture", "collaboration", "engagement", "performance", "output", "burnout", "hybrid", "distributed team", "work-from-home"] },
    ];

    let mut best_domain = "general";
    let mut best_score = 0usize;

    for ds in &domains {
        let score = ds.keywords.iter()
            .filter(|&&kw| lower.contains(kw))
            .count();
        if score > best_score {
            best_score = score;
            best_domain = ds.domain;
        }
    }

    // Require at least 2 keyword hits for a confident domain match
    if best_score >= 2 {
        best_domain.to_string()
    } else if best_score == 1 {
        best_domain.to_string() // single hit — still better than "general"
    } else {
        "general".to_string()
    }
}

/// Simple subject extraction for dynamic domains — extracts the most likely focus noun
fn extract_subject(raw: &str) -> Option<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "is", "are", "was", "were", "the", "a", "an", "this", "that", "it", "how", "why", "what", "actually", "just", "more", "or", "to", "for"
    ].iter().copied().collect();

    let words: Vec<&str> = raw.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 3)
        .collect();

    for word in words {
        let l = word.to_lowercase();
        if !stop_words.contains(l.as_str()) {
            // Return first significant noun-like word as the subject
            return Some(l.chars().next().unwrap().to_uppercase().collect::<String>() + &l[1..]);
        }
    }
    None
}

/// Derive output preference from the raw input — replaces hardcoded "json"
fn detect_output_preference(lower: &str) -> String {
    // Planning/roadmap signals → structured markdown
    let planning_signals = ["plan", "roadmap", "step-by-step", "strategy", "phases", "timeline",
        "milestones", "action items", "checklist", "breakdown", "steps", "schedule", "list", "ways", "unconventional"];
    if planning_signals.iter().any(|s| lower.contains(s)) {
        return "structured_markdown".to_string();
    }

    // Data/API/schema signals → JSON
    let json_signals = ["json", "api response", "schema", "payload", "endpoint", "data structure"];
    if json_signals.iter().any(|s| lower.contains(s)) {
        return "json".to_string();
    }

    // Code signals → code block
    let code_signals = ["code", "implement", "function", "class", "module", "script"];
    if code_signals.iter().any(|s| lower.contains(s)) {
        return "code".to_string();
    }

    // Table/comparison signals → table
    let table_signals = ["compare", "versus", "vs", "table", "comparison", "pros and cons"];
    if table_signals.iter().any(|s| lower.contains(s)) {
        return "table".to_string();
    }

    // Narrative/explanation signals → prose
    let prose_signals = ["explain", "describe", "elaborate", "overview", "summary"];
    if prose_signals.iter().any(|s| lower.contains(s)) {
        return "prose".to_string();
    }

    // Default to markdown (not json) — markdown preserves reasoning depth
    "markdown".to_string()
}

/// Derive temporal scope from input — replaces hardcoded "immediate"
fn detect_temporal_scope(lower: &str) -> (String, bool, bool) {
    // Phase-based signals
    let phase_signals = ["phase", "phases", "stage", "stages", "sprint", "iteration", "milestone"];
    let has_phases = phase_signals.iter().any(|s| lower.contains(s));

    // Timeline signals
    let timeline_patterns = [
        ("day", "daily"), ("week", "weekly"), ("month", "monthly"),
        ("quarter", "quarterly"), ("year", "yearly"), ("hour", "hourly"),
    ];

    let mut detected_scope = None;
    let has_timeline;

    for (short, long) in &timeline_patterns {
        if lower.contains(long) || lower.contains(short) {
            detected_scope = Some(long.to_string());
            break;
        }
    }

    has_timeline = detected_scope.is_some() || has_phases;

    let scope = if has_phases && detected_scope.is_some() {
        format!("phased_{}", detected_scope.unwrap())
    } else if has_phases {
        "phased".to_string()
    } else if let Some(s) = detected_scope {
        s
    } else if lower.contains("long-term") || lower.contains("long term") {
        "long_term".to_string()
    } else if lower.contains("short-term") || lower.contains("short term") || lower.contains("quick") {
        "short_term".to_string()
    } else if lower.contains("now") || lower.contains("immediately") || lower.contains("asap") || lower.contains("urgent") {
        "immediate".to_string()
    } else {
        "unspecified".to_string()
    };

    (scope, has_timeline, has_phases)
}

/// Detect knowledge level from vocabulary sophistication, not just intent_vec magnitude
fn detect_knowledge_level(lower: &str, intent_confidence: f32) -> KnowledgeLevel {
    // Expert vocabulary signals
    let expert_signals = ["architecture", "optimize", "scale", "distributed", "microservice",
        "latency", "throughput", "concurrency", "idempotent", "consensus",
        "pharmacokinetics", "bioavailability", "periodization", "macros",
        "p/e ratio", "roi", "cagr", "cap table"];
    let expert_hits = expert_signals.iter().filter(|&&s| lower.contains(s)).count();

    // Novice signals
    let novice_signals = ["how to", "what is", "beginner", "basic", "simple", "easy",
        "learn", "start", "new to", "first time"];
    let novice_hits = novice_signals.iter().filter(|&&s| lower.contains(s)).count();

    if expert_hits >= 2 || (expert_hits >= 1 && intent_confidence > 0.8) {
        KnowledgeLevel::Expert
    } else if novice_hits >= 2 {
        KnowledgeLevel::Novice
    } else {
        KnowledgeLevel::Intermediate
    }
}

/// Detect validation/quality assurance signals in the input
fn detect_validation_signals(lower: &str) -> bool {
    let validation_signals = ["test", "verify", "validate", "check", "audit", "review",
        "quality", "benchmark", "measure", "kpi", "metric", "criteria", "acceptance"];
    validation_signals.iter().any(|s| lower.contains(s))
}

/// Detect geography from input
fn detect_geography(lower: &str) -> Option<String> {
    let geos = [
        ("india", "India"), ("us", "United States"), ("usa", "United States"),
        ("uk", "United Kingdom"), ("europe", "Europe"), ("asia", "Asia"),
        ("africa", "Africa"), ("australia", "Australia"), ("canada", "Canada"),
        ("global", "Global"), ("worldwide", "Global"), ("local", "Local"),
        ("domestic", "Domestic"), ("international", "International"),
    ];
    for (signal, geo) in &geos {
        if lower.contains(signal) {
            return Some(geo.to_string());
        }
    }
    None
}

/// Detect risk tolerance from input
fn detect_risk_tolerance(lower: &str) -> Option<String> {
    if lower.contains("conservative") || lower.contains("safe") || lower.contains("low risk") || lower.contains("minimal risk") {
        Some("conservative".to_string())
    } else if lower.contains("aggressive") || lower.contains("high risk") || lower.contains("bold") || lower.contains("disruptive") {
        Some("aggressive".to_string())
    } else if lower.contains("balanced") || lower.contains("moderate") || lower.contains("calculated") {
        Some("moderate".to_string())
    } else {
        None
    }
}

/// Detect business type from input
fn detect_business_type(lower: &str) -> Option<String> {
    if lower.contains("saas") || lower.contains("software as a service") {
        Some("SaaS".to_string())
    } else if lower.contains("e-commerce") || lower.contains("ecommerce") || lower.contains("online store") {
        Some("E-commerce".to_string())
    } else if lower.contains("consulting") || lower.contains("agency") || lower.contains("freelance") {
        Some("Services".to_string())
    } else if lower.contains("physical") || lower.contains("retail") || lower.contains("brick") || lower.contains("store") {
        Some("Physical/Retail".to_string())
    } else if lower.contains("digital") || lower.contains("app") || lower.contains("platform") {
        Some("Digital Product".to_string())
    } else if lower.contains("content") || lower.contains("creator") || lower.contains("media") {
        Some("Content/Media".to_string())
    } else {
        None
    }
}

/// Detect team composition from input
fn detect_team_composition(lower: &str) -> Option<String> {
    if lower.contains("solo") || lower.contains("solopreneur") || lower.contains("alone") || lower.contains("one person") || lower.contains("just me") {
        Some("solo".to_string())
    } else if lower.contains("small team") || lower.contains("co-founder") || lower.contains("partner") {
        Some("small_team".to_string())
    } else if lower.contains("team") || lower.contains("hire") || lower.contains("employees") || lower.contains("staff") {
        Some("team".to_string())
    } else {
        None
    }
}
