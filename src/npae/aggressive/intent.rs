use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::DeliverableType;

#[derive(Debug, Clone, PartialEq)]
pub enum IntentClass {
    Build,
    Explain,
    Debug,
    Analyze,
    Transform,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KnowledgeLevel {
    Novice,
    Intermediate,
    Expert,
}

#[derive(Debug, Clone)]
pub struct IntentProfile {
    pub primary_intent: IntentClass,
    pub deliverable_type: DeliverableType,
    pub domain: String,
    pub user_knowledge: KnowledgeLevel,
    pub temporal_scope: String,
    pub output_preference: String,
    pub confidence: f32,
    // New: execution-aware fields
    pub has_timeline: bool,
    pub has_phases: bool,
    pub has_validation: bool,
    pub detected_geography: Option<String>,
    pub detected_risk: Option<String>,
    pub detected_biz_type: Option<String>,
    pub detected_team: Option<String>,
    pub dynamic_subject: Option<String>,
    pub baseline_knowledge: Option<String>,
    /// Goal intent from the deterministic rule layer (`wealth-building`, `legal-query`).
    pub goal_intent: Option<String>,
    /// Industry vertical, independent of primary role.
    pub vertical: Option<String>,
    /// When true, hash/Ory classifiers must not override domain or role.
    pub rule_locked: bool,
    /// Legal/regulatory is a constraint lens, not the owning role.
    pub legal_as_constraint: bool,
    /// Generic primary role composed by the rule layer.
    pub composed_primary_role: Option<String>,
}

impl IntentProfile {
    pub fn is_wealth_build(&self) -> bool {
        self.goal_intent.as_deref() == Some("wealth-building")
    }

