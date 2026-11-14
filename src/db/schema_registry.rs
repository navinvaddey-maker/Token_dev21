//! Unified Config-Backed `SchemaRegistry` (`ARCHITECTURE.md` §1.4, §2, §6).
//!
//! Resolves `CompressionSchema` directly in-memory from `config/unified.json` (`ConfigHandle`)
//! using a 3-tier resolution strategy (`Domain+TaskType+Expertise` -> `Domain+TaskType` -> `General fallback`)
//! and supporting 3-Axis Intent Tensor `(Domain × ActorPerspective × IntentClass)` perspective-prefixed
//! keys (`"consumer:build"`, `"practitioner:build"`, `"investor:analyze"`, etc.) with zero SQLite queries.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use once_cell::sync::Lazy;
use sqlx::SqlitePool;

use crate::npae::aggressive::config::{
    normalize_domain, ConfigHandle, ConfigLoader, DomainTaxonomy, UnifiedConfig,
};
use crate::types::{CompressionSchema, Constraint, Deliverable};

/// Lazily initialized default `ConfigHandle` loaded once from `config/unified.json`.
///
/// Ensures `SchemaRegistry::without_pool()`, `SchemaRegistry::new(...)`,
/// `SchemaRegistry::with_optional_pool(...)`, and any `ConfigHandle` initialized with an empty
/// test `domain_taxonomy` still resolve canonical domain schemas in-memory without a database pool.
#[allow(clippy::non_std_lazy_statics)]
static DEFAULT_CONFIG_HANDLE: Lazy<Arc<ConfigHandle>> = Lazy::new(|| {
    let config = ConfigLoader::load("config/unified.json")
        .or_else(|_| {
            serde_json::from_str::<UnifiedConfig>(include_str!("../../config/unified.json"))
        })
        .unwrap_or_default();
    Arc::new(ConfigHandle::new(config))
});

/// Key used to look up the appropriate `CompressionSchema` template for a given request.
#[derive(Debug, Clone)]
pub struct SchemaLookupKey {
    /// Canonical or alias domain name (e.g. `"software"`, `"tech"`, `"legal"`, `"finance"`).
    pub domain: String,
    /// Task category (e.g. `"creation"`, `"analysis"`, `"planning"`, `"troubleshooting"`, `"optimization"`).
    pub task_type: String,
    /// User expertise level (e.g. `"expert"`, `"advanced"`, `"intermediate"`, `"beginner"`).
    pub expertise: String,
}

/// Provides zero-DB, in-memory `CompressionSchema` resolution backed by `ConfigHandle` (`config/unified.json`).
///
/// # Invariants
/// - Never executes SQLite queries; `_pool` is retained solely for API backward compatibility.
/// - Always falls back to `DEFAULT_CONFIG_HANDLE` (`config/unified.json`) when a custom `ConfigHandle`
///   has an empty `domain_taxonomy`.
#[derive(Debug, Clone)]
pub struct SchemaRegistry {
    /// Retained for backward compatibility with callers passing `SqlitePool`.
    _pool: Option<SqlitePool>,
    /// Shared configuration handle backing 3-tier schema resolution.
    config: Arc<ConfigHandle>,
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::without_pool()
    }
}

