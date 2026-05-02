//! Ory Learning Engine v2
//!
//! Deep semantic analysis of raw user prompts. Builds a `LearnedIntent` that captures
//! not just surface-level keywords but semantic intent, hidden dependencies, novel signals,
//! and computed confidence scores.
//!
//! v2 improvements:
//! - Reuses Aggressive engine's 19-domain classifier instead of 3-domain keyword match
//! - Token-level semantic classification via the `semantic` module
//! - Multi-dimensional novel signal detection (constraints, temporal, meta-instructions)
//! - Computed confidence based on token coverage + domain signal strength + ambiguity
//! - Intent fingerprinting for pattern memory matching

use crate::npae::ory::types::{LearnedIntent, TokenClass, TokenAnalysis};
use crate::npae::ory::semantic;
use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct OryLearner;

impl OryLearner {
    /// v2 learning — uses semantic analysis + Aggressive engine domain detection
    pub fn learn(raw: &str) -> Result<LearnedIntent> {
        let lower = raw.to_lowercase();
        
        // 1. Semantic tokenization and analysis
        let analysis = semantic::analyze(raw);
        
        // 2. Extract core objective from action-object pairs
        let core_objective = extract_core_objective_v2(&analysis, &lower);
        
        // 3. Deep domain detection — reuse Aggressive engine's 19-domain classifier
        let inferred_domain = detect_deep_domain_v2(&lower);
        
        // 4. Multi-dimensional novel signal detection
        let novel_signals = identify_novel_signals_v2(&analysis, &lower);
        
        // 5. Context-aware dependency unpacking
        let hidden_dependencies = unpack_dependencies_v2(&lower, &inferred_domain, &analysis);
        
        // 6. Audience detection
        let audience_level = detect_audience(&analysis);
        
        // 7. Temporal markers
        let temporal_markers = extract_temporal_markers(&analysis);
        
        // 8. Constraint phrases
        let constraint_phrases = extract_constraints(&analysis);
        
        // 9. Meta-instructions
        let meta_instructions = extract_meta_instructions(&analysis);
        
        // 10. Computed confidence (not hardcoded!)
        let confidence_score = compute_confidence(&analysis, &inferred_domain);
        
        // 11. Generate fingerprint for pattern matching
        let intent_fingerprint = generate_fingerprint(&core_objective, &inferred_domain, &novel_signals);
        
        Ok(LearnedIntent {
            raw_prompt: raw.to_string(),
            core_objective,
            inferred_domain,
            novel_signals,
            hidden_dependencies,
            confidence_score,
            audience_level,
            temporal_markers,
            constraint_phrases,
            meta_instructions,
            complexity_score: analysis.complexity_score,
            intent_fingerprint,
        })
    }
}

/// v2 objective extraction using semantic action-object pairs
fn extract_core_objective_v2(analysis: &TokenAnalysis, lower: &str) -> String {
    // First try: use action-object pairs for precise objectives
    if let Some(primary) = analysis.action_objects.first() {
        let objective_type = match primary.action.as_str() {
            "build" | "create" | "make" | "implement" | "develop" | "design" | "write" | "generate"
                => "Execution & Construction",
            "explain" | "describe" | "elaborate" | "clarify" | "teach" | "illustrate"
                => "Conceptual Understanding",
            "debug" | "fix" | "solve" | "troubleshoot" | "repair" | "resolve" | "patch"
                => "Problem Analysis",
            "analyze" | "evaluate" | "assess" | "review" | "audit" | "examine" | "investigate"
                => "Analytical Assessment",
            "transform" | "convert" | "refactor" | "migrate" | "restructure" | "optimize"
                => "Transformation & Optimization",
            "plan" | "draft" | "outline" | "prepare" | "schedule" | "organize"
                => "Strategic Planning",
            "compare" | "contrast" | "differentiate" | "benchmark"
                => "Comparative Analysis",
            "summarize" | "condense" | "compress" | "distill" | "extract"
                => "Information Synthesis",
            "test" | "validate" | "verify" | "check"
                => "Quality Verification",
            "deploy" | "launch" | "release" | "ship"
                => "Deployment & Delivery",
            _ => "Information Retrieval",
        };
        return format!("{} — target: {}", objective_type, primary.object);
    }
    
    // Fallback: keyword-based detection with expanded categories
    if lower.contains("roadmap") || lower.contains("strategy") || lower.contains("plan") {
        "Strategic Planning".to_string()
    } else if lower.contains("debug") || lower.contains("fix") || lower.contains("why") {
        "Problem Analysis".to_string()
    } else if lower.contains("build") || lower.contains("create") || lower.contains("implement") {
        "Execution & Construction".to_string()
    } else if lower.contains("explain") || lower.contains("understand") || lower.contains("how") {
        "Conceptual Understanding".to_string()
    } else if lower.contains("compare") || lower.contains("vs") || lower.contains("versus") {
        "Comparative Analysis".to_string()
    } else if lower.contains("analyze") || lower.contains("evaluate") || lower.contains("assess") {
        "Analytical Assessment".to_string()
    } else {
        "Information Retrieval".to_string()
    }
}