    pub fn is_legal_query(&self) -> bool {
        self.goal_intent.as_deref() == Some("legal-query")
    }
}

pub fn extract(repr: &CompressedRepr, raw: &str) -> Result<IntentProfile, String> {
    extract_with_config(repr, raw, None)
}

pub fn extract_with_config(
    repr: &CompressedRepr,
    raw: &str,
    config: Option<&super::config::UnifiedConfig>,
) -> Result<IntentProfile, String> {
    if repr.intent_vec.is_empty() {
        return Err("intent_vec is empty".into());
    }

    let lower = raw.to_lowercase();

    // Rule layer first: intent × vertical. Hash domain is fallback only.
    let rule = crate::classifier::intent_rule::resolve_prompt_rules(raw);
    let domain = if let Some(ref verdict) = rule {
        verdict.pipeline_domain().to_string()
    } else {
        detect_domain(raw, config)
    };

    // Simplistic mapping: locate max in intent_vec
    let (max_idx, max_val) =
        repr.intent_vec
            .iter()
            .enumerate()
            .fold(
                (0, 0.0f32),
                |(max_i, max_v), (i, &v)| {
                    if v > max_v {
                        (i, v)
                    } else {
                        (max_i, max_v)
                    }
                },
            );

    let mut primary_intent = match max_idx % 5 {
        0 => IntentClass::Build,
        1 => IntentClass::Explain,
        2 => IntentClass::Debug,
        3 => IntentClass::Analyze,
        _ => IntentClass::Transform,
    };

    // Heuristic: questions about "is X better" or "why X" are usually Analytical
    if lower.contains("is ")
        || lower.contains("why ")
        || lower.contains("actually")
        || lower.contains("feels like")
        || lower.ends_with('?')
    {
        if primary_intent == IntentClass::Build || primary_intent == IntentClass::Transform {
            primary_intent = IntentClass::Analyze;
        }
    }

    // New Heuristic: "List", "Identify", "Find" at start usually implies Analyze or Explain, not Build
    let start_words = [
        "list", "identify", "find", "show", "what are", "give me", "provide",
    ];
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
    let dynamic_subject = extract_subject(raw);
    let baseline_knowledge = extract_baseline(raw);
    let deliverable_type = detect_deliverable_type(&lower, &primary_intent, &domain);
    if rule.as_ref().map(|v| v.intent)
        == Some(crate::classifier::intent_rule::GoalIntent::WealthBuild)
    {
        primary_intent = IntentClass::Build;
    }

    Ok(IntentProfile {
        primary_intent,
        deliverable_type,
        domain,
        user_knowledge,
        temporal_scope,
        output_preference,
        confidence: if rule.is_some() { 0.95 } else { max_val },
        has_timeline,
        has_phases,
        has_validation,
        detected_geography,
        detected_risk,
        detected_biz_type,
        detected_team,
        dynamic_subject,
        baseline_knowledge,
        goal_intent: rule.as_ref().map(|v| v.intent_class().to_string()),
        vertical: rule
            .as_ref()
            .and_then(|v| v.vertical.map(|s| s.to_string())),
        rule_locked: rule.is_some(),
        legal_as_constraint: rule
            .as_ref()
            .map(|v| v.legal_as_constraint)
            .unwrap_or(false),
        composed_primary_role: rule.as_ref().map(|v| v.primary_role.to_string()),
    })
}

/// Extract the user's current professional or technical baseline
fn extract_baseline(raw: &str) -> Option<String> {
    let lower = raw.to_lowercase();
    let patterns = [
        ("currently a ", ""),
        ("starting from ", ""),
        ("my background is ", ""),
        ("experience as a ", ""),
        ("background in ", ""),
        ("work as a ", ""),
    ];

    for (pattern, _) in &patterns {
        if let Some(start_idx) = lower.find(pattern) {
            let actual_start = start_idx + pattern.len();
            let remaining = &raw[actual_start..];
            let end_idx = remaining.find(['.', ',', ';']).unwrap_or(remaining.len());
            let baseline = remaining[..end_idx].trim();
            if !baseline.is_empty() {
                return Some(baseline.to_string());
            }
        }
    }
    None
}

use crate::npae::ory::embeddings::embed_text;
use crate::npae::ory::math::cosine_similarity;

/// Dynamic domain detection — Uses Vector Space Modeling (VSM) and Cosine Similarity
/// Replaces heuristic keyword matching with semantic centroid comparison.
fn detect_domain(raw: &str, config: Option<&super::config::UnifiedConfig>) -> String {
    let prompt_vec = embed_text(raw);
    let mut scored: Vec<(String, f32)> = Vec::new();

    if let Some(cfg) = config {
        for tax in &cfg.domain_taxonomy {
            if tax.keywords.is_empty() {
                continue;
            }
            let score = centroid_similarity(&prompt_vec, &tax.keywords) * (tax.boost.max(1) as f32);
            scored.push((tax.domain.clone(), score));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let picked = pick_hash_domain(&scored);
        return super::config::normalize_domain(&picked, &cfg.domain_taxonomy).to_string();
    }

    let domains: &[(&str, &[&str])] = &[
        (
            "computers",
            &[
                "computer",
                "computing",
                "cpu",
                "processor",
                "memory",
                "kernel",
                "operating-system",
                "linux",
                "systems",
            ],
        ),
        (
            "science",
            &[
                "science",
                "scientific",
                "physics",
                "chemistry",
                "biology",
                "experiment",
                "hypothesis",
                "research",
            ],
        ),
        (
            "health",
            &[
                "health",
                "healthcare",
                "wellness",
                "medical",
                "clinical",
                "patient",
                "vitality",
                "prevention",
            ],
        ),
        (
            "nutrition",
            &[
                "nutritionist",
                "diet",
                "macro",
                "protein",
                "training",
                "nutrition",
                "supplement",
            ],
        ),
        (
            "software",
            &[
                "code",
                "rust",
                "api",
                "backend",
                "software",
                "developer",
                "programming",
                "implementation",
            ],
        ),
        (
            "business",
            &[
                "business", "startup", "revenue", "market", "strategy", "monetize", "pricing",
                "growth",
            ],
        ),
        (
            "data-science",
            &["data", "ml", "model", "prediction", "analysis"],
        ),
        (
            "education",
            &["teach", "learn", "curriculum", "course", "pedagogy"],
        ),
        (
            "creative",
            &["story", "novel", "plot", "fiction", "narrative"],
        ),
        (
            "health-fitness",
            &["workout", "exercise", "health", "wellness", "fitness"],
        ),
        (
            "legal",
            &["contract", "legal", "compliance", "law", "attorney"],
        ),
        (
            "marketing",
            &["marketing", "brand", "campaign", "seo", "audience"],
        ),
        (
            "finance",
            &[
                "money",
                "earn",
                "income",
                "wealth",
                "salary",
                "investment",
                "stock",
                "portfolio",
                "banking",
                "finance",
                "cashflow",
                "million",
                "millions",
                "billion",
                "dollars",
                "rich",
            ],
        ),
        (
            "devops",
            &["pipeline", "infrastructure", "cloud", "aws", "devops"],
        ),
        (
            "ai-ml",
            &["llm", "neural", "transformer", "alignment", "ai"],
        ),
        (
            "medical",
            &["medical", "patient", "doctor", "hospital", "healthcare"],
        ),
        (
            "cybersecurity",
            &["security", "hacking", "firewall", "encryption", "threat"],
        ),
        (
            "real-estate",
            &[
                "realtor",
                "property",
                "estate",
                "housing",
                "mortgage",
                "brokerage",
                "agent",
            ],
        ),
        (
            "workplace-productivity",
            &["productivity", "culture", "collaboration", "burnout"],
        ),
        (
            "career-growth",
            &[
                "career",
                "become",
                "transition",
                "professional",
                "job",
                "promotion",
                "certification",
                "credential",
                "salary",
                "role",
                "position",
            ],
        ),
    ];

    for (name, keywords) in domains {
        let kw: Vec<String> = keywords.iter().map(|s| (*s).to_string()).collect();
        scored.push(((*name).to_string(), centroid_similarity(&prompt_vec, &kw)));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    pick_hash_domain(&scored)
}

fn centroid_similarity(prompt_vec: &[f32], keywords: &[String]) -> f32 {
    let mut centroid = vec![0.0f32; crate::npae::ory::embeddings::EMBEDDING_DIM];
    for kw in keywords {
        let kw_vec = embed_text(kw);
        for i in 0..centroid.len() {
            centroid[i] += kw_vec[i];
        }
    }
    if !keywords.is_empty() {
        let n = keywords.len() as f32;
        for v in centroid.iter_mut() {
            *v /= n;
        }
    }
    crate::npae::ory::math::l2_normalize(&mut centroid);
    cosine_similarity(prompt_vec, &centroid)
}

fn pick_hash_domain(scored: &[(String, f32)]) -> String {
    if scored.is_empty() {
        return "general".to_string();
    }
    let top = scored[0].1;
    let second = scored.get(1).map(|s| s.1);
    if crate::classifier::intent_rule::hash_margin_allows(top, second, 0.05) {
        scored[0].0.clone()
    } else {
        "general".to_string()
    }
}

/// Multi-word noun-phrase extraction — extracts the most specific subject from ANY domain.
/// Examples:
///   "Build a Kubernetes autoscaler" → "Kubernetes Autoscaler"
///   "I want to become a real estate agent" → "Real Estate Agent"
///   "Design a meal plan for marathon training" → "Meal Plan"
fn extract_subject(raw: &str) -> Option<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "i", "me", "my", "we", "our", "you", "your", "he", "she", "it", "they", "is", "are", "was",
        "were", "be", "been", "being", "am", "the", "a", "an", "this", "that", "these", "those",
        "how", "why", "what", "when", "where", "which", "who", "do", "does", "did", "will",
        "would", "could", "should", "can", "may", "might", "have", "has", "had", "to", "for", "of",
        "in", "on", "at", "by", "with", "from", "and", "or", "but", "not", "if", "so", "just",
        "also", "more", "most", "very", "about", "into", "some", "want", "need", "like",
        "actually", "really",
    ]
    .iter()
    .copied()
    .collect();

    // Action verbs to skip when looking for the subject (the subject follows these)
    let action_verbs: std::collections::HashSet<&str> = [
        "build",
        "create",
        "make",
        "design",
        "develop",
        "implement",
        "write",
        "analyze",
        "explain",
        "debug",
        "fix",
        "deploy",
        "optimize",
        "plan",
        "compare",
        "evaluate",
        "generate",
        "set",
        "start",
        "become",
        "get",
        "help",
        "give",
        "provide",
        "show",
        "tell",
        "find",
        "list",
        "identify",
        "earn",
        "gain",
        "increase",
        "grow",
        "raise",
        "boost",
        "maximize",
        "scale",
        "accumulate",
        "learn",
        "study",
        "research",
        "diagnose",
        "treat",
        "improve",
        "advance",
        "launch",
    ]
    .iter()
    .copied()
    .collect();

    // Isolate the user's actual prompt (strip Context/RAG prefixes)
    let (_, _, user_prompt) = super::structurer::split_raw_input(raw);
    let words: Vec<&str> = user_prompt.split_whitespace().collect();

    if words.is_empty() {
        return None;
    }

    // Strategy: find the first contiguous run of non-stop, non-verb words
    // that represents the noun phrase (the "what" of the prompt).
    let mut best_phrase: Vec<String> = Vec::new();
    let mut current_phrase: Vec<String> = Vec::new();
    let mut past_first_verb = false;

    for word in &words {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '$');
        if clean.is_empty() {
            continue;
        }
        let lower = clean.to_lowercase();

        // Skip stop words and action verbs — they precede the subject
        if stop_words.contains(lower.as_str()) {
            // If we were building a phrase, save it as candidate
            if current_phrase.len() >= best_phrase.len() && !current_phrase.is_empty() {
                best_phrase = current_phrase.clone();
            }
            current_phrase.clear();
            continue;
        }

        if action_verbs.contains(lower.as_str()) {
            past_first_verb = true;
            if current_phrase.len() >= best_phrase.len() && !current_phrase.is_empty() {
                best_phrase = current_phrase.clone();
            }
            current_phrase.clear();
            continue;
        }

        // After an action verb, accumulate noun-phrase words
        if past_first_verb || current_phrase.is_empty() {
            // Title-case the word
            let titled = title_case_word(clean);
            current_phrase.push(titled);
        }
    }

