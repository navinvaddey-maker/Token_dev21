//! Intent & Domain Extraction for the NPAE Aggressive Path.
//!
//! // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-37 — implements deterministic Intent × Vertical rule
//! // composition with cached sentence-exemplar + keyword-centroid embedding fallback, whole-word
//! // lexical density scoring, multi-label secondary domain extraction, and the 3-Axis Intent
//! // Tensor (`Domain x ActorPerspective x IntentClass`).

use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::DeliverableType;

/// Identifies the user's actor perspective within the target domain (3-Axis Intent Tensor: D x P x I).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ActorPerspective {
    /// Buyer, patient, student, citizen, or personal end-user.
    Consumer,
    /// Professional practitioner, engineer, clinician, counsel, agent, or operator.
    #[default]
    Practitioner,
    /// Capital allocator, property/deal underwriter, or portfolio strategist.
    Investor,
}

impl ActorPerspective {
    /// Returns the lowercase key string for template and taxonomy lookups (`"consumer"`, `"practitioner"`, `"investor"`).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Consumer => "consumer",
            Self::Practitioner => "practitioner",
            Self::Investor => "investor",
        }
    }
}

/// Primary intent classification for a user prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentClass {
    Build,
    Explain,
    Debug,
    Analyze,
    Transform,
}

/// Estimated expertise level of the user based on vocabulary density.
#[derive(Debug, Clone, PartialEq)]
pub enum KnowledgeLevel {
    Novice,
    Intermediate,
    Expert,
}

/// Complete intent, domain, and execution metadata profile extracted from a user prompt.
#[allow(clippy::struct_excessive_bools)]
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
    /// Secondary domains detected in multi-domain prompts (Step 2.3).
    pub secondary_domains: Vec<String>,
    /// When true, hash/Ory classifiers must not override domain or role.
    pub rule_locked: bool,
    /// Legal/regulatory is a constraint lens, not the owning role.
    pub legal_as_constraint: bool,
    /// Generic primary role composed by the rule layer.
    pub composed_primary_role: Option<String>,
    /// Actor perspective within the resolved domain (`Consumer`, `Practitioner`, or `Investor`).
    pub perspective: ActorPerspective,
}

impl IntentProfile {
    /// Returns `true` when the deterministic rule layer classified the prompt as wealth-building.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn is_wealth_build(&self) -> bool {
        self.goal_intent.as_deref() == Some("wealth-building")
    }

    /// Returns `true` when the deterministic rule layer classified the prompt as a legal query.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn is_legal_query(&self) -> bool {
        self.goal_intent.as_deref() == Some("legal-query")
    }
}

/// Extracts an `IntentProfile` without external taxonomy configuration.
///
/// # Errors
/// Returns `Err` if `repr.intent_vec` is empty.
///
/// # Panics
/// Never.
pub fn extract(repr: &CompressedRepr, raw: &str) -> Result<IntentProfile, String> {
    extract_with_config(repr, raw, None)
}