/// v2 domain detection — reuses the Aggressive engine's weighted keyword scoring
/// with 19+ domains. This replaces the v1 3-domain keyword check.
fn detect_deep_domain_v2(lower: &str) -> String {
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
        DomainScore { domain: "ai-ml", keywords: &["alignment", "safety", "llm", "transformer", "agi", "general intelligence", "rlhf", "inference", "deep learning"] },
        DomainScore { domain: "medical", keywords: &["medical", "patient", "doctor", "hospital", "surgery", "healthcare", "diagnosis", "treatment", "anatomy", "physiology"] },
        DomainScore { domain: "scientific-research", keywords: &["research", "experiment", "laboratory", "hypothesis", "data analysis", "publication", "peer-review", "methodology"] },
        DomainScore { domain: "cybersecurity", keywords: &["security", "hacking", "firewall", "encryption", "vulnerability", "pentest", "soc", "malware", "threat"] },
        DomainScore { domain: "ecommerce", keywords: &["store", "shopping", "cart", "checkout", "inventory", "warehouse", "logistics", "marketplace"] },
        DomainScore { domain: "real-estate", keywords: &["property", "house", "apartment", "mortgage", "realtor", "appraisal", "listing"] },
        DomainScore { domain: "workplace-productivity", keywords: &["remote", "wfh", "productivity", "culture", "collaboration", "engagement", "performance", "output", "burnout", "hybrid", "distributed team", "work-from-home"] },
        // Additional domains beyond aggressive engine
        DomainScore { domain: "quantum-computing", keywords: &["quantum", "qubit", "superposition", "entanglement", "quantum gate", "decoherence"] },
        DomainScore { domain: "sustainability", keywords: &["sustainability", "carbon", "renewable", "climate", "green", "emissions", "esg", "circular economy"] },
        DomainScore { domain: "gaming", keywords: &["game", "gaming", "player", "level", "multiplayer", "engine", "unity", "unreal"] },
        DomainScore { domain: "psychology", keywords: &["psychology", "cognitive", "behavioral", "therapy", "mental health", "anxiety", "depression", "mindfulness"] },
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

    if best_score >= 1 {
        best_domain.to_string()
    } else {
        "general".to_string()
    }
}