impl SchemaRegistry {
    /// Creates a `SchemaRegistry` retaining `pool` for signature compatibility while resolving schemas from `config/unified.json`.
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            _pool: Some(pool),
            config: Arc::clone(&DEFAULT_CONFIG_HANDLE),
        }
    }

    /// Creates a `SchemaRegistry` backed by the default `config/unified.json` handle without a database pool.
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn without_pool() -> Self {
        Self {
            _pool: None,
            config: Arc::clone(&DEFAULT_CONFIG_HANDLE),
        }
    }

    /// Creates a `SchemaRegistry` with an optional database pool, resolving schemas in-memory from `config/unified.json`.
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn with_optional_pool(pool: Option<SqlitePool>) -> Self {
        Self {
            _pool: pool,
            config: Arc::clone(&DEFAULT_CONFIG_HANDLE),
        }
    }

    /// Creates a `SchemaRegistry` backed by an explicit `Arc<ConfigHandle>`.
    ///
    /// If the provided `config` has an empty `domain_taxonomy` (e.g., in minimal unit tests),
    /// lookups automatically fall back to `DEFAULT_CONFIG_HANDLE` (`config/unified.json`).
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn with_config(config: Arc<ConfigHandle>) -> Self {
        Self {
            _pool: None,
            config,
        }
    }

    /// Returns the active `UnifiedConfig`, falling back to `DEFAULT_CONFIG_HANDLE` when `domain_taxonomy` is empty.
    fn active_config(&self) -> UnifiedConfig {
        let cfg = self.config.read();
        if cfg.domain_taxonomy.is_empty() {
            DEFAULT_CONFIG_HANDLE.read()
        } else {
            cfg
        }
    }

    /// Tier 1: Exact match on canonicalized `domain` + `task_type` + `expertise`.
    ///
    /// Returns `Some(CompressionSchema)` when:
    /// 1. `key.domain` resolves to a known canonical domain in `domain_taxonomy` (not `"general"`),
    /// 2. `key.task_type` matches an explicit task/phase template for that domain, and
    /// 3. `key.expertise` is a recognized expertise tier (`"expert"`, `"advanced"`, `"intermediate"`, `"beginner"`).
    ///
    /// # Panics
    /// Never panics.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn find_exact_schema_match(
        &self,
        key: &SchemaLookupKey,
    ) -> Option<CompressionSchema> {
        let cfg = self.active_config();
        let canonical_domain = normalize_domain(&key.domain, &cfg.domain_taxonomy);
        if canonical_domain.eq_ignore_ascii_case("general") || canonical_domain.is_empty() {
            return None;
        }

        let tax = cfg
            .domain_taxonomy
            .iter()
            .find(|t| t.domain.eq_ignore_ascii_case(canonical_domain))?;

        let (perspective, raw_task) = parse_perspective_and_task(&key.task_type);
        let norm_task = normalize_task_type(raw_task)?;
        let norm_expertise = normalize_expertise(&key.expertise)?;

        // Ensure the domain actually has a template for this specific (perspective, task_type)
        if !has_explicit_task_or_phase_template(tax, perspective, norm_task) {
            return None;
        }

        Some(build_domain_schema(
            &cfg,
            tax,
            perspective,
            norm_task,
            Some(norm_expertise),
        ))
    }

    /// Tier 2: Domain + `task_type` match, ignoring expertise (and falling back to `"default"` task templates for known domains).
    ///
    /// Supports both standard task types (`"creation"`, `"analysis"`, `"planning"`) and
    /// perspective-prefixed task keys (`"consumer:build"`, `"practitioner:build"`, `"investor:analyze"`).
    ///
    /// # Panics
    /// Never panics.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn find_domain_level_schema(
        &self,
        domain: &str,
        task_type: &str,
    ) -> Option<CompressionSchema> {
        let cfg = self.active_config();
        let canonical_domain = normalize_domain(domain, &cfg.domain_taxonomy);
        if canonical_domain.eq_ignore_ascii_case("general") || canonical_domain.is_empty() {
            return None;
        }

        let tax = cfg
            .domain_taxonomy
            .iter()
            .find(|t| t.domain.eq_ignore_ascii_case(canonical_domain))?;

        let (perspective, raw_task) = parse_perspective_and_task(task_type);
        let norm_task = normalize_task_type(raw_task).unwrap_or("default");
        Some(build_domain_schema(&cfg, tax, perspective, norm_task, None))
    }

    /// Tier 3: General fallback template when domain is `"general"` or unrecognized.
    ///
    /// # Panics
    /// Never panics.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn get_general_fallback_schema(&self) -> CompressionSchema {
        // ARCH: §6 — Deterministic in-memory general fallback schema (zero SQLite queries).
        CompressionSchema {
            role: Some("Domain Expert".to_string()),
            context: Some("general_context".to_string()),
            task: Some(
                "Deliver a structured, accurate, and actionable response addressing the core objective."
                    .to_string(),
            ),
            constraints: vec![Constraint {
                name: "preserve_hard_constraints".to_string(),
            }],
            output: vec![
                Deliverable {
                    name: "structured_response".to_string(),
                },
                Deliverable {
                    name: "actionable_steps".to_string(),
                },
            ],
        }
    }

    /// Unified 3-tier entry point used by Stage 4 (`Tier 1 -> Tier 2 -> Tier 3`).
    ///
    /// # Panics
    /// Never panics.
    pub async fn resolve_schema_for_signal(&self, key: &SchemaLookupKey) -> CompressionSchema {
        if let Some(schema) = self.find_exact_schema_match(key).await {
            return schema;
        }
        if let Some(schema) = self
            .find_domain_level_schema(&key.domain, &key.task_type)
            .await
        {
            return schema;
        }
        self.get_general_fallback_schema().await
    }
}

