use crate::types::{CompressionSchema, Constraint, Deliverable};
use serde::Deserialize;
use sqlx::SqlitePool;

/// Key used to look up the correct schema template for a given request.
#[derive(Debug, Clone)]
pub struct SchemaLookupKey {
    pub domain: String,
    pub task_type: String,
    pub expertise: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSchemaTemplate {
    pub role: Option<RawRole>,
    pub context_fields: Option<Vec<String>>,
    pub constraints: Option<serde_json::Value>,
    pub execution_phases: Option<Vec<RawPhase>>,
    pub _success_criteria_keys: Option<Vec<String>>,
    pub task_instruction_suffix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRole {
    pub profile: Option<String>,
    pub _guardrail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPhase {
    pub name: String,
    pub _duration: Option<String>,
}

fn parse_template_json(json_str: &str) -> Option<CompressionSchema> {
    if let Ok(schema) = serde_json::from_str::<CompressionSchema>(json_str) {
        return Some(schema);
    }

    if let Ok(raw) = serde_json::from_str::<RawSchemaTemplate>(json_str) {
        let role = raw.role.and_then(|r| r.profile);
        let context = raw.context_fields.map(|fields| fields.join(" "));
        let task = raw.task_instruction_suffix;
        let mut constraints = Vec::new();
        if let Some(val) = raw.constraints {
            if let Some(obj) = val.as_object() {
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        constraints.push(Constraint {
                            name: format!("{}: {}", k, s),
                        });
                    }
                }
            } else if let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        constraints.push(Constraint {
                            name: s.to_string(),
                        });
                    }
                }
            }
        }
        let output = raw
            .execution_phases
            .map(|phases| {
                phases
                    .into_iter()
                    .map(|p| Deliverable { name: p.name })
                    .collect()
            })
            .unwrap_or_default();

        return Some(CompressionSchema {
            role,
            context,
            task,
            constraints,
            output,
        });
    }

    None
}

/// Provides schema template lookup against the schema_index SQLite table.
#[derive(Clone, Default)]
pub struct SchemaRegistry {
    pool: Option<SqlitePool>,
}

impl SchemaRegistry {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool: Some(pool) }
    }

    pub fn without_pool() -> Self {
        Self { pool: None }
    }

    pub fn with_optional_pool(pool: Option<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Tier 1: Exact match on domain + task_type + expertise.
    pub async fn find_exact_schema_match(
        &self,
        key: &SchemaLookupKey,
    ) -> Option<CompressionSchema> {
        let pool = self.pool.as_ref()?;
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT template_json FROM schema_index
             WHERE domain = ?1 AND task_type = ?2 AND expertise = ?3
             LIMIT 1",
        )
        .bind(&key.domain)
        .bind(&key.task_type)
        .bind(&key.expertise)
        .fetch_optional(pool)
        .await
        .ok()?;

        row.and_then(|r| parse_template_json(&r.0))
    }

    /// Tier 2: Domain + task_type match, ignoring expertise.
    pub async fn find_domain_level_schema(
        &self,
        domain: &str,
        task_type: &str,
    ) -> Option<CompressionSchema> {
        let pool = self.pool.as_ref()?;
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT template_json FROM schema_index
             WHERE domain = ?1 AND task_type = ?2
             ORDER BY version DESC LIMIT 1",
        )
        .bind(domain)
        .bind(task_type)
        .fetch_optional(pool)
        .await
        .ok()?;

        row.and_then(|r| parse_template_json(&r.0))
    }

    /// Tier 3: General fallback template.
    pub async fn get_general_fallback_schema(&self) -> CompressionSchema {
        if let Some(pool) = self.pool.as_ref() {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT template_json FROM schema_index
                 WHERE domain = 'general' LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            if let Some(schema) = row.and_then(|r| parse_template_json(&r.0)) {
                return schema;
            }
        }
        CompressionSchema::default()
    }

    /// Unified entry point used by Stage 4.
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