/// v2 novel signal detection — multi-dimensional analysis using semantic tokens
fn identify_novel_signals_v2(analysis: &TokenAnalysis, lower: &str) -> Vec<String> {
    let mut signals = Vec::new();
    
    // 1. Meta-instruction signals from semantic analysis
    if let Some(meta_tokens) = analysis.clusters.get(&TokenClass::Meta) {
        for meta in meta_tokens {
            let signal = match meta.to_lowercase().as_str() {
                "unconventional" | "contrarian" | "alternative" | "innovative"
                    => Some("Non-Standard Methodology"),
                "comprehensive" | "exhaustive" | "thorough" | "complete"
                    => Some("Exhaustive Coverage Required"),
                "concise" | "brief" | "short" | "minimal"
                    => Some("Brevity Constraint"),
                "creative" | "novel" | "unique" | "original"
                    => Some("Creative Approach Required"),
                "critical" | "rigorous" | "precise"
                    => Some("High-Precision Required"),
                _ => None,
            };
            if let Some(s) = signal {
                if !signals.contains(&s.to_string()) {
                    signals.push(s.to_string());
                }
            }
        }
    }
    
    // 2. Scale signals
    if lower.contains("at scale") || lower.contains("millions") || lower.contains("enterprise") {
        signals.push("High-Scale Constraints".to_string());
    }
    
    // 3. First-principles signals
    if lower.contains("first principles") || lower.contains("from scratch") || lower.contains("ground up") {
        signals.push("Foundational Rebuilding".to_string());
    }
    
    // 4. Source/evidence signals
    if lower.contains("peer-reviewed") || lower.contains("cited") || lower.contains("evidence-based") || lower.contains("published") {
        signals.push("Evidence-Based Requirement".to_string());
    }
    
    // 5. Audience-specific signals
    if let Some(audience_tokens) = analysis.clusters.get(&TokenClass::Audience) {
        if audience_tokens.iter().any(|a: &String| {
            let la = a.to_lowercase();
            la == "non-technical" || la == "layperson" || la == "child" || la == "children" || la == "kid" || la == "kids"
        }) {
            signals.push("Non-Expert Audience Adaptation".to_string());
        }
    }
    
    // 6. Temporal complexity signals
    if let Some(temporal_tokens) = analysis.clusters.get(&TokenClass::Temporal) {
        let count = temporal_tokens.len();
        if count >= 2 {
            signals.push("Multi-Phase Timeline".to_string());
        }
    }
    
    // 7. Multi-objective signals
    if analysis.action_objects.len() >= 3 {
        signals.push("Multi-Objective Request".to_string());
    }
    
    // 8. Comparison/trade-off signals
    if lower.contains("trade-off") || lower.contains("tradeoff") || lower.contains("pros and cons") || lower.contains("versus") {
        signals.push("Trade-Off Analysis Required".to_string());
    }
    
    // 9. High complexity signal
    if analysis.complexity_score >= 7.0 {
        signals.push("High Complexity Request".to_string());
    }
    
    signals
}

/// v2 dependency unpacking — uses semantic analysis + expanded domain knowledge
fn unpack_dependencies_v2(lower: &str, domain: &str, analysis: &TokenAnalysis) -> Vec<String> {
    let mut deps = Vec::new();
    
    // Domain-specific dependencies (expanded from v1's 3 domains to 23)
    match domain {
        "sports-nutrition" | "health-fitness" => {
            deps.push("Nutritional Timing".to_string());
            deps.push("Injury Prevention".to_string());
            if lower.contains("marathon") || lower.contains("race") || lower.contains("competition") {
                deps.push("Tapering Strategy".to_string());
                deps.push("Race-Day Nutrition".to_string());
            }
        },
        "business-strategy" => {
            deps.push("Unit Economics".to_string());
            deps.push("Customer Acquisition Cost".to_string());
            deps.push("Scaling Bottlenecks".to_string());
            if lower.contains("startup") || lower.contains("launch") {
                deps.push("Go-To-Market Strategy".to_string());
                deps.push("Fundraising Readiness".to_string());
            }
        },
        "scientific-research" | "quantum-computing" => {
            deps.push("Mathematical Foundations".to_string());
            deps.push("Experimental Verification".to_string());
            deps.push("Literature Review".to_string());
        },
        "software-engineering" | "devops-infra" => {
            deps.push("Architecture Decisions".to_string());
            deps.push("Test Strategy".to_string());
            if lower.contains("scale") || lower.contains("distributed") {
                deps.push("Infrastructure Planning".to_string());
                deps.push("Monitoring & Observability".to_string());
            }
        },
        "ai-ml" | "data-science" => {
            deps.push("Data Quality Assessment".to_string());
            deps.push("Model Selection Criteria".to_string());
            deps.push("Evaluation Metrics".to_string());
            if lower.contains("alignment") || lower.contains("safety") {
                deps.push("Safety Guardrails".to_string());
                deps.push("Bias Assessment".to_string());
            }
        },
        "medical" => {
            deps.push("Clinical Evidence Base".to_string());
            deps.push("Regulatory Compliance".to_string());
            deps.push("Risk-Benefit Analysis".to_string());
        },
        "finance" => {
            deps.push("Risk Assessment Framework".to_string());
            deps.push("Regulatory Requirements".to_string());
            if lower.contains("portfolio") || lower.contains("investment") {
                deps.push("Diversification Strategy".to_string());
            }
        },
        "legal" => {
            deps.push("Jurisdictional Analysis".to_string());
            deps.push("Precedent Research".to_string());
            deps.push("Compliance Requirements".to_string());
        },
        "education" => {
            deps.push("Learning Objectives Definition".to_string());
            deps.push("Assessment Design".to_string());
            deps.push("Cognitive Load Management".to_string());
        },
        "marketing" => {
            deps.push("Target Audience Analysis".to_string());
            deps.push("Channel Strategy".to_string());
            deps.push("Measurement Framework".to_string());
        },
        "cybersecurity" => {
            deps.push("Threat Model".to_string());
            deps.push("Compliance Framework".to_string());
            deps.push("Incident Response Plan".to_string());
        },
        _ => {
            // Generic dependencies based on semantic analysis
            if analysis.complexity_score >= 5.0 {
                deps.push("Requirements Clarification".to_string());
            }
            if analysis.clusters.contains_key(&TokenClass::Temporal) {
                deps.push("Timeline Planning".to_string());
            }
            if analysis.clusters.contains_key(&TokenClass::Constraint) {
                deps.push("Constraint Validation".to_string());
            }
        }
    }
    
    deps
}

