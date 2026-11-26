//! Stage 4: Schema Filling (NULL Resolution) (`ARCHITECTURE.md` §1.2, §1.4, §1.5).
//!
//! Resolves domain-specific schema templates via `SchemaRegistry` and merges them
//! non-destructively with `ScenarioSignal` and `SchemaFilling` output while
//! keeping constraint and deliverable lists compact for Balanced Mode.

use std::sync::Arc;

use crate::{
    algorithms::schema_filling::SchemaFilling,
    classifier::signal::ScenarioSignal,
    db::schema_registry::{SchemaLookupKey, SchemaRegistry},
    npae::aggressive::config::ConfigHandle,
    types::{AlgorithmOutput, Constraint, Deliverable, Mode},
};

/// Handles Stage 4: Schema Filling (NULL Resolution).
#[derive(Debug)]
pub struct Stage4 {
    schema: SchemaFilling,
    registry: SchemaRegistry,
}

impl Stage4 {
    /// Creates a new `Stage4` instance without a database pool (fallback mode).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: SchemaRegistry::without_pool(),
        }
    }

    /// Creates a new `Stage4` instance backed by the unified configuration handle.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn with_config(config: Arc<ConfigHandle>) -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: SchemaRegistry::with_config(config),
        }
    }

    /// Creates a new `Stage4` instance with a database pool.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: SchemaRegistry::new(pool),
        }
    }

    /// Creates a new `Stage4` instance with an optional database pool.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn with_optional_pool(pool: Option<sqlx::SqlitePool>) -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: SchemaRegistry::with_optional_pool(pool),
        }
    }

    /// Processes input through Stage 4: Schema Filling.
    ///
    /// Mode-aware: Gentle = 1 layer, Balanced = 2 layers, Aggressive = 3 layers.
    /// Merges `SchemaFilling::fill()` non-destructively with `SchemaRegistry` and
    /// `ScenarioSignal` so domain roles, secondary verticals, tasks, constraints,
    /// and deliverables survive without exceeding Balanced Mode compactness limits.
    ///
    /// # Panics
    /// Never.
    pub async fn run(&self, out: &mut AlgorithmOutput) {
        // Resolve schema using ScenarioSignal if present, otherwise use general fallback.
        // ARCH: §1.4 & §2 — 3-tier schema lookup from SchemaRegistry before non-destructive merge.
        let registry_schema = if let Some(ref signal) = out.scenario {
            let key = SchemaLookupKey {
                domain: signal.domain.clone(),
                task_type: signal.task_type.clone(),
                expertise: signal.expertise.clone(),
            };
            self.registry.resolve_schema_for_signal(&key).await
        } else {
            self.registry.get_general_fallback_schema().await
        };
        out.resolved_schema = registry_schema.clone();

        // Extract needed values to avoid borrow conflicts during `SchemaFilling::fill`.
        let wm_slots = out.wm_slots.clone();
        let delta_tokens = out.delta_tokens.clone();
        let clusters = out.clusters.clone();
        let signal_snapshot = out.scenario.clone();

        // Determine layers based on mode (default to Gentle if not set).
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);
        let layers = match mode {
            Mode::Gentle | Mode::Ambiguous => 1,
            Mode::Balanced => 2,
            Mode::Aggressive => 3,
        };

        let result = self
            .schema
            .fill(&wm_slots, layers, &delta_tokens, &clusters, Some(out));

        // ARCH: §1.4 & §1.5 — Non-destructive merge of `SchemaFilling` with `ScenarioSignal`
        // and `SchemaRegistry`.
        // 1. Role priority:
        //    (1) `signal.primary_role` composed with `signal.secondary_domain` (e.g. "Business Strategist (Pharma)")
        //    (2) `result.role` if `Some(r)` and `r != "Domain Expert"`
        //    (3) `registry_schema.role` if `Some`
        //    (4) `result.role`
        out.resolved_schema.role = Self::merge_role(
            signal_snapshot.as_ref(),
            result.role.as_deref(),
            registry_schema.role.as_deref(),
        );

        // 2. Context: keep `result.context.join(" ")` if non-empty, else `registry_schema.context`.
        out.resolved_schema.context = if result.context.is_empty() {
            registry_schema.context
        } else {
            Some(result.context.join(" "))
        };

        // 3. Task: if `result.task` is `None` or starts with generic `"Perform "` while
        //    `registry_schema.task` is `Some`, prefer or combine with the richer registry task.
        out.resolved_schema.task = Self::merge_task(result.task, registry_schema.task);

        // 4. Constraints: start with `result.constraints` (which preserves all `constraint_locks`),
        //    append `"regulatory_compliance"` if `signal.legal_as_constraint` is true, and merge
        //    up to 3 concise registry constraints (deduplicated case-insensitively, grounded against
        //    the prompt so unrelated trigger IDs like `high_carb` or `no_gluten` are never injected).
        let mut merged_constraints = Vec::new();
        for c in result.constraints {
            if !merged_constraints
                .iter()
                .any(|existing: &Constraint| existing.name.eq_ignore_ascii_case(&c.name))
            {
                merged_constraints.push(c);
            }
        }
        if signal_snapshot
            .as_ref()
            .is_some_and(|s| s.legal_as_constraint)
            && !merged_constraints
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case("regulatory_compliance"))
        {
            merged_constraints.push(Constraint {
                name: "regulatory_compliance".to_string(),
            });
        }
        let prompt_lower = out
            .normalization
            .as_ref()
            .map(|n| n.normalized_text.to_lowercase())
            .unwrap_or_default();
        let prompt_words: std::collections::HashSet<&str> = prompt_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        for reg_c in registry_schema.constraints {
            if merged_constraints.len() >= 3 {
                break;
            }
            let name = reg_c.name.trim();
            if name.is_empty() || name.split_whitespace().count() > 6 {
                continue;
            }
            let name_tokens: Vec<String> = name
                .split(['_', '-', ' '])
                .map(str::to_lowercase)
                .filter(|s| s.len() >= 3 && s != "preserve" && s != "hard" && s != "constraints")
                .collect();
            let is_grounded = name.eq_ignore_ascii_case("preserve_hard_constraints")
                || name.ends_with("_context")
                || prompt_words.is_empty()
                || (!name_tokens.is_empty()
                    && name_tokens.iter().all(|tok| {
                        prompt_words.contains(tok.as_str()) || prompt_lower.contains(tok.as_str())
                    }));
            if !is_grounded {
                continue;
            }
            if !merged_constraints
                .iter()
                .any(|existing| existing.name.eq_ignore_ascii_case(name))
            {
                merged_constraints.push(Constraint {
                    name: name.to_string(),
                });
            }
        }
        out.resolved_schema.constraints = merged_constraints;

        // 5. Output deliverables: start with `result.output` and merge up to 3 concise
        //    deliverables from `registry_schema.output` when `result.output` is empty.
        let mut merged_output = Vec::new();
        for d in result.output {
            if !merged_output
                .iter()
                .any(|existing: &Deliverable| existing.name.eq_ignore_ascii_case(&d.name))
            {
                merged_output.push(d);
            }
        }
        if merged_output.is_empty() {
            for reg_d in registry_schema.output {
                if merged_output.len() >= 3 {
                    break;
                }
                let name = reg_d.name.trim();
                if name.is_empty() {
                    continue;
                }
                if !merged_output
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(name))
                {
                    merged_output.push(Deliverable {
                        name: name.to_string(),
                    });
                }
            }
        }
        out.resolved_schema.output = merged_output;

        out.null_fields = result.null_fields;
        out.task_inferred = result.task_inferred;
    }

    /// Resolves the highest-priority role from `ScenarioSignal`, `SchemaFilling`, and `SchemaRegistry`.
    fn merge_role(
        signal: Option<&ScenarioSignal>,
        fill_role: Option<&str>,
        registry_role: Option<&str>,
    ) -> Option<String> {
        if let Some(sig) = signal {
            if let Some(ref primary) = sig.primary_role {
                let trimmed = primary.trim();
                if !trimmed.is_empty() {
                    if let Some(ref sec) = sig.secondary_domain {
                        let sec_titled = Self::title_case_slug(sec);
                        if !sec_titled.is_empty()
                            && !trimmed.to_lowercase().contains(&sec.to_lowercase())
                        {
                            return Some(format!("{trimmed} ({sec_titled})"));
                        }
                    }
                    return Some(trimmed.to_string());
                }
            }
        }

        if let Some(r) = fill_role {
            let trimmed = r.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("Domain Expert") {
                if let Some(sig) = signal {
                    if let Some(ref sec) = sig.secondary_domain {
                        let sec_titled = Self::title_case_slug(sec);
                        if !sec_titled.is_empty()
                            && !trimmed.to_lowercase().contains(&sec.to_lowercase())
                        {
                            return Some(format!("{trimmed} ({sec_titled})"));
                        }
                    }
                }
                return Some(trimmed.to_string());
            }
        }

        if let Some(reg_r) = registry_role {
            let trimmed = reg_r.trim();
            if !trimmed.is_empty() {
                if let Some(sig) = signal {
                    if let Some(ref sec) = sig.secondary_domain {
                        let sec_titled = Self::title_case_slug(sec);
                        if !sec_titled.is_empty()
                            && !trimmed.to_lowercase().contains(&sec.to_lowercase())
                        {
                            return Some(format!("{trimmed} ({sec_titled})"));
                        }
                    }
                }
                return Some(trimmed.to_string());
            }
        }

        fill_role.map(ToString::to_string)
    }

    /// Merges the task inferred by `SchemaFilling` with the domain task template from `SchemaRegistry`.
    fn merge_task(fill_task: Option<String>, registry_task: Option<String>) -> Option<String> {
        let reg_clean = registry_task
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        match (fill_task, reg_clean) {
            (None, Some(reg_t)) => Some(reg_t),
            (Some(ft), Some(reg_t)) => {
                let ft_trimmed = ft.trim();
                let is_generic = ft_trimmed.is_empty()
                    || ft_trimmed.starts_with("Perform ")
                    || ft_trimmed.eq_ignore_ascii_case("Complete the requested action")
                    || ft_trimmed.eq_ignore_ascii_case("Analyze and process the provided context");
                if is_generic {
                    Some(reg_t)
                } else {
                    Some(ft)
                }
            }
            (ft, None) => ft,
        }
    }

    /// Converts a hyphenated or underscored domain slug into title case (e.g., `"real-estate"` -> `"Real Estate"`).
    fn title_case_slug(slug: &str) -> String {
        crate::utils::text::OryxTextMatcher::format_slug_as_title_case(slug)
    }
}

impl Default for Stage4 {
    fn default() -> Self {
        Self::new()
    }
}
