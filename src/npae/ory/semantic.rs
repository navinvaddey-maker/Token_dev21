//! Ory Semantic Analysis Module
//! 
//! NLP-lite semantic analysis for prompt understanding without external dependencies.
//! Provides token classification, action-object extraction, constraint detection,
//! audience identification, and complexity estimation.

use crate::npae::ory::types::{
    TokenAnalysis, ClassifiedToken, TokenClass, ActionObject,
};
use std::collections::HashMap;

/// Perform full semantic analysis on raw input text
pub fn analyze(raw: &str) -> TokenAnalysis {
    let lower = raw.to_lowercase();
    let words: Vec<&str> = raw.split_whitespace().collect();
    
    // 1. Classify each token
    let tokens = classify_tokens(&lower, &words);
    
    // 2. Build clusters by class
    let clusters = build_clusters(&tokens);
    
    // 3. Extract action-object pairs
    let action_objects = extract_action_objects(&tokens);
    
    // 4. Calculate coverage ratio (% non-noise tokens)
    let non_noise = tokens.iter().filter(|t| t.token_class != TokenClass::Noise).count();
    let coverage_ratio = if tokens.is_empty() { 0.0 } else { non_noise as f32 / tokens.len() as f32 };
    
    // 5. Estimate complexity
    let complexity_score = estimate_complexity(&tokens, &clusters, &action_objects);
    
    TokenAnalysis {
        tokens,
        clusters,
        coverage_ratio,
        action_objects,
        complexity_score,
    }
}

/// Classify each token in the prompt into semantic categories
fn classify_tokens(lower: &str, words: &[&str]) -> Vec<ClassifiedToken> {
    let mut tokens = Vec::with_capacity(words.len());
    let lower_words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    
    for (i, word) in words.iter().enumerate() {
        let lw = &lower_words[i];
        let (class, confidence) = classify_single_token(lw, lower, i, &lower_words);
        tokens.push(ClassifiedToken {
            text: word.to_string(),
            token_class: class,
            confidence,
        });
    }
    
    tokens
}

/// Classify a single token with context-awareness
fn classify_single_token(word: &str, _full_lower: &str, position: usize, all_words: &[String]) -> (TokenClass, f32) {
    // Noise/stop words — check first for quick filtering
    if STOP_WORDS.contains(&word) {
        return (TokenClass::Noise, 0.95);
    }
    
    // Action verbs (high confidence)
    if ACTION_VERBS.iter().any(|&v| word == v) {
        return (TokenClass::Action, 0.9);
    }
    
    // Meta-instructions (highest priority after actions)
    if META_SIGNALS.iter().any(|&m| word == m) {
        return (TokenClass::Meta, 0.85);
    }
    // Multi-word meta signals
    if position > 0 {
        let bigram = format!("{} {}", all_words[position - 1], word);
        if META_PHRASES.iter().any(|&p| bigram == p) {
            return (TokenClass::Meta, 0.88);
        }
    }
    
    // Temporal signals
    if TEMPORAL_WORDS.iter().any(|&t| word == t) {
        return (TokenClass::Temporal, 0.88);
    }
    // Temporal patterns (numbers + time units)
    if position + 1 < all_words.len() {
        let next = &all_words[position + 1];
        if word.chars().all(|c| c.is_ascii_digit()) && TIME_UNITS.iter().any(|&u| next == u) {
            return (TokenClass::Temporal, 0.85);
        }
    }
    
    // Audience markers
    if AUDIENCE_WORDS.iter().any(|&a| word == a) {
        return (TokenClass::Audience, 0.85);
    }
    
    // Constraint words
    if CONSTRAINT_WORDS.iter().any(|&c| word == c) {
        return (TokenClass::Constraint, 0.82);
    }
    // Constraint patterns: "no X", "without X", "avoid X", "under N", "within N"
    if position > 0 {
        let prev = &all_words[position - 1];
        if CONSTRAINT_PREFIXES.iter().any(|&p| prev == p) {
            return (TokenClass::Constraint, 0.85);
        }
    }
    
    // Domain signals
    if DOMAIN_WORDS.iter().any(|&d| word == d) {
        return (TokenClass::Domain, 0.8);
    }
    
    // Modifiers (adjectives/adverbs)
    if MODIFIER_WORDS.iter().any(|&m| word == m) {
        return (TokenClass::Modifier, 0.75);
    }
    
    // Subject: nouns that don't fall into other categories
    // Heuristic: words > 3 chars that aren't categorized above are likely subjects
    if word.len() > 3 && word.chars().all(|c| c.is_alphanumeric()) {
        return (TokenClass::Subject, 0.6);
    }
    
    // Default: noise for very short uncategorized words
    (TokenClass::Noise, 0.4)
}

