//! Unified configuration loader and domain taxonomy structures for NPAE and Stage 4 schema resolution.
//!
//! Implements `ARCHITECTURE.md` §1.4 (Scenario Classification & Taxonomy Fallback) and §6 (Unified Config-Backed `SchemaRegistry`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::npae::schema::types::ExecutionPhase;

/// Top-level unified configuration deserialized from `config/unified.json`.
///
/// # Invariants
/// Contains canonical domain taxonomies, role scoring rules, and constraint triggers shared across
/// `AggressiveEngine`, `PipelineOrchestrator`, and `SchemaRegistry`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UnifiedConfig {
    /// Canonical domain definitions, keywords, templates, and aliases.
    pub domain_taxonomy: Vec<DomainTaxonomy>,
    /// Role scoring rules weighted by action verbs and domain priority.
    pub roles: Vec<RoleRule>,
    /// Constraint extraction rules with weighted trigger phrases.
    pub constraints: Vec<ConstraintRule>,
}

/// Canonical domain entry in `config/unified.json` defining keywords, aliases, persona, phases, and Stage 4 schema templates.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DomainTaxonomy {
    /// Canonical domain identifier (e.g., `"software"`, `"finance"`, `"legal"`).
    pub domain: String,
    /// Domain signal keywords used by intent and centroid classifiers.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Priority boost applied during role and domain scoring.
    #[serde(default)]
    pub boost: u32,
    /// Alternate names (e.g. legacy hardcoded strings or `"tech"`) that canonicalize to `domain`.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Optional persona template with `{role}` placeholder for domain-specific role framing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_template: Option<String>,
    /// Optional multi-phase execution blueprints keyed by intent (`"build"`, `"analyze"`, `"plan"`, `"default"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_templates: Option<HashMap<String, Vec<ExecutionPhase>>>,
    /// Optional canonical context dimensions for Stage 4 `CompressionSchema` resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_fields: Option<Vec<String>>,
    /// Optional task templates keyed by task type (`"creation"`, `"analysis"`, `"planning"`, `"troubleshooting"`, `"optimization"`, `"default"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_templates: Option<HashMap<String, String>>,
    /// Optional default deliverables keyed by task type (`"creation"`, `"analysis"`, `"planning"`, `"troubleshooting"`, `"default"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_deliverables: Option<HashMap<String, Vec<String>>>,
}

impl Default for DomainTaxonomy {
    fn default() -> Self {
        Self {
            domain: String::new(),
            keywords: Vec::new(),
            boost: 1,
            aliases: Vec::new(),
            persona_template: None,
            phase_templates: None,
            context_fields: None,
            task_templates: None,
            default_deliverables: None,
        }
    }
}

/// Maps any alias or legacy domain name (case-insensitively) to the canonical domain name defined in `unified.json`.
///
/// Built-in fallback aliases (such as `"tech"` -> `"software"`) are also resolved when `taxonomy` is empty
/// or does not explicitly list the alias, ensuring consistent canonical domain naming across all stages.
///
/// # Panics
/// Never panics.
#[must_use]
pub fn normalize_domain<'a>(name: &'a str, taxonomy: &'a [DomainTaxonomy]) -> &'a str {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return name;
    }

    // ARCH: §1.4 & §6 — Case-insensitive canonical domain and alias resolution against `unified.json`.
    for tax in taxonomy {
        if tax.domain.eq_ignore_ascii_case(trimmed) {
            return tax.domain.as_str();
        }
        if tax.aliases.iter().any(|a| a.eq_ignore_ascii_case(trimmed)) {
            return tax.domain.as_str();
        }
    }

    // Built-in canonical alias table when `taxonomy` is minimal (e.g. unit tests) or omits a legacy alias.
    if trimmed.eq_ignore_ascii_case("tech")
        || trimmed.eq_ignore_ascii_case("software-engineering")
        || trimmed.eq_ignore_ascii_case("web-development")
        || trimmed.eq_ignore_ascii_case("technical-writing")
        || trimmed.eq_ignore_ascii_case("coding")
        || trimmed.eq_ignore_ascii_case("programming")
    {
        return "software";
    }
    if trimmed.eq_ignore_ascii_case("computer-science")
        || trimmed.eq_ignore_ascii_case("computing")
        || trimmed.eq_ignore_ascii_case("it")
        || trimmed.eq_ignore_ascii_case("systems-architecture")
    {
        return "computers";
    }
    if trimmed.eq_ignore_ascii_case("scientific-research")
        || trimmed.eq_ignore_ascii_case("natural-sciences")
        || trimmed.eq_ignore_ascii_case("physics")
        || trimmed.eq_ignore_ascii_case("chemistry")
        || trimmed.eq_ignore_ascii_case("biology")
    {
        return "science";
    }
    if trimmed.eq_ignore_ascii_case("healthcare")
        || trimmed.eq_ignore_ascii_case("wellness")
        || trimmed.eq_ignore_ascii_case("public-health")
        || trimmed.eq_ignore_ascii_case("health-fitness")
    {
        return "health";
    }
    if trimmed.eq_ignore_ascii_case("personal-finance")
        || trimmed.eq_ignore_ascii_case("wealth-management")
        || trimmed.eq_ignore_ascii_case("financial-planning")
        || trimmed.eq_ignore_ascii_case("investing")
    {
        return "finance";
    }
    if trimmed.eq_ignore_ascii_case("business-strategy")
        || trimmed.eq_ignore_ascii_case("career-growth")
        || trimmed.eq_ignore_ascii_case("monetization")
    {
        return "business";
    }
    if trimmed.eq_ignore_ascii_case("sports-nutrition")
        || trimmed.eq_ignore_ascii_case("clinical-nutrition")
        || trimmed.eq_ignore_ascii_case("dietetics")
    {
        return "nutrition";
    }
    if trimmed.eq_ignore_ascii_case("devops-infra")
        || trimmed.eq_ignore_ascii_case("platform-engineering")
    {
        return "devops";
    }
    if trimmed.eq_ignore_ascii_case("data-analytics")
        || trimmed.eq_ignore_ascii_case("machine-learning")
    {
        return "data-science";
    }
    if trimmed.eq_ignore_ascii_case("creative-writing")
        || trimmed.eq_ignore_ascii_case("content-writing")
    {
        return "creative";
    }
    if trimmed.eq_ignore_ascii_case("lifestyle-travel")
        || trimmed.eq_ignore_ascii_case("tourism")
        || trimmed.eq_ignore_ascii_case("itinerary-planning")
    {
        return "travel";
    }
    if trimmed.eq_ignore_ascii_case("real estate") {
        return "real-estate";
    }

    name
}