    // Final flush
    if current_phrase.len() >= best_phrase.len() && !current_phrase.is_empty() {
        best_phrase = current_phrase;
    }

    // Cap at 4 words to keep subject concise
    if best_phrase.is_empty() {
        return None;
    }
    let result: Vec<String> = best_phrase.into_iter().take(4).collect();
    Some(result.join(" "))
}

/// Title-cases a single word: "kubernetes" → "Kubernetes", "$10M" stays "$10M", "API" stays "API"
fn title_case_word(word: &str) -> String {
    if word.starts_with('$') && word.len() > 1 {
        return format!("${}", title_case_word(&word[1..]));
    }
    // If it's already all-caps (acronym), keep it
    if word.len() <= 4 && word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
        return word.to_string();
    }
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
    }
}

/// Derive output preference from the raw input — replaces hardcoded "json"
fn detect_output_preference(lower: &str) -> String {
    // Planning/roadmap signals → structured markdown
    let planning_signals = [
        "plan",
        "roadmap",
        "step-by-step",
        "strategy",
        "phases",
        "timeline",
        "milestones",
        "action items",
        "checklist",
        "breakdown",
        "steps",
        "schedule",
        "list",
        "ways",
        "unconventional",
    ];
    if planning_signals.iter().any(|s| lower.contains(s)) {
        return "structured_markdown".to_string();
    }

    // Data/API/schema signals → JSON
    let json_signals = [
        "json",
        "api response",
        "schema",
        "payload",
        "endpoint",
        "data structure",
    ];
    if json_signals.iter().any(|s| lower.contains(s)) {
        return "json".to_string();
    }

    // Code signals → code block
    let code_signals = ["code", "implement", "function", "class", "module", "script"];
    if code_signals.iter().any(|s| lower.contains(s)) {
        return "code".to_string();
    }

    // Table/comparison signals → table
    let table_signals = [
        "compare",
        "versus",
        "vs",
        "table",
        "comparison",
        "pros and cons",
    ];
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
    let phase_signals = [
        "phase",
        "phases",
        "stage",
        "stages",
        "sprint",
        "iteration",
        "milestone",
    ];
    let has_phases = phase_signals.iter().any(|s| lower.contains(s));

    // Multi-year and specific timeline patterns
    let specific_spans = [
        "10 years",
        "7 years",
        "5 years",
        "3 years",
        "2 years",
        "1 year",
        "12 months",
        "6 months",
        "90 days",
        "30 days",
    ];

    let mut detected_scope = None;
    for span in &specific_spans {
        if lower.contains(span) {
            detected_scope = Some(span.replace(' ', "_"));
            break;
        }
    }

    // General timeline signals
    if detected_scope.is_none() {
        let timeline_patterns = [
            ("day", "daily"),
            ("week", "weekly"),
            ("month", "monthly"),
            ("quarter", "quarterly"),
            ("year", "yearly"),
            ("hour", "hourly"),
        ];

        for (short, long) in &timeline_patterns {
            if lower.contains(long) || lower.contains(short) {
                detected_scope = Some(long.to_string());
                break;
            }
        }
    }

    let has_timeline = detected_scope.is_some() || has_phases;

    let scope = if has_phases && detected_scope.is_some() {
        format!("phased_{}", detected_scope.unwrap())
    } else if has_phases {
        "phased".to_string()
    } else if let Some(s) = detected_scope {
        s
    } else if lower.contains("long-term") || lower.contains("long term") {
        "long_term".to_string()
    } else if lower.contains("short-term")
        || lower.contains("short term")
        || lower.contains("quick")
    {
        "short_term".to_string()
    } else if lower.contains("now")
        || lower.contains("immediately")
        || lower.contains("asap")
        || lower.contains("urgent")
    {
        "immediate".to_string()
    } else {
        "unspecified".to_string()
    };

    (scope, has_timeline, has_phases)
}