/// Build clusters grouping tokens by their class
fn build_clusters(tokens: &[ClassifiedToken]) -> HashMap<TokenClass, Vec<String>> {
    let mut clusters: HashMap<TokenClass, Vec<String>> = HashMap::new();
    for token in tokens {
        if token.token_class != TokenClass::Noise {
            clusters.entry(token.token_class.clone())
                .or_default()
                .push(token.text.clone());
        }
    }
    clusters
}

/// Extract action-object pairs (verb + nearest noun/subject)
fn extract_action_objects(tokens: &[ClassifiedToken]) -> Vec<ActionObject> {
    let mut pairs = Vec::new();
    
    for (i, token) in tokens.iter().enumerate() {
        if token.token_class == TokenClass::Action {
            // Look for the nearest Subject/Domain token after this action
            for j in (i + 1)..tokens.len().min(i + 5) {
                if tokens[j].token_class == TokenClass::Subject 
                   || tokens[j].token_class == TokenClass::Domain {
                    let confidence = token.confidence * tokens[j].confidence;
                    pairs.push(ActionObject {
                        action: token.text.to_lowercase(),
                        object: tokens[j].text.to_lowercase(),
                        confidence,
                    });
                    break;
                }
            }
        }
    }
    
    pairs
}

/// Estimate request complexity on a 1.0-10.0 scale
fn estimate_complexity(
    tokens: &[ClassifiedToken],
    clusters: &HashMap<TokenClass, Vec<String>>,
    action_objects: &[ActionObject],
) -> f32 {
    let mut score = 1.0_f32;
    
    // Word count contributes to complexity
    let word_count = tokens.len();
    if word_count > 50 { score += 2.0; }
    else if word_count > 20 { score += 1.0; }
    
    // Multiple action-objects indicate multi-step tasks
    score += (action_objects.len() as f32 * 0.8).min(3.0);
    
    // Constraints add complexity
    if let Some(constraints) = clusters.get(&TokenClass::Constraint) {
        score += (constraints.len() as f32 * 0.5).min(2.0);
    }
    
    // Temporal markers indicate phased/timeline work
    if clusters.contains_key(&TokenClass::Temporal) {
        score += 1.0;
    }
    
    // Meta-instructions indicate sophisticated requests
    if let Some(meta) = clusters.get(&TokenClass::Meta) {
        score += (meta.len() as f32 * 0.7).min(1.5);
    }
    
    score.clamp(1.0, 10.0)
}

// ============================================================================
// Vocabulary Tables
// ============================================================================

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "to", "of", "in", "for",
    "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "but", "and", "or",
    "nor", "not", "so", "yet", "both", "either", "neither", "each",
    "every", "all", "any", "few", "more", "most", "other", "some",
    "such", "than", "too", "very", "just", "also", "then", "if",
    "this", "that", "these", "those", "it", "its", "i", "me", "my",
    "we", "our", "you", "your", "he", "she", "they", "them", "their",
];

const ACTION_VERBS: &[&str] = &[
    "build", "create", "make", "implement", "develop", "design", "write",
    "explain", "describe", "elaborate", "clarify", "teach", "illustrate",
    "debug", "fix", "solve", "troubleshoot", "repair", "resolve", "patch",
    "analyze", "evaluate", "assess", "review", "audit", "examine", "investigate",
    "transform", "convert", "refactor", "migrate", "restructure", "optimize",
    "compare", "contrast", "differentiate", "benchmark",
    "plan", "draft", "outline", "prepare", "schedule", "organize",
    "summarize", "condense", "compress", "distill", "extract",
    "list", "identify", "find", "show", "provide", "give", "generate",
    "calculate", "compute", "estimate", "measure", "predict",
    "translate", "interpret", "adapt",
    "test", "validate", "verify", "check",
    "deploy", "launch", "release", "ship",
];