/// Splits an optional actor perspective prefix (`"consumer:"`, `"practitioner:"`, `"investor:"`)
/// from a task or phase key.
///
/// For example:
/// - `"consumer:build"` -> `(Some("consumer"), "build")`
/// - `"practitioner:creation"` -> `(Some("practitioner"), "creation")`
/// - `"investor:analyze"` -> `(Some("investor"), "analyze")`
/// - `"consumer"` -> `(Some("consumer"), "creation")`
/// - `"creation"` -> `(None, "creation")`
fn parse_perspective_and_task(task_type: &str) -> (Option<&'static str>, &str) {
    let trimmed = task_type.trim();
    if let Some((prefix, rest)) = trimmed.split_once(':') {
        let p = match prefix.trim().to_ascii_lowercase().as_str() {
            "consumer" | "buyer" | "patient" | "student" | "citizen" => Some("consumer"),
            "practitioner" | "agent" | "professional" | "clinician" | "counsel" => {
                Some("practitioner")
            }
            "investor" | "underwriter" | "allocator" => Some("investor"),
            _ => None,
        };
        if p.is_some() {
            let task_part = rest.trim();
            return (
                p,
                if task_part.is_empty() {
                    "creation"
                } else {
                    task_part
                },
            );
        }
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "consumer" => (Some("consumer"), "creation"),
        "practitioner" => (Some("practitioner"), "creation"),
        "investor" => (Some("investor"), "analysis"),
        _ => (None, trimmed),
    }
}

/// Checks whether `tax` defines an explicit task or phase template for `(perspective, norm_task)`.
fn has_explicit_task_or_phase_template(
    tax: &DomainTaxonomy,
    perspective: Option<&str>,
    norm_task: &str,
) -> bool {
    let phase_key = task_type_to_phase_key(norm_task);

    if let Some(p) = perspective {
        let comp_task = format!("{p}:{norm_task}");
        let comp_phase = format!("{p}:{phase_key}");
        if tax
            .task_templates
            .as_ref()
            .is_some_and(|m| m.contains_key(&comp_task) || m.contains_key(&comp_phase))
        {
            return true;
        }
        if tax
            .phase_templates
            .as_ref()
            .is_some_and(|m| m.contains_key(&comp_phase) || m.contains_key(p))
        {
            return true;
        }
    }

    tax.task_templates
        .as_ref()
        .is_some_and(|m| m.contains_key(norm_task))
        || tax.phase_templates.as_ref().is_some_and(|m| {
            m.contains_key(phase_key)
                || m.contains_key(&format!("consumer:{phase_key}"))
                || m.contains_key(&format!("practitioner:{phase_key}"))
                || m.contains_key(&format!("investor:{phase_key}"))
        })
}