/// Extracts an `IntentProfile` using optional `UnifiedConfig` taxonomy and exemplars.
///
/// # Errors
/// Returns `Err` if `repr.intent_vec` is empty.
///
/// # Panics
/// Never.
pub fn extract_with_config(
    repr: &CompressedRepr,
    raw: &str,
    config: Option<&super::config::UnifiedConfig>,
) -> Result<IntentProfile, String> {
    if repr.intent_vec.is_empty() {
        return Err("intent_vec is empty".into());
    }

    let lower = raw.to_lowercase();

    // Rule layer first: intent × vertical. Hash/embedding domain is fallback only.
    let rule = crate::classifier::intent_rule::resolve_prompt_rules(raw);
    let (domain, domain_confidence, secondary_domains) = if let Some(ref verdict) = rule {
        let mut secs = Vec::new();
        if let Some(v) = verdict.vertical {
            secs.push(v.to_string());
        }
        (verdict.pipeline_domain().to_string(), 0.95, secs)
    } else {
        detect_domain_with_confidence(raw, config)
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

    // FIX: Step 1.4 — Match whole words instead of substrings ("is " / "why ") to avoid
    // false triggers on words like "anyhow" or "this ".
    let words: std::collections::HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    if (words.contains("why")
        || words.contains("actually")
        || lower.contains("feels like")
        || lower.trim_end().ends_with('?'))
        && (primary_intent == IntentClass::Build || primary_intent == IntentClass::Transform)
    {
        primary_intent = IntentClass::Analyze;
    }

    // New Heuristic: "List", "Identify", "Find" at start usually implies Analyze or Explain, not Build
    let start_words = [
        "list", "identify", "find", "show", "what are", "give me", "provide",
    ];
    if start_words.iter().any(|&w| lower.starts_with(w)) && primary_intent == IntentClass::Build {
        primary_intent = IntentClass::Analyze;
    }

    // Knowledge level: check for domain-specific vocabulary density, not just intent_vec magnitude
    let user_knowledge = detect_knowledge_level(&lower, max_val);

    // Derive output_preference from input — NOT hardcoded
    let output_preference = detect_output_preference(&lower);

    // Derive temporal_scope from input — NOT hardcoded
    let (temporal_scope, has_timeline, has_phases) = detect_temporal_scope(&lower);

    // Detect validation/quality signals
    let has_validation = detect_validation_signals(&lower);

    // Detect actor perspective within resolved domain (3-Axis Intent Tensor: D x P x I)
    let perspective = detect_actor_perspective(&lower, &domain);

    // Detect business constraints metadata (strictly domain- and perspective-gated)
    let detected_geography = detect_geography(&lower);
    let detected_risk = detect_risk_tolerance(&lower);
    let detected_biz_type = detect_business_type(&lower, &domain, perspective);
    let detected_team = detect_team_composition(&lower);
    let dynamic_subject = extract_subject(raw);
    let baseline_knowledge = extract_baseline(raw);
    let deliverable_type = detect_deliverable_type(&lower, &primary_intent, &domain);
    if rule.as_ref().map(|v| v.intent)
        == Some(crate::classifier::intent_rule::GoalIntent::WealthBuild)
    {
        primary_intent = IntentClass::Build;
    }

    // FIX: Step 1.2 — Use calibrated `domain_confidence` (not `max_val` from `intent_vec`)
    // so downstream domain comparisons against Ory operate on domain signal strength.
    Ok(IntentProfile {
        primary_intent,
        deliverable_type,
        domain,
        user_knowledge,
        temporal_scope,
        output_preference,
        confidence: domain_confidence,
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
            .and_then(|v| v.vertical.map(std::string::ToString::to_string)),
        secondary_domains,
        rule_locked: rule.is_some(),
        legal_as_constraint: rule.as_ref().is_some_and(|v| v.legal_as_constraint),
        composed_primary_role: rule.as_ref().map(|v| v.primary_role.to_string()),
        perspective,
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
use once_cell::sync::Lazy;

/// Cached embedding vectors for individual keywords and exemplar sentences so that
/// `centroid_similarity` and exemplar prototype matching never re-run ONNX inference
/// for static taxonomy keywords across requests (Step 2.2 / GAP-37).
static EMBED_CACHE: Lazy<dashmap::DashMap<String, Vec<f32>>> = Lazy::new(dashmap::DashMap::new);

fn embed_cached(text: &str) -> Vec<f32> {
    if let Some(entry) = EMBED_CACHE.get(text) {
        return entry.value().clone();
    }
    let vec = embed_text(text);
    EMBED_CACHE.insert(text.to_string(), vec.clone());
    vec
}

/// Curated sentence-level exemplars per canonical domain (Step 2.1).
/// `all-MiniLM-L6-v2` is a sentence transformer; comparing against natural sentences
/// captures paraphrases that single-keyword centroids miss.
fn domain_exemplars(domain: &str) -> &'static [&'static str] {
    match domain {
        "finance" => &[
            "Build a diversified investment portfolio for retirement and passive income",
            "Analyze monthly cashflow, savings rate, debt payoff, and tax allocation",
            "Compare equity index funds, bonds, and fixed deposits for wealth preservation",
        ],
        "medical" | "health" | "pharma" => &[
            "Clinical diagnosis and treatment protocol for a hospital patient with symptoms",
            "Analyze pathology, radiology, and biomarkers before surgical intervention",
            "Preventative healthcare screening and medical rehabilitation plan",
        ],
        "nutrition" => &[
            "Create a high protein vegetarian meal plan with calorie and macro targets",
            "Design a keto or vegan diet protocol avoiding gluten, dairy, and allergens",
            "Nutritional supplements, hydration, and carbohydrate loading for endurance",
        ],
        "legal" => &[
            "Review commercial contract clauses for liability, IP, and regulatory compliance",
            "Trademark, patent litigation, and statutory obligations under jurisdiction",
            "Draft an NDA or legal data privacy checklist compliant with GDPR",
        ],
        "education" => &[
            "Design a university course syllabus, curriculum pedagogy, and student assessment",
            "Explain active recall teaching strategies for an academic lecture or tutor",
            "Compare classroom instruction with self-paced e-learning modules",
        ],
        "travel" => &[
            "Plan a multi-day travel itinerary, sightseeing schedule, and hotel booking",
            "Compare flight transit routes, resort lodging, and vacation budget costs",
            "Backpacking roadtrip and destination tourism excursion under budget",
        ],
        "software" => &[
            "Architect and implement a production backend microservice API in Rust or Python",
            "Debug database query latency, concurrency bottlenecks, and software unit tests",
            "Refactor application code architecture and REST or GraphQL API endpoints",
        ],
        "devops" => &[
            "Configure Kubernetes cluster autoscaling, Terraform infrastructure, and CI/CD pipelines",
            "Set up Docker container orchestration, Prometheus monitoring, and cloud deployment on AWS",
            "Automate site reliability engineering, zero-downtime rollouts, and observability logging",
        ],
        "ai-ml" => &[
            "Fine-tune a large language model transformer with RLHF alignment and safety guardrails",
            "Optimize neural network inference latency, vector embeddings, and RAG retrieval",
            "Design deep learning model architecture, prompt engineering, and evaluation benchmarks",
        ],
        "data-science" => &[
            "Build a predictive classification and regression pipeline on a tabular dataset",
            "Perform exploratory data analysis, feature engineering, and statistical hypothesis testing",
            "Evaluate machine learning model precision, recall, ROC-AUC, and data drift",
        ],
        "cybersecurity" => &[
            "Conduct a penetration test, threat modeling audit, and vulnerability assessment",
            "Harden network firewall rules, zero-trust authentication, and encryption protocols",
            "Investigate SOC security incident response, malware indicators, and SIEM alerts",
        ],
        "business" => &[
            "Develop a B2B SaaS go-to-market strategy, pricing model, and unit economics plan",
            "Analyze customer acquisition cost, LTV, churn, and startup venture scaling roadmap",
            "Design a competitive market positioning and enterprise revenue expansion framework",
        ],
        "real-estate" => &[
            "Analyze residential and commercial property valuation, cap rate, and rental cashflow",
            "Plan a real estate brokerage listing, mortgage financing, and buyer acquisition strategy",
            "Evaluate housing market comps, zoning regulations, and property management operations",
        ],
        "marketing" => &[
            "Launch a multi-channel digital marketing campaign, SEO content funnel, and brand strategy",
            "Optimize paid ad conversion rates, audience segmentation, and customer retention",
            "Design social media growth loops, email drip campaigns, and attribution analytics",
        ],
        "ecommerce" => &[
            "Optimize online store checkout conversion, shopping cart UX, and product catalog",
            "Manage ecommerce inventory forecasting, warehouse fulfillment, and marketplace listings",
            "Increase average order value and repeat purchase rate for a direct-to-consumer storefront",
        ],
        "science" => &[
            "Formulate a falsifiable scientific hypothesis and controlled laboratory experiment protocol",
            "Analyze empirical physics, chemistry, or biology research data for peer-reviewed publication",
            "Evaluate experimental methodology, statistical significance, and reproducibility",
        ],
        "computers" => &[
            "Optimize operating system kernel scheduling, CPU cache locality, and memory hierarchy",
            "Design low-level systems architecture, hardware driver interfaces, and instruction pipelines",
            "Analyze processor throughput, virtual memory paging, and embedded systems performance",
        ],
        _ => &[],
    }
}

/// Dynamic domain detection — Uses cached Vector Space Modeling (VSM), sentence-level
/// exemplar prototypes, whole-word lexical tie-breaking, and multi-label secondary domain
/// extraction (Steps 2.1, 2.2, 2.3).
///
/// // ARCH: ARCHITECTURE.md §1.4 — exposed as `pub` so `PipelineOrchestrator` can invoke the
/// // in-memory taxonomy + exemplar classifier when the SQLite scenario index abstains.
///
/// # Panics
/// Never.
pub fn detect_domain_with_confidence(
    raw: &str,
    config: Option<&super::config::UnifiedConfig>,
) -> (String, f32, Vec<String>) {
    let prompt_vec = embed_text(raw);
    let lower = raw.to_lowercase();
    let words: std::collections::HashSet<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    let has_kw = |kw: &str| -> bool {
        let k = kw.to_lowercase();
        if k.contains(' ') {
            lower.contains(&k)
        } else {
            words.contains(k.as_str())
        }
    };

    // Each entry: (domain, effective_score_in_0_to_1, lexical_hit_count, boost)
    let mut scored: Vec<(String, f32, usize, u32)> = Vec::new();

    if let Some(cfg) = config {
        for tax in &cfg.domain_taxonomy {
            if tax.keywords.is_empty() {
                continue;
            }
            let kw_cos = centroid_similarity(&prompt_vec, &tax.keywords).clamp(0.0, 1.0);
            let ex_cos = exemplar_similarity(&prompt_vec, domain_exemplars(&tax.domain));
            let raw_cos = kw_cos.max(ex_cos);
            let hits = tax.keywords.iter().filter(|kw| has_kw(kw)).count();
            let boost = tax.boost.max(1);
            let hit_bonus = if hits > 0 {
                (hits as f32 * 0.08).min(0.32) + ((boost.saturating_sub(1)) as f32 * 0.015)
            } else {
                0.0
            };
            let eff = (raw_cos + hit_bonus).clamp(0.0, 0.99);
            scored.push((tax.domain.clone(), eff, hits, boost));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let (picked, conf, secondaries) = pick_hash_domain_with_confidence(&scored);
        let canonical = super::config::normalize_domain(&picked, &cfg.domain_taxonomy).to_string();
        let norm_secs: Vec<String> = secondaries
            .into_iter()
            .map(|s| super::config::normalize_domain(&s, &cfg.domain_taxonomy).to_string())
            .filter(|s| s != &canonical)
            .collect();
        return (canonical, conf, norm_secs);
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
            "travel",
            &[
                "itinerary",
                "trip",
                "travel",
                "vacation",
                "holiday",
                "sightseeing",
                "tourism",
                "hotel",
                "resort",
                "tour",
            ],
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
        let kw_cos = centroid_similarity(&prompt_vec, &kw).clamp(0.0, 1.0);
        let ex_cos = exemplar_similarity(&prompt_vec, domain_exemplars(name));
        let raw_cos = kw_cos.max(ex_cos);
        let hits = keywords.iter().filter(|k| has_kw(k)).count();
        let hit_bonus = (hits as f32 * 0.08).min(0.32);
        scored.push((
            (*name).to_string(),
            (raw_cos + hit_bonus).clamp(0.0, 0.99),
            hits,
            1,
        ));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    pick_hash_domain_with_confidence(&scored)
}

fn exemplar_similarity(prompt_vec: &[f32], exemplars: &[&str]) -> f32 {
    exemplars
        .iter()
        .map(|ex| {
            let v = embed_cached(ex);
            cosine_similarity(prompt_vec, &v).clamp(0.0, 1.0)
        })
        .fold(0.0_f32, f32::max)
}

fn centroid_similarity(prompt_vec: &[f32], keywords: &[String]) -> f32 {
    let mut centroid = vec![0.0f32; crate::npae::ory::embeddings::EMBEDDING_DIM];
    for kw in keywords {
        let kw_vec = embed_cached(kw);
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

fn pick_hash_domain_with_confidence(
    scored: &[(String, f32, usize, u32)],
) -> (String, f32, Vec<String>) {
    if scored.is_empty() {
        return ("general".to_string(), 0.30, Vec::new());
    }
    let top = scored[0].1;
    let top_hits = scored[0].2;
    let second = scored.get(1).map(|s| s.1);
    let second_hits = scored.get(1).map(|s| s.2).unwrap_or(0);

    // Accept top domain if it passes the 0.05 similarity margin gate OR has strictly more
    // whole-word keyword hits than the runner-up (with at least 2 hits), OR has a strong
    // co-domain tie where both top and second have >= 2 explicit keyword hits.
    let margin_ok = crate::classifier::intent_rule::hash_margin_allows(top, second, 0.05);
    let lexical_clear_winner = top_hits >= 2 && top_hits > second_hits;
    let multi_domain_strong = top_hits >= 2 && second_hits >= 2 && top >= 0.45;

    if margin_ok || lexical_clear_winner || multi_domain_strong {
        let margin = second.map(|s| (top - s).max(0.0)).unwrap_or(top);
        let conf = (0.55 + (top * 0.25) + (margin * 0.8) + (top_hits.min(4) as f32 * 0.04))
            .clamp(0.55, 0.95);

        // Step 2.3: Collect secondary domains that have explicit keyword hits and score within
        // 0.15 of the primary domain (up to 2 secondary domains).
        let primary = &scored[0].0;
        let secondaries: Vec<String> = scored
            .iter()
            .skip(1)
            .filter(|(dom, sc, hits, _)| {
                dom != primary
                    && !is_redundant_domain_pair(primary, dom)
                    && *hits >= 1
                    && (top - *sc) <= 0.15
            })
            .take(2)
            .map(|(dom, _, _, _)| dom.clone())
            .collect();

        (scored[0].0.clone(), conf, secondaries)
    } else {
        ("general".to_string(), 0.35, Vec::new())
    }
}

fn is_redundant_domain_pair(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("medical", "health")
            | ("health", "medical")
            | ("science", "scientific-research")
            | ("scientific-research", "science")
    )
}

/// Multi-word noun-phrase extraction — extracts the most specific subject from ANY domain.
/// Examples:
///   "Build a Kubernetes autoscaler" → "Kubernetes Autoscaler"
///   "I want to become a real estate agent" → "Real Estate Agent"
///   "Design a meal plan for marathon training" → "Meal Plan"
fn extract_subject(raw: &str) -> Option<String> {
    let stop_words: std::collections::HashSet<&str> = [
        "i",
        "me",
        "my",
        "we",
        "our",
        "you",
        "your",
        "he",
        "she",
        "it",
        "they",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "am",
        "the",
        "a",
        "an",
        "this",
        "that",
        "these",
        "those",
        "how",
        "why",
        "what",
        "when",
        "where",
        "which",
        "who",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "can",
        "may",
        "might",
        "have",
        "has",
        "had",
        "to",
        "for",
        "of",
        "in",
        "on",
        "at",
        "by",
        "with",
        "from",
        "and",
        "or",
        "but",
        "not",
        "if",
        "so",
        "just",
        "also",
        "more",
        "most",
        "very",
        "about",
        "into",
        "some",
        "want",
        "need",
        "like",
        "actually",
        "really",
        "no",
        "non",
        "never",
        "without",
        "avoid",
        "exclude",
        "except",
        "please",
        "only",
        "days",
        "day",
        "hours",
        "hour",
        "since",
        "under",
        "over",
        "hi",
        "hello",
        "hey",
        "there",
        "honestly",
        "pretty",
        "finally",
        "together",
        "years",
        "year",
        "feels",
        "feel",
        "huge",
        "entire",
        "whole",
        "specifically",
        "furthermore",
        "first",
        "second",
        "third",
        "hi",
        "hello",
        "hey",
        "there",
        "honestly",
        "pretty",
        "finally",
        "together",
        "years",
        "year",
        "feels",
        "feel",
        "huge",
        "entire",
        "whole",
        "specifically",
        "furthermore",
        "first",
        "second",
        "third",
    ]
    .iter()
    .copied()
    .collect();

    // Generic/abstract nouns that should never stand alone as a prompt's subject
    // (e.g., "get to this point" -> "Point").
    let generic_nouns: std::collections::HashSet<&str> = [
        "point",
        "thing",
        "things",
        "way",
        "ways",
        "time",
        "times",
        "part",
        "parts",
        "lot",
        "bit",
        "kind",
        "sort",
        "type",
        "case",
        "fact",
        "end",
        "start",
        "beginning",
        "plunge",
        "milestone",
        "life",
        "lives",
        "world",
        "place",
        "area",
        "side",
        "hand",
        "step",
        "steps",
        "detail",
        "details",
        "info",
        "information",
        "everything",
        "anything",
        "something",
        "nothing",
        "everyone",
        "anyone",
        "someone",
        "people",
        "person",
        "stuff",
        "matter",
        "reason",
        "reasons",
        "issue",
        "issues",
        "problem",
        "problems",
        "question",
        "questions",
        "answer",
        "answers",
        "idea",
        "ideas",
        "example",
        "examples",
        "tip",
        "tips",
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

    // Strategy: find noun phrases.
    // Prioritize the first valid phrase immediately following an action verb (the direct object/target),
    // then fall back to the longest non-empty candidate.
    let mut first_post_verb: Option<Vec<String>> = None;
    let mut best_phrase: Vec<String> = Vec::new();
    let mut current_phrase: Vec<String> = Vec::new();
    let mut past_first_verb = false;

    let is_valid_candidate = |phrase: &[String]| -> bool {
        if phrase.is_empty() {
            return false;
        }
        !phrase.iter().all(|w| {
            let wl = w.to_lowercase();
            w.chars()
                .all(|c| c.is_numeric() || c == '$' || c == '%' || !c.is_alphanumeric())
                || w.eq_ignore_ascii_case("no")
                || w.eq_ignore_ascii_case("not")
                || w.eq_ignore_ascii_case("without")
                || w.len() <= 1
                || generic_nouns.contains(wl.as_str())
        })
    };

    for word in &words {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '$');
        if clean.is_empty() {
            continue;
        }
        let lower = clean.to_lowercase();

        // Skip stop words and action verbs — they delimit noun phrases
        if stop_words.contains(lower.as_str()) {
            if is_valid_candidate(&current_phrase) {
                if past_first_verb && first_post_verb.is_none() {
                    first_post_verb = Some(current_phrase.clone());
                }
                if current_phrase.len() >= best_phrase.len() {
                    best_phrase = current_phrase.clone();
                }
            }
            current_phrase.clear();
            continue;
        }

        if action_verbs.contains(lower.as_str()) {
            past_first_verb = true;
            if is_valid_candidate(&current_phrase) {
                if first_post_verb.is_none() {
                    first_post_verb = Some(current_phrase.clone());
                }
                if current_phrase.len() >= best_phrase.len() {
                    best_phrase = current_phrase.clone();
                }
            }
            current_phrase.clear();
            continue;
        }

        // Accumulate noun-phrase words
        if past_first_verb || current_phrase.is_empty() {
            let titled = title_case_word(clean);
            current_phrase.push(titled);
        }
    }

    // Final flush
    if is_valid_candidate(&current_phrase) {
        if past_first_verb && first_post_verb.is_none() {
            first_post_verb = Some(current_phrase.clone());
        }
        if current_phrase.len() >= best_phrase.len() {
            best_phrase = current_phrase;
        }
    }

    // Prefer the first noun phrase right after the verb (the core subject)
    let chosen = first_post_verb.unwrap_or(best_phrase);
    if chosen.is_empty() {
        return None;
    }
    // Filter out invalid phrases that are pure numbers, pure currency, or pure punctuation
    let is_pure_numbers_or_invalid = chosen.iter().all(|w| {
        w.chars()
            .all(|c| c.is_numeric() || c == '$' || c == '%' || !c.is_alphanumeric())
            || w.eq_ignore_ascii_case("no")
            || w.eq_ignore_ascii_case("not")
            || w.eq_ignore_ascii_case("without")
            || w.len() <= 1
    });
    if is_pure_numbers_or_invalid {
        return None;
    }
    let result: Vec<String> = chosen.into_iter().take(4).collect();
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

    // Explicit structured text formats (respecting negations like "no yaml" / "no json")
    let forbids = |fmt: &str| -> bool {
        lower.contains(&format!("no {fmt}"))
            || lower.contains(&format!("without {fmt}"))
            || lower.contains(&format!("avoid {fmt}"))
            || lower.contains(&format!("not {fmt}"))
            || lower.contains(&format!("exclude {fmt}"))
    };

    if (lower.contains("yaml") || lower.contains("yml")) && !forbids("yaml") && !forbids("yml") {
        return "yaml".to_string();
    }
    if lower.contains("csv") && !forbids("csv") {
        return "csv".to_string();
    }
    if lower.contains("toml") && !forbids("toml") {
        return "toml".to_string();
    }

    // Data/API/schema signals → JSON (never when user explicitly forbids JSON)
    let json_signals = [
        "json",
        "api response",
        "schema",
        "payload",
        "endpoint",
        "data structure",
    ];
    if !forbids("json") && json_signals.iter().any(|s| lower.contains(s)) {
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

/// Derive temporal scope from input — replaces hardcoded "immediate".
///
/// // ARCH: ARCHITECTURE.md §1.4 — uses word/phrase-boundary checks (`has_word_or_phrase`) so
/// // incidental words like `"closing day"` or `"for years and years"` do not falsely set
/// // `phased_daily` or `phased_yearly`.
///
/// # Panics
/// Never.
fn detect_temporal_scope(lower: &str) -> (String, bool, bool) {
    // Phase-based signals
    let phase_signals = [
        "phase",
        "phased",
        "phases",
        "stage",
        "stages",
        "sprint",
        "iteration",
        "milestone",
        "step-by-step",
        "entire process",
        "from the very beginning",
    ];
    let has_phases = phase_signals.iter().any(|s| has_word_or_phrase(lower, s));

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
        if has_word_or_phrase(lower, span) {
            detected_scope = Some(span.replace(' ', "_"));
            break;
        }
    }

    // FIX: Replace raw `lower.contains(short)` with strict phrase/word-boundary checks so
    // `"closing day"` or `"for years and years"` do not falsely trigger `"daily"` (`phased_daily`)
    // or `"yearly"`.
    if detected_scope.is_none() {
        let timeline_patterns: &[(&str, &[&str])] = &[
            (
                "daily",
                &[
                    "daily",
                    "day-by-day",
                    "day by day",
                    "per day",
                    "every day",
                    "each day",
                ],
            ),
            (
                "weekly",
                &["weekly", "week-by-week", "per week", "every week", "weeks"],
            ),
            (
                "monthly",
                &[
                    "monthly",
                    "month-by-month",
                    "per month",
                    "every month",
                    "months",
                ],
            ),
            ("quarterly", &["quarterly", "per quarter"]),
            ("yearly", &["yearly", "annual", "annually", "per year"]),
            ("hourly", &["hourly", "per hour"]),
        ];

        for &(scope_label, patterns) in timeline_patterns {
            if patterns.iter().any(|&p| has_word_or_phrase(lower, p)) {
                detected_scope = Some(scope_label.to_string());
                break;
            }
        }
    }

    let has_timeline = detected_scope.is_some() || has_phases;

    let scope = match (has_phases, detected_scope) {
        (true, Some(s)) => format!("phased_{s}"),
        (true, None) => "phased".to_string(),
        (false, Some(s)) => s,
        (false, None)
            if has_word_or_phrase(lower, "long-term") || has_word_or_phrase(lower, "long term") =>
        {
            "long_term".to_string()
        }
        (false, None)
            if has_word_or_phrase(lower, "short-term")
                || has_word_or_phrase(lower, "short term")
                || has_word_or_phrase(lower, "quick") =>
        {
            "short_term".to_string()
        }
        (false, None)
            if has_word_or_phrase(lower, "now")
                || has_word_or_phrase(lower, "immediately")
                || has_word_or_phrase(lower, "asap")
                || has_word_or_phrase(lower, "urgent") =>
        {
            "immediate".to_string()
        }
        (false, None) => "unspecified".to_string(),
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

/// Detect validation/quality assurance signals in the input.
///
/// // FIX: Use `has_word_or_phrase` instead of raw `lower.contains` so incidental substrings
/// // like `"checks"` in `"what a home inspector actually checks"` do not trigger validation gates.
///
/// # Panics
/// Never.
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
    validation_signals
        .iter()
        .any(|s| has_word_or_phrase(lower, s))
}

/// Detect geography from input.
///
/// // FIX: Use `has_word_or_phrase` and explicit US phrases (`"united states"`, `"in the us"`,
/// // `"us market"`, `"usa"`, `"u.s."`) so `"us"` inside `"house"` / `"nervous"` or the English
/// // pronoun `"us"` never falsely sets `Geography: United States`.
///
/// # Panics
/// Never.
fn detect_geography(lower: &str) -> Option<String> {
    if lower.contains("u.s.") {
        return Some("United States".to_string());
    }
    let geos = [
        ("india", "India"),
        ("united states", "United States"),
        ("in the us", "United States"),
        ("us market", "United States"),
        ("us law", "United States"),
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
        if has_word_or_phrase(lower, signal) {
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

/// Checks whether `lower` contains `needle` on word or phrase boundaries.
///
/// Prevents substring false positives such as `"app"` matching `"appraisal"` or `"appreciate"`,
/// or `"earn"` matching `"learn"`.
///
/// # Panics
/// Never.
pub(crate) fn has_word_or_phrase(lower: &str, needle: &str) -> bool {
    let target = needle.trim();
    if target.is_empty() {
        return false;
    }
    let bytes = lower.as_bytes();
    let t_bytes = target.as_bytes();
    if t_bytes.len() > bytes.len() {
        return false;
    }

    let is_word_char = |b: u8| b.is_ascii_alphanumeric();
    let mut start = 0;
    while let Some(rel_idx) = lower[start..].find(target) {
        let idx = start + rel_idx;
        let end = idx + t_bytes.len();
        let left_ok = idx == 0 || !is_word_char(bytes[idx - 1]);
        let right_ok = end == bytes.len() || !is_word_char(bytes[end]);
        if left_ok && right_ok {
            return true;
        }
        start = idx + 1;
        while start < lower.len() && !lower.is_char_boundary(start) {
            start += 1;
        }
    }
    false
}

/// Detects the user's `ActorPerspective` (`Consumer`, `Practitioner`, or `Investor`) within `domain`
/// using word/phrase boundary matching (`has_word_or_phrase`).
///
/// // ARCH: ARCHITECTURE.md §1.4 & §6 — implements the `ActorPerspective` axis of the 3-Axis Intent
/// // Tensor (`Domain x ActorPerspective x IntentClass`).
///
/// # Panics
/// Never.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn detect_actor_perspective(lower: &str, domain: &str) -> ActorPerspective {
    const CONSUMER_SIGNALS: &[&str] = &[
        "buy a house",
        "buying a house",
        "buying a home",
        "buy a home",
        "buy a condo",
        "buy an apartment",
        "buying an apartment",
        "purchase a home",
        "purchase a house",
        "first-time homebuyer",
        "first time homebuyer",
        "homebuyer",
        "home buyer",
        "house hunting",
        "down payment",
        "closing costs",
        "home inspection",
        "my house",
        "my home",
        "my mortgage",
        "my landlord",
        "security deposit",
        "my symptoms",
        "knee pain",
        "back pain",
        "help me learn",
        "want to learn",
        "study for",
        "personal budget",
        "my paycheck",
        "credit card debt",
        "emergency fund",
        "for myself",
        "for my family",
        "family of",
        "for my trip",
        "how do i buy",
        "should i buy",
    ];

    const INVESTOR_SIGNALS: &[&str] = &[
        "cap rate",
        "noi",
        "net operating income",
        "cash-on-cash",
        "dscr",
        "multifamily",
        "brrrr",
        "reit",
        "underwriting",
        "rental property",
        "investment property",
        "flipping houses",
        "cagr",
        "sharpe",
        "asset allocation",
        "wealth gap",
        "net worth",
        "passive income",
        "retire early",
        "valuation multiple",
        "ltv/cac",
    ];

    const PRACTITIONER_SIGNALS: &[&str] = &[
        "become a real estate agent",
        "real estate agent",
        "realtor license",
        "real estate license",
        "pre-licensing",
        "broker sponsorship",
        "start a brokerage",
        "real estate business",
        "real estate agency",
        "client acquisition",
        "get listings",
        "mls",
        "crm",
        "production",
        "microservice",
        "kubernetes",
        "clinical trial",
        "fda",
        "indemnification",
        "redline",
        "syllabus",
        "curriculum",
        "b2b",
        "saas",
        "checkout conversion",
        "roas",
    ];

    let consumer_score = CONSUMER_SIGNALS
        .iter()
        .filter(|&&s| has_word_or_phrase(lower, s))
        .count();
    let investor_score = INVESTOR_SIGNALS
        .iter()
        .filter(|&&s| has_word_or_phrase(lower, s))
        .count();
    let practitioner_score = PRACTITIONER_SIGNALS
        .iter()
        .filter(|&&s| has_word_or_phrase(lower, s))
        .count();

    // Domain-aware tie-breaking:
    // 1. In "real-estate", if any consumer home-buying signal is present and practitioner_score == 0,
    //    return ActorPerspective::Consumer.
    if matches!(domain, "real-estate" | "real estate") && practitioner_score == 0 {
        let explicit_homebuy = [
            "buy a house",
            "buying a house",
            "buying a home",
            "buy a home",
            "homebuyer",
            "first-time",
            "down payment",
            "closing costs",
            "home inspection",
            "house hunting",
        ]
        .iter()
        .any(|&s| has_word_or_phrase(lower, s));

        let has_house_or_home =
            has_word_or_phrase(lower, "house") || has_word_or_phrase(lower, "home");
        let has_buy_or_mortgage = has_word_or_phrase(lower, "buy")
            || has_word_or_phrase(lower, "buying")
            || has_word_or_phrase(lower, "purchase")
            || has_word_or_phrase(lower, "mortgage");
        let has_agent_or_brokerage = [
            "agent",
            "realtor",
            "broker",
            "brokerage",
            "license",
            "licensing",
            "pre-licensing",
        ]
        .iter()
        .any(|&s| has_word_or_phrase(lower, s));

        if explicit_homebuy || (has_house_or_home && has_buy_or_mortgage && !has_agent_or_brokerage)
        {
            return ActorPerspective::Consumer;
        }
    }

    // 2. In "travel" or "nutrition", default to ActorPerspective::Consumer unless practitioner signals appear.
    if matches!(domain, "travel" | "nutrition") {
        let travel_or_nutrition_practitioner = [
            "nutritionist",
            "dietitian",
            "dietician",
            "travel agency",
            "clinical",
        ]
        .iter()
        .any(|&s| has_word_or_phrase(lower, s));
        if travel_or_nutrition_practitioner || practitioner_score > consumer_score {
            return ActorPerspective::Practitioner;
        }
        return ActorPerspective::Consumer;
    }

    if consumer_score > practitioner_score && consumer_score >= investor_score && consumer_score > 0
    {
        return ActorPerspective::Consumer;
    }
    if investor_score > practitioner_score && investor_score > consumer_score {
        return ActorPerspective::Investor;
    }
    ActorPerspective::Practitioner
}

/// Detects the business model or venture type from `lower`, gated by `domain` and `perspective`.
///
/// // ARCH: ARCHITECTURE.md §1.4 & §6 — prevents consumer queries (`ActorPerspective::Consumer`)
/// // and substring false positives (`"appraisal"`, `"apply"`, `"appreciate"` containing `"app"`)
/// // from triggering `"Digital Product"` or enterprise business metadata.
///
/// # Panics
/// Never.
#[must_use]
pub fn detect_business_type(
    lower: &str,
    domain: &str,
    perspective: ActorPerspective,
) -> Option<String> {
    if perspective == ActorPerspective::Consumer {
        return None;
    }

    let is_business_or_tech_domain = matches!(
        domain,
        "business"
            | "business-strategy"
            | "ecommerce"
            | "marketing"
            | "software"
            | "software-engineering"
    );

    let explicit_venture = [
        "saas",
        "software as a service",
        "e-commerce",
        "ecommerce",
        "online store",
        "consulting agency",
        "digital product",
    ]
    .iter()
    .any(|&phrase| has_word_or_phrase(lower, phrase));

    if !is_business_or_tech_domain && !explicit_venture {
        return None;
    }

    if has_word_or_phrase(lower, "saas") || has_word_or_phrase(lower, "software as a service") {
        Some("SaaS".to_string())
    } else if has_word_or_phrase(lower, "e-commerce")
        || has_word_or_phrase(lower, "ecommerce")
        || has_word_or_phrase(lower, "online store")
    {
        Some("E-commerce".to_string())
    } else if has_word_or_phrase(lower, "consulting")
        || has_word_or_phrase(lower, "consulting agency")
        || has_word_or_phrase(lower, "agency")
        || has_word_or_phrase(lower, "freelance")
    {
        Some("Services".to_string())
    } else if has_word_or_phrase(lower, "physical")
        || has_word_or_phrase(lower, "retail")
        || has_word_or_phrase(lower, "brick and mortar")
        || has_word_or_phrase(lower, "retail store")
    {
        Some("Physical/Retail".to_string())
    } else if [
        "mobile app",
        "web app",
        "ios app",
        "android app",
        "app store",
        "digital product",
        "software platform",
    ]
    .iter()
    .any(|&p| has_word_or_phrase(lower, p))
    {
        Some("Digital Product".to_string())
    } else if has_word_or_phrase(lower, "content creator")
        || has_word_or_phrase(lower, "media company")
    {
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
                if matches!(
                    domain,
                    "software"
                        | "software-engineering"
                        | "devops"
                        | "devops-infra"
                        | "ai-ml"
                        | "data-science"
                        | "computers"
                        | "cybersecurity"
                ) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_actor_perspective_real_estate_consumer_vs_practitioner_vs_investor() {
        let consumer_prompt =
            "help me learn and build a step-by-step plan for buying a house in austin with a $500k budget, evaluate mortgage options and neighborhood appraisal comparables";
        assert_eq!(
            detect_actor_perspective(consumer_prompt, "real-estate"),
            ActorPerspective::Consumer
        );

        let practitioner_prompt =
            "how do i become a licensed real estate agent and start a brokerage with mls and crm?";
        assert_eq!(
            detect_actor_perspective(practitioner_prompt, "real-estate"),
            ActorPerspective::Practitioner
        );

        let investor_prompt =
            "analyze multifamily cap rate, noi, cash-on-cash return, and dscr underwriting";
        assert_eq!(
            detect_actor_perspective(investor_prompt, "real-estate"),
            ActorPerspective::Investor
        );
    }

    #[test]
    fn test_detect_business_type_gates_consumer_and_substring_app() {
        let house_prompt =
            "i would really appreciate it if you could help me apply for a mortgage and check neighborhood appraisal comparables for an apartment";
        assert_eq!(
            detect_business_type(house_prompt, "real-estate", ActorPerspective::Consumer),
            None
        );
        assert_eq!(
            detect_business_type(house_prompt, "real-estate", ActorPerspective::Practitioner),
            None
        );
        assert_eq!(
            detect_business_type(house_prompt, "business", ActorPerspective::Practitioner),
            None
        );

        let saas_prompt = "build a b2b saas pricing strategy";
        assert_eq!(
            detect_business_type(saas_prompt, "business", ActorPerspective::Practitioner),
            Some("SaaS".to_string())
        );

        let app_prompt = "launch a mobile app on the app store";
        assert_eq!(
            detect_business_type(app_prompt, "software", ActorPerspective::Practitioner),
            Some("Digital Product".to_string())
        );
    }

    #[test]
    fn test_detect_temporal_scope_avoids_closing_day_false_positive() {
        let homebuy_lower = "we have been saving up for years and years to get to this point, so it feels like a huge milestone in our lives. walk me through the entire process of buying a house from the very beginning and what happens on closing day";
        let (scope, has_timeline, has_phases) = detect_temporal_scope(homebuy_lower);
        assert_eq!(scope, "phased");
        assert!(has_timeline);
        assert!(has_phases);

        let explicit_daily = "create a phased daily meal plan for every day";
        let (daily_scope, _, _) = detect_temporal_scope(explicit_daily);
        assert_eq!(daily_scope, "phased_daily");

        // Ensure "house" / "nervous" / "checks" do not trigger Geography: United States or validation
        let inspector_lower = "i'm pretty nervous to buy a house and want to understand what a home inspector actually checks";
        assert_eq!(detect_geography(inspector_lower), None);
        assert!(!detect_validation_signals(inspector_lower));
        assert_eq!(
            detect_geography("expand into the us market"),
            Some("United States".to_string())
        );
    }
}