/// Role rule mapping domain and weighted action verbs to a base professional title.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleRule {
    /// Priority tier (higher values take precedence when verb weights tie).
    pub priority: u32,
    /// Associated canonical domain name.
    pub domain: String,
    /// Base role title (e.g. `"Systems Architect"`, `"Legal Strategist"`).
    pub base_title: String,
    /// Weighted action/signal words that trigger this role.
    pub verbs: Vec<WeightedWord>,
}

/// Constraint extraction rule triggered by weighted token patterns.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstraintRule {
    /// Unique identifier for the constraint rule (e.g. `"no_gluten"`, `"gdpr_compliant"`).
    pub id: String,
    /// Canonical domain this constraint belongs to.
    pub domain: String,
    /// Priority score for ranking extracted constraints.
    pub priority: u32,
    /// True if the constraint represents an exclusion/prohibition.
    pub is_forbidden: bool,
    /// Human-readable description of the constraint.
    pub description: String,
    /// Disjunctive list of conjunctive weighted word triggers.
    pub trigger: Vec<Vec<WeightedWord>>,
}

/// A weighted keyword and optional synonyms used in role and constraint rules.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeightedWord {
    /// Primary trigger word.
    pub word: String,
    /// Match weight.
    pub weight: u32,
    /// Optional list of synonym strings that match with the same weight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<Vec<String>>,
}

/// Thread-safe configuration handle backed by `Arc<RwLock<UnifiedConfig>>`.
///
/// # Panics
/// Never panics: recovers gracefully from poisoned `RwLock` states using `into_inner()`.
#[derive(Debug, Clone)]
pub struct ConfigHandle {
    inner: Arc<RwLock<UnifiedConfig>>,
}

impl ConfigHandle {
    /// Creates a new `ConfigHandle` wrapping `initial`.
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn new(initial: UnifiedConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(initial)),
        }
    }

    /// Returns a cloned snapshot of the current `UnifiedConfig`.
    ///
    /// # Panics
    /// Never panics (recovers from poisoned lock via `into_inner`).
    #[must_use]
    pub fn read(&self) -> UnifiedConfig {
        // FIX: Recover inner config if RwLock is poisoned instead of panicking via unwrap() (nrule.md §6).
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Atomically swaps the active `UnifiedConfig` with `new_config`.
    ///
    /// # Panics
    /// Never panics (recovers from poisoned lock via `into_inner`).
    pub fn swap(&self, new_config: UnifiedConfig) {
        // FIX: Recover write guard if RwLock is poisoned instead of panicking via unwrap() (nrule.md §6).
        let mut lock = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = new_config;
    }
}

/// File-system loader and serializer for `UnifiedConfig` (`config/unified.json`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Loads and deserializes `UnifiedConfig` from the given JSON file path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or contains invalid JSON.
    ///
    /// # Panics
    /// Never panics.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<UnifiedConfig> {
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!("Failed to read config file at {}", path.as_ref().display())
        })?;
        let config: UnifiedConfig =
            serde_json::from_str(&content).with_context(|| "Failed to parse unified.json")?;

        Ok(config)
    }

    /// Serializes and writes `UnifiedConfig` as pretty-printed JSON to `path`.
    ///
    /// # Errors
    /// Returns an error if JSON serialization or file writing fails.
    ///
    /// # Panics
    /// Never panics.
    pub fn save<P: AsRef<Path>>(path: P, config: &UnifiedConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