/// Normalizes task type aliases into canonical `task_templates` keys.
fn normalize_task_type(task_type: &str) -> Option<&'static str> {
    let lower = task_type.trim().to_ascii_lowercase();
    match lower.as_str() {
        "creation" | "create" | "build" | "generation" | "design" | "implement"
        | "implementation" | "draft" => Some("creation"),
        "analysis" | "analyze" | "evaluate" | "evaluation" | "audit" | "review" | "compare" => {
            Some("analysis")
        }
        "planning" | "plan" | "roadmap" | "strategy" | "schedule" => Some("planning"),
        "troubleshooting" | "debug" | "debugging" | "fix" | "incident" | "diagnosis" => {
            Some("troubleshooting")
        }
        "optimization" | "optimize" | "refactor" | "transform" | "tuning" | "improve" => {
            Some("optimization")
        }
        "explanation" | "explain" | "teach" | "summary" | "default" | "general" => Some("default"),
        _ => None,
    }
}

/// Maps a normalized task type key to a `phase_templates` lookup key.
fn task_type_to_phase_key(norm_task: &str) -> &'static str {
    match norm_task {
        "creation" | "planning" => "build",
        "analysis" => "analyze",
        _ => "default",
    }
}

/// Normalizes user expertise strings into canonical expertise levels.
fn normalize_expertise(expertise: &str) -> Option<&'static str> {
    let lower = expertise.trim().to_ascii_lowercase();
    match lower.as_str() {
        "expert" | "principal" | "staff" | "senior" => Some("expert"),
        "advanced" | "proficient" => Some("advanced"),
        "intermediate" | "mid" | "standard" | "general" => Some("intermediate"),
        "beginner" | "novice" | "junior" | "introductory" => Some("beginner"),
        _ => None,
    }
}

/// Builds a rich `CompressionSchema` from `UnifiedConfig` for a matched `DomainTaxonomy`.
fn build_domain_schema(
    cfg: &UnifiedConfig,
    tax: &DomainTaxonomy,
    perspective: Option<&str>,
    norm_task: &str,
    expertise: Option<&str>,
) -> CompressionSchema {
    let role_str = resolve_domain_role(cfg, tax, perspective, expertise);
    let context = tax
        .context_fields
        .as_ref()
        .filter(|f| !f.is_empty())
        .map(|fields| fields.join(" "))
        .or_else(|| Some(format!("{}_context", tax.domain)));

    let phase_key = task_type_to_phase_key(norm_task);
    let task = tax
        .task_templates
        .as_ref()
        .and_then(|templates| {
            if let Some(p) = perspective {
                if let Some(t) = templates
                    .get(&format!("{p}:{norm_task}"))
                    .or_else(|| templates.get(&format!("{p}:{phase_key}")))
                    .or_else(|| templates.get(p))
                {
                    return Some(t.clone());
                }
            }
            templates
                .get(norm_task)
                .or_else(|| templates.get("default"))
                .cloned()
        })
        .or_else(|| {
            tax.persona_template
                .as_ref()
                .map(|tpl| tpl.replace("{role}", &role_str))
        });

    let constraints = resolve_domain_constraints(cfg, tax);
    let output = resolve_domain_deliverables(tax, perspective, norm_task);

    CompressionSchema {
        role: Some(role_str),
        context,
        task,
        constraints,
        output,
    }
}