const META_SIGNALS: &[&str] = &[
    "unconventional", "contrarian", "alternative", "innovative",
    "comprehensive", "exhaustive", "thorough", "complete",
    "concise", "brief", "short", "minimal",
    "detailed", "in-depth", "granular",
    "creative", "novel", "unique", "original",
    "practical", "actionable", "applied",
    "theoretical", "conceptual", "abstract",
    "critical", "rigorous", "precise",
];

const META_PHRASES: &[&str] = &[
    "first principles", "from scratch", "at scale", "ground up",
    "step by", "end to", "real world", "best practices",
    "state of", "cutting edge", "deep dive",
];

const TEMPORAL_WORDS: &[&str] = &[
    "daily", "weekly", "monthly", "quarterly", "yearly", "annual",
    "immediately", "urgent", "asap", "now", "today", "tomorrow",
    "deadline", "timeline", "schedule", "roadmap", "milestone",
    "phase", "phases", "stage", "stages", "sprint", "iteration",
    "short-term", "long-term", "overnight",
];

const TIME_UNITS: &[&str] = &[
    "days", "day", "weeks", "week", "months", "month",
    "years", "year", "hours", "hour", "minutes", "minute",
    "quarters", "quarter", "sprints", "sprint",
];

const AUDIENCE_WORDS: &[&str] = &[
    "beginner", "beginners", "novice", "newbie", "starter",
    "intermediate", "mid-level",
    "expert", "advanced", "senior", "experienced", "professional",
    "child", "children", "kid", "kids", "student", "students",
    "developer", "developers", "engineer", "engineers",
    "executive", "executives", "manager", "managers", "leader",
    "team", "stakeholder", "stakeholders",
    "non-technical", "technical", "layperson",
];

const CONSTRAINT_WORDS: &[&str] = &[
    "only", "must", "required", "mandatory", "necessary",
    "maximum", "minimum", "limit", "restrict", "bound",
    "exclude", "except", "unless", "forbidden", "prohibited",
    "budget", "cost", "price", "affordable",
    "free", "open-source", "proprietary",
    "secure", "compliant", "certified",
];

const CONSTRAINT_PREFIXES: &[&str] = &[
    "no", "not", "never", "without", "avoid", "exclude",
    "under", "within", "below", "above", "between", "less",
];

const DOMAIN_WORDS: &[&str] = &[
    // Tech
    "api", "database", "backend", "frontend", "microservice", "kubernetes",
    "docker", "cloud", "server", "algorithm", "architecture", "infrastructure",
    "rust", "python", "javascript", "typescript",
    // Business
    "startup", "revenue", "market", "profit", "investor", "funding",
    "saas", "b2b", "b2c", "growth", "venture", "bootstrap",
    // Science
    "research", "experiment", "hypothesis", "laboratory", "peer-review",
    "quantum", "physics", "biology", "chemistry",
    // Medical
    "medical", "patient", "diagnosis", "treatment", "clinical",
    // Finance
    "investment", "portfolio", "stock", "trading", "banking",
    // AI/ML
    "llm", "transformer", "neural", "alignment", "inference",
    "machine", "learning", "deep", "model", "training",
    // Fitness
    "workout", "exercise", "nutrition", "diet", "marathon",
    "protein", "macro", "calories",
    // Legal
    "contract", "legal", "compliance", "regulation", "patent",
    // Education
    "curriculum", "pedagogy", "assessment", "certification",
    // Marketing
    "campaign", "seo", "brand", "conversion", "funnel",
];

const MODIFIER_WORDS: &[&str] = &[
    "scalable", "fast", "efficient", "reliable", "robust",
    "simple", "complex", "elegant", "clean", "modern",
    "enterprise", "production", "lightweight", "heavy",
    "secure", "performant", "responsive", "intuitive",
    "comprehensive", "minimal", "optimal", "ideal",
    "realistic", "ambitious", "conservative", "aggressive",
    "innovative", "traditional", "proven", "experimental",
    "cost-effective", "high-quality", "low-latency",
    "real-time", "batch", "streaming", "distributed",
    "maintainable", "testable", "extensible", "modular",
];