/// Detect knowledge level from vocabulary sophistication, not just intent_vec magnitude
fn detect_knowledge_level(lower: &str, intent_confidence: f32) -> KnowledgeLevel {
    // Expert vocabulary signals
    let expert_signals = [
        "architecture",
        "optimize",
        "scale",
        "distributed",
        "microservice",
        "latency",
        "throughput",
        "concurrency",
        "idempotent",
        "consensus",
        "pharmacokinetics",
        "bioavailability",
        "periodization",
        "macros",
        "p/e ratio",
        "roi",
        "cagr",
        "cap table",
    ];
    let expert_hits = expert_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

    // Novice signals
    let novice_signals = [
        "how to",
        "what is",
        "beginner",
        "basic",
        "simple",
        "easy",
        "learn",
        "start",
        "new to",
        "first time",
    ];
    let novice_hits = novice_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

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
    let validation_signals = [
        "test",
        "verify",
        "validate",
        "check",
        "audit",
        "review",
        "quality",
        "benchmark",
        "measure",
        "kpi",
        "metric",
        "criteria",
        "acceptance",
    ];
    validation_signals.iter().any(|s| lower.contains(s))
}

/// Detect geography from input
fn detect_geography(lower: &str) -> Option<String> {
    let geos = [
        ("india", "India"),
        ("us", "United States"),
        ("usa", "United States"),
        ("uk", "United Kingdom"),
        ("europe", "Europe"),
        ("asia", "Asia"),
        ("africa", "Africa"),
        ("australia", "Australia"),
        ("canada", "Canada"),
        ("global", "Global"),
        ("worldwide", "Global"),
        ("local", "Local"),
        ("domestic", "Domestic"),
        ("international", "International"),
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
    if lower.contains("conservative")
        || lower.contains("safe")
        || lower.contains("low risk")
        || lower.contains("minimal risk")
    {
        Some("conservative".to_string())
    } else if lower.contains("aggressive")
        || lower.contains("high risk")
        || lower.contains("bold")
        || lower.contains("disruptive")
    {
        Some("aggressive".to_string())
    } else if lower.contains("balanced")
        || lower.contains("moderate")
        || lower.contains("calculated")
    {
        Some("moderate".to_string())
    } else {
        None
    }
}

/// Detect business type from input
fn detect_business_type(lower: &str) -> Option<String> {
    if lower.contains("saas") || lower.contains("software as a service") {
        Some("SaaS".to_string())
    } else if lower.contains("e-commerce")
        || lower.contains("ecommerce")
        || lower.contains("online store")
    {
        Some("E-commerce".to_string())
    } else if lower.contains("consulting")
        || lower.contains("agency")
        || lower.contains("freelance")
    {
        Some("Services".to_string())
    } else if lower.contains("physical")
        || lower.contains("retail")
        || lower.contains("brick")
        || lower.contains("store")
    {
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
    if lower.contains("solo")
        || lower.contains("solopreneur")
        || lower.contains("alone")
        || lower.contains("one person")
        || lower.contains("just me")
    {
        Some("solo".to_string())
    } else if lower.contains("small team")
        || lower.contains("co-founder")
        || lower.contains("partner")
    {
        Some("small_team".to_string())
    } else if lower.contains("team")
        || lower.contains("hire")
        || lower.contains("employees")
        || lower.contains("staff")
    {
        Some("team".to_string())
    } else {
        None
    }
}

/// Classify the type of deliverable the user expects — prevents intent mixing.
/// Content (email, article) should NOT get roadmaps; Strategy (plan, roadmap) should NOT get content-style formatting.
fn detect_deliverable_type(lower: &str, intent: &IntentClass, domain: &str) -> DeliverableType {
    // Content signals: the user wants a written piece, not a plan
    let content_signals = [
        "write",
        "draft",
        "compose",
        "email",
        "article",
        "essay",
        "blog post",
        "copy",
        "caption",
        "script",
        "speech",
        "letter",
        "message",
        "description",
        "summary",
        "abstract",
        "headline",
        "tagline",
        "slogan",
    ];
    let content_hits = content_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

    // Strategy signals: the user wants a plan, roadmap, or strategic framework
    let strategy_signals = [
        "plan",
        "roadmap",
        "strategy",
        "business model",
        "go-to-market",
        "framework",
        "phases",
        "milestones",
        "timeline",
        "action items",
        "initiative",
        "proposal",
        "blueprint",
        "playbook",
        "campaign strategy",
    ];
    let strategy_hits = strategy_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

    // Artifact signals: the user wants a concrete technical artifact
    let artifact_signals = [
        "implement",
        "build",
        "code",
        "api",
        "schema",
        "config",
        "function",
        "class",
        "module",
        "script",
        "endpoint",
        "database",
        "query",
        "migration",
        "deploy",
        "dockerfile",
        "pipeline",
    ];
    let artifact_hits = artifact_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

    // Analysis signals: the user wants evaluation, comparison, or investigation
    let analysis_signals = [
        "analyze",
        "compare",
        "evaluate",
        "review",
        "assess",
        "investigate",
        "benchmark",
        "audit",
        "pros and cons",
        "trade-off",
        "feasibility",
        "gap analysis",
        "root cause",
    ];
    let analysis_hits = analysis_signals
        .iter()
        .filter(|&&s| lower.contains(s))
        .count();

    // If both content AND strategy signals are strong, it's Hybrid
    if content_hits >= 1 && strategy_hits >= 1 {
        return DeliverableType::Hybrid;
    }

    // Highest signal wins
    let max_hits = content_hits
        .max(strategy_hits)
        .max(artifact_hits)
        .max(analysis_hits);

    if max_hits == 0 {
        // Fallback: use intent + domain to infer
        return match intent {
            IntentClass::Build => {
                if domain == "software-engineering" || domain == "devops-infra" || domain == "ai-ml"
                {
                    DeliverableType::Artifact
                } else {
                    DeliverableType::Strategy
                }
            }
            IntentClass::Explain => DeliverableType::Content,
            IntentClass::Debug => DeliverableType::Artifact,
            IntentClass::Analyze => DeliverableType::Analysis,
            IntentClass::Transform => DeliverableType::Artifact,
        };
    }

    if content_hits == max_hits {
        DeliverableType::Content
    } else if strategy_hits == max_hits {
        DeliverableType::Strategy
    } else if artifact_hits == max_hits {
        DeliverableType::Artifact
    } else {
        DeliverableType::Analysis
    }
}
