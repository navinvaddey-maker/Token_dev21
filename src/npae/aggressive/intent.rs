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

use crate::npae::ory::math::cosine_similarity;
use crate::npae::ory::embeddings::embed_text;

/// Dynamic domain detection — Uses Vector Space Modeling (VSM) and Cosine Similarity
/// Replaces heuristic keyword matching with semantic centroid comparison.
fn detect_domain(raw: &str) -> String {
    let prompt_vec = embed_text(raw);
    
    // Centroids for each domain. In a production system, these are pre-calculated
    // embeddings of high-quality domain descriptions.
    let domains = [
        ("sports-nutrition", vec!["nutritionist", "diet", "macro", "protein", "training"]),
        ("software-engineering", vec!["code", "rust", "api", "backend", "software"]),
        ("technical-writing", vec!["article", "documentation", "tutorial", "guide"]),
        ("business-strategy", vec!["business", "startup", "revenue", "market", "strategy"]),
        ("data-science", vec!["data", "ml", "model", "prediction", "analysis"]),
        ("education", vec!["teach", "learn", "curriculum", "course", "pedagogy"]),
        ("creative-writing", vec!["story", "novel", "plot", "fiction", "narrative"]),
        ("health-fitness", vec!["workout", "exercise", "health", "wellness", "fitness"]),
        ("legal", vec!["contract", "legal", "compliance", "law", "attorney"]),
        ("marketing", vec!["marketing", "brand", "campaign", "seo", "audience"]),
        ("finance", vec!["investment", "stock", "portfolio", "banking", "finance"]),
        ("devops-infra", vec!["pipeline", "infrastructure", "cloud", "aws", "devops"]),
        ("ai-ml", vec!["llm", "neural", "transformer", "alignment", "ai"]),
        ("medical", vec!["medical", "patient", "doctor", "hospital", "healthcare"]),
        ("cybersecurity", vec!["security", "hacking", "firewall", "encryption", "threat"]),
        ("workplace-productivity", vec!["productivity", "culture", "collaboration", "burnout"]),
    ];

    let mut best_domain = "general";
    let mut best_score = 0.35f32; // Minimum threshold for domain matching

    for (name, keywords) in domains {
        // Simple centroid: average of keyword embeddings
        let mut centroid = vec![0.0f32; crate::npae::ory::embeddings::EMBEDDING_DIM];
        for kw in keywords {
            let kw_vec = embed_text(kw);
            for i in 0..centroid.len() {
                centroid[i] += kw_vec[i];
            }
        }
        crate::npae::ory::math::l2_normalize(&mut centroid);

        let similarity = cosine_similarity(&prompt_vec, &centroid);
        if similarity > best_score {
            best_score = similarity;
            best_domain = name;
        }
    }

    best_domain.to_string()
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