/// Detect audience level from semantic tokens
fn detect_audience(analysis: &TokenAnalysis) -> Option<String> {
    if let Some(audience_tokens) = analysis.clusters.get(&TokenClass::Audience) {
        let lower_tokens: Vec<String> = audience_tokens.iter().map(|t: &String| t.to_lowercase()).collect();
        
        if lower_tokens.iter().any(|t: &String| matches!(t.as_str(), "beginner" | "beginners" | "novice" | "newbie" | "starter" | "child" | "children" | "kid" | "kids")) {
            return Some("beginner".to_string());
        }
        if lower_tokens.iter().any(|t: &String| matches!(t.as_str(), "expert" | "advanced" | "senior" | "experienced" | "professional")) {
            return Some("expert".to_string());
        }
        if lower_tokens.iter().any(|t: &String| matches!(t.as_str(), "non-technical" | "layperson")) {
            return Some("non-technical".to_string());
        }
        return Some("intermediate".to_string());
    }
    None
}

/// Extract temporal markers from semantic analysis
fn extract_temporal_markers(analysis: &TokenAnalysis) -> Vec<String> {
    analysis.clusters.get(&TokenClass::Temporal)
        .cloned()
        .unwrap_or_default()
}

/// Extract constraint phrases from semantic analysis
fn extract_constraints(analysis: &TokenAnalysis) -> Vec<String> {
    analysis.clusters.get(&TokenClass::Constraint)
        .cloned()
        .unwrap_or_default()
}

/// Extract meta-instructions from semantic analysis
fn extract_meta_instructions(analysis: &TokenAnalysis) -> Vec<String> {
    analysis.clusters.get(&TokenClass::Meta)
        .cloned()
        .unwrap_or_default()
}

/// Compute confidence score based on actual analysis quality (not hardcoded!)
/// 
/// Dimensions:
/// - Token coverage ratio (40%) — what % of tokens were meaningfully classified
/// - Domain signal strength (30%) — how strongly the domain was identified
/// - Action clarity (20%) — were clear action-object pairs found
/// - Ambiguity penalty (10%) — penalize very short or very vague inputs
fn compute_confidence(analysis: &TokenAnalysis, domain: &str) -> f32 {
    // Token coverage: higher is better
    let coverage_score = analysis.coverage_ratio;
    
    // Domain confidence: "general" is low, specific domains are higher
    let domain_score = if domain == "general" { 0.3 } else { 0.85 };
    
    // Action clarity: having action-object pairs is a strong signal
    let action_score = if analysis.action_objects.is_empty() {
        0.3
    } else {
        let avg_confidence = analysis.action_objects.iter()
            .map(|ao| ao.confidence)
            .sum::<f32>() / analysis.action_objects.len() as f32;
        avg_confidence.min(1.0)
    };
    
    // Ambiguity penalty: very short prompts get penalized
    let token_count = analysis.tokens.len();
    let ambiguity_penalty = if token_count <= 2 { 0.2 }
        else if token_count <= 5 { 0.5 }
        else { 1.0 };
    
    let confidence = (coverage_score * 0.4)
        + (domain_score * 0.3)
        + (action_score * 0.2)
        + (ambiguity_penalty * 0.1);
    
    confidence.clamp(0.05, 0.99)
}

/// Generate a fingerprint hash for pattern memory matching
fn generate_fingerprint(objective: &str, domain: &str, signals: &[String]) -> String {
    let mut hasher = DefaultHasher::new();
    objective.hash(&mut hasher);
    domain.hash(&mut hasher);
    for signal in signals {
        signal.hash(&mut hasher);
    }
    format!("ory-{:016x}", hasher.finish())
}