/// Resolves the highest-priority role title for a domain, modulated by optional actor perspective and expertise level.
fn resolve_domain_role(
    cfg: &UnifiedConfig,
    tax: &DomainTaxonomy,
    perspective: Option<&str>,
    expertise: Option<&str>,
) -> String {
    let base_role = match (perspective, tax.domain.as_str()) {
        (Some("consumer"), "real-estate") => {
            "Residential Home-Buying & Mortgage Advisor".to_string()
        }
        (Some("consumer"), "legal") => "Consumer Legal Rights & Dispute Advisor".to_string(),
        (Some("consumer"), "medical" | "health") => {
            "Clinical Health & Patient Care Educator".to_string()
        }
        (Some("consumer"), "finance") => "Personal Finance & Household Budget Advisor".to_string(),
        (Some("consumer"), "education") => "Personal Learning & Mastery Coach".to_string(),
        (Some("investor"), "real-estate") => {
            "Real Estate Investment & Underwriting Strategist".to_string()
        }
        _ => {
            let mut matching_roles: Vec<_> = cfg
                .roles
                .iter()
                .filter(|r| r.domain.eq_ignore_ascii_case(&tax.domain))
                .collect();
            matching_roles.sort_by_key(|r| Reverse(r.priority));

            matching_roles.first().map_or_else(
                || default_domain_role_title(&tax.domain),
                |r| r.base_title.clone(),
            )
        }
    };

    if perspective == Some("consumer") {
        return match expertise {
            Some("beginner") => format!("{base_role} (Guided Mode)"),
            _ => base_role,
        };
    }

    match expertise {
        Some("expert" | "advanced") => {
            if base_role.starts_with("Principal ")
                || base_role.starts_with("Senior ")
                || base_role.starts_with("Lead ")
                || base_role.starts_with("Staff ")
                || base_role.starts_with("Board-Certified ")
                || base_role.starts_with("Registered ")
            {
                base_role
            } else {
                format!("Principal {base_role}")
            }
        }
        Some("beginner") => format!("{base_role} (Guided Mode)"),
        _ => base_role,
    }
}

/// Collects up to 3 priority-sorted domain constraints from `UnifiedConfig`.
fn resolve_domain_constraints(cfg: &UnifiedConfig, tax: &DomainTaxonomy) -> Vec<Constraint> {
    let mut matching_rules: Vec<_> = cfg
        .constraints
        .iter()
        .filter(|c| c.domain.eq_ignore_ascii_case(&tax.domain))
        .collect();
    matching_rules.sort_by_key(|a| Reverse(a.priority));

    let mut seen_constraints = HashSet::new();
    let mut constraints = Vec::new();
    for rule in matching_rules {
        let key = rule.id.to_ascii_lowercase();
        if seen_constraints.insert(key) {
            constraints.push(Constraint {
                name: rule.id.clone(),
            });
            if constraints.len() >= 3 {
                break;
            }
        }
    }
    if constraints.is_empty() {
        if let Some(first_ctx) = tax.context_fields.as_ref().and_then(|f| f.first()) {
            constraints.push(Constraint {
                name: first_ctx.clone(),
            });
        }
    }
    constraints
}

/// Collects up to 3 concise deliverables for `(tax, perspective, norm_task)` from `default_deliverables` or `phase_templates`.
fn resolve_domain_deliverables(
    tax: &DomainTaxonomy,
    perspective: Option<&str>,
    norm_task: &str,
) -> Vec<Deliverable> {
    let mut output = Vec::new();
    let mut seen_deliverables = HashSet::new();
    let phase_key = task_type_to_phase_key(norm_task);

    // 1. When an explicit actor perspective is present, check perspective-keyed deliverables and phase_templates first.
    if let Some(p) = perspective {
        let comp_task = format!("{p}:{norm_task}");
        let comp_phase = format!("{p}:{phase_key}");

        if let Some(ref deliv_map) = tax.default_deliverables {
            if let Some(items) = deliv_map
                .get(&comp_task)
                .or_else(|| deliv_map.get(&comp_phase))
                .or_else(|| deliv_map.get(p))
            {
                for item in items {
                    let lower = item.to_ascii_lowercase();
                    if seen_deliverables.insert(lower) {
                        output.push(Deliverable { name: item.clone() });
                        if output.len() >= 3 {
                            return output;
                        }
                    }
                }
            }
        }

        if let Some(ref phases_map) = tax.phase_templates {
            if let Some(phases) = phases_map.get(&comp_phase).or_else(|| phases_map.get(p)) {
                for phase in phases {
                    for deliv in &phase.deliverables {
                        let lower = deliv.to_ascii_lowercase();
                        if seen_deliverables.insert(lower) {
                            output.push(Deliverable {
                                name: deliv.clone(),
                            });
                            if output.len() >= 3 {
                                return output;
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Standard task_type lookup in default_deliverables.
    if let Some(ref deliv_map) = tax.default_deliverables {
        if let Some(items) = deliv_map
            .get(norm_task)
            .or_else(|| deliv_map.get("default"))
        {
            for item in items {
                let lower = item.to_ascii_lowercase();
                if seen_deliverables.insert(lower) {
                    output.push(Deliverable { name: item.clone() });
                    if output.len() >= 3 {
                        break;
                    }
                }
            }
        }
    }

    // 3. Fallback to phase_templates (including perspective-prefixed keys if standard keys are absent).
    if output.is_empty() {
        if let Some(ref phases_map) = tax.phase_templates {
            if let Some(phases) = phases_map
                .get(phase_key)
                .or_else(|| phases_map.get(&format!("consumer:{phase_key}")))
                .or_else(|| phases_map.get(&format!("practitioner:{phase_key}")))
                .or_else(|| phases_map.get(&format!("investor:{phase_key}")))
                .or_else(|| phases_map.get("build"))
                .or_else(|| phases_map.get("consumer:build"))
                .or_else(|| phases_map.get("practitioner:build"))
                .or_else(|| phases_map.get("default"))
            {
                for phase in phases {
                    for deliv in &phase.deliverables {
                        let lower = deliv.to_ascii_lowercase();
                        if seen_deliverables.insert(lower) {
                            output.push(Deliverable {
                                name: deliv.clone(),
                            });
                            if output.len() >= 3 {
                                break;
                            }
                        }
                    }
                    if output.len() >= 3 {
                        break;
                    }
                }
            }
        }
    }
    output
}

/// Fallback role title when a domain has no explicit entry in `roles`.
fn default_domain_role_title(domain: &str) -> String {
    match domain {
        "computers" => "Computer Systems Architect".to_string(),
        "science" | "scientific-research" => "Lead Research Scientist".to_string(),
        "health" => "Clinical Health Specialist".to_string(),
        "finance" => "Wealth & Income Strategist".to_string(),
        "software" => "Systems Architect".to_string(),
        "business" => "Business Strategist".to_string(),
        "nutrition" => "Registered Dietician".to_string(),
        "pharma" => "Pharmacologist".to_string(),
        "data-science" => "Data Scientist".to_string(),
        "education" => "Curriculum Designer".to_string(),
        "creative" => "Creative Director".to_string(),
        "legal" => "Legal Strategist".to_string(),
        "marketing" => "Growth Strategist".to_string(),
        "devops" => "Platform Engineer".to_string(),
        "ai-ml" => "AI Researcher".to_string(),
        "medical" => "Physician".to_string(),
        "cybersecurity" => "Security Analyst".to_string(),
        "ecommerce" => "E-commerce Manager".to_string(),
        "real-estate" => "Real Estate Agent".to_string(),
        "workplace-productivity" => "Workplace Strategist".to_string(),
        "travel" => "Travel Planner".to_string(),
        _ => "Domain Specialist".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_three_tier_resolution_without_pool_across_canonical_domains_and_aliases() {
        let registry = SchemaRegistry::without_pool();

        // Tier 1: Exact match across canonical domains and aliases ("software", "tech", "legal", "finance", "medical", "nutrition", "cybersecurity")
        for (domain, task_type, expertise, expected_role_substr) in [
            ("software", "creation", "expert", "Systems Architect"),
            ("tech", "creation", "expert", "Systems Architect"),
            (
                "software-engineering",
                "analysis",
                "intermediate",
                "Systems Architect",
            ),
            ("legal", "analysis", "expert", "Legal Strategist"),
            (
                "finance",
                "planning",
                "expert",
                "Wealth & Income Strategist",
            ),
            ("medical", "analysis", "expert", "Physician"),
            (
                "nutrition",
                "creation",
                "intermediate",
                "Registered Dietician",
            ),
            (
                "cybersecurity",
                "troubleshooting",
                "expert",
                "Security Analyst",
            ),
        ] {
            let key = SchemaLookupKey {
                domain: domain.to_string(),
                task_type: task_type.to_string(),
                expertise: expertise.to_string(),
            };
            let tier1 = registry.find_exact_schema_match(&key).await;
            assert!(
                tier1.is_some(),
                "Tier 1 must resolve for domain={domain}, task={task_type}, expertise={expertise}"
            );
            let schema = tier1.unwrap_or_default();
            let role = schema.role.clone().unwrap_or_default();
            assert!(
                role.contains(expected_role_substr),
                "Expected role for {domain} to contain '{expected_role_substr}', got '{role}'"
            );
            assert!(
                schema.context.is_some(),
                "Expected non-empty context for {domain}"
            );
            assert!(
                schema.task.is_some(),
                "Expected non-empty task template for {domain}"
            );
            assert!(
                !schema.output.is_empty(),
                "Expected non-empty deliverables for {domain}"
            );
        }

        // Tier 2: Domain-level match when expertise is unknown or task_type is non-standard
        let tier1_miss_key = SchemaLookupKey {
            domain: "tech".to_string(),
            task_type: "unknown_custom_task".to_string(),
            expertise: "unknown_level".to_string(),
        };
        assert!(
            registry
                .find_exact_schema_match(&tier1_miss_key)
                .await
                .is_none(),
            "Tier 1 should abstain on unrecognized task_type/expertise"
        );
        let tier2 = registry
            .find_domain_level_schema(&tier1_miss_key.domain, &tier1_miss_key.task_type)
            .await;
        assert!(
            tier2.is_some(),
            "Tier 2 must resolve 'tech' -> 'software' domain-level schema"
        );
        let resolved = registry.resolve_schema_for_signal(&tier1_miss_key).await;
        assert!(
            resolved
                .role
                .as_deref()
                .unwrap_or_default()
                .contains("Systems Architect"),
            "resolve_schema_for_signal must fall back to Tier 2 for 'tech'"
        );

        // Tier 3: General fallback when domain is "general" or unknown
        let general_key = SchemaLookupKey {
            domain: "general".to_string(),
            task_type: "creation".to_string(),
            expertise: "intermediate".to_string(),
        };
        assert!(registry
            .find_exact_schema_match(&general_key)
            .await
            .is_none());
        assert!(registry
            .find_domain_level_schema("general", "creation")
            .await
            .is_none());
        let fallback = registry.resolve_schema_for_signal(&general_key).await;
        assert_eq!(fallback.role.as_deref(), Some("Domain Expert"));
        assert!(!fallback.output.is_empty());
    }

    #[tokio::test]
    async fn test_empty_config_handle_falls_back_to_unified_json() {
        let empty_handle = Arc::new(ConfigHandle::new(UnifiedConfig {
            domain_taxonomy: vec![],
            roles: vec![],
            constraints: vec![],
        }));
        let registry = SchemaRegistry::with_config(empty_handle);

        let key = SchemaLookupKey {
            domain: "legal".to_string(),
            task_type: "analysis".to_string(),
            expertise: "expert".to_string(),
        };
        let schema = registry.resolve_schema_for_signal(&key).await;
        assert!(
            schema
                .role
                .as_deref()
                .unwrap_or_default()
                .contains("Legal Strategist"),
            "Empty ConfigHandle must still fall back to unified.json, got: {:?}",
            schema.role
        );
        assert!(!schema.output.is_empty());
    }

    #[tokio::test]
    async fn test_perspective_keyed_resolution_consumer_practitioner_investor() {
        let registry = SchemaRegistry::without_pool();

        // 1. Real Estate Consumer ("consumer:build") -> Residential Home-Buying & Mortgage Advisor + homebuying deliverables
        let re_consumer = registry
            .resolve_schema_for_signal(&SchemaLookupKey {
                domain: "real-estate".to_string(),
                task_type: "consumer:build".to_string(),
                expertise: "intermediate".to_string(),
            })
            .await;
        assert_eq!(
            re_consumer.role.as_deref(),
            Some("Residential Home-Buying & Mortgage Advisor")
        );
        assert!(
            re_consumer
                .output
                .iter()
                .any(|d| d.name.contains("Mortgage Pre-Approval")
                    || d.name.contains("Affordability")),
            "Expected homebuyer deliverables for consumer:build in real-estate, got: {:?}",
            re_consumer.output
        );

        // 2. Real Estate Practitioner ("practitioner:build") -> Real Estate Agent + licensing/brokerage deliverables
        let re_practitioner = registry
            .resolve_schema_for_signal(&SchemaLookupKey {
                domain: "real-estate".to_string(),
                task_type: "practitioner:build".to_string(),
                expertise: "intermediate".to_string(),
            })
            .await;
        assert_eq!(re_practitioner.role.as_deref(), Some("Real Estate Agent"));
        assert!(
            re_practitioner
                .output
                .iter()
                .any(|d| d.name.contains("Real Estate License")
                    || d.name.contains("Broker Sponsorship")),
            "Expected licensing deliverables for practitioner:build in real-estate, got: {:?}",
            re_practitioner.output
        );

        // 3. Real Estate Investor ("investor:analyze") -> Real Estate Investment & Underwriting Strategist + CMA/NOI deliverables
        let re_investor = registry
            .resolve_schema_for_signal(&SchemaLookupKey {
                domain: "real-estate".to_string(),
                task_type: "investor:analyze".to_string(),
                expertise: "expert".to_string(),
            })
            .await;
        assert!(
            re_investor
                .role
                .as_deref()
                .unwrap_or_default()
                .contains("Real Estate Investment & Underwriting Strategist"),
            "Got investor role: {:?}",
            re_investor.role
        );
        assert!(
            re_investor
                .output
                .iter()
                .any(|d| d.name.contains("Comparative Market Analysis") || d.name.contains("NOI")),
            "Expected underwriting deliverables for investor:analyze, got: {:?}",
            re_investor.output
        );

        // 4. Finance, Legal, Education consumer:build vs practitioner:build
        for (domain, consumer_role, consumer_deliv_substr, practitioner_deliv_substr) in [
            (
                "finance",
                "Personal Finance & Household Budget Advisor",
                "Budget",
                "Burn Rate Audit",
            ),
            (
                "legal",
                "Consumer Legal Rights & Dispute Advisor",
                "Evidence Dossier",
                "Statutory Basis Analysis",
            ),
            (
                "education",
                "Personal Learning & Mastery Coach",
                "Study Schedule",
                "Prerequisite Audit",
            ),
        ] {
            let cons = registry
                .resolve_schema_for_signal(&SchemaLookupKey {
                    domain: domain.to_string(),
                    task_type: "consumer:build".to_string(),
                    expertise: "intermediate".to_string(),
                })
                .await;
            assert_eq!(cons.role.as_deref(), Some(consumer_role));
            assert!(
                cons.output
                    .iter()
                    .any(|d| d.name.contains(consumer_deliv_substr)),
                "[{domain} consumer:build] expected deliverable containing '{consumer_deliv_substr}', got {:?}",
                cons.output
            );

            let prac = registry
                .resolve_schema_for_signal(&SchemaLookupKey {
                    domain: domain.to_string(),
                    task_type: "practitioner:build".to_string(),
                    expertise: "expert".to_string(),
                })
                .await;
            assert!(
                prac.output
                    .iter()
                    .any(|d| d.name.contains(practitioner_deliv_substr)),
                "[{domain} practitioner:build] expected deliverable containing '{practitioner_deliv_substr}', got {:?}",
                prac.output
            );
        }
    }
}
