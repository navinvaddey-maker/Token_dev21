use crate::{algorithms::schema_filling::SchemaFilling, types::AlgorithmOutput};

/// Handles Stage 4: Schema Filling (NULL Resolution)
pub struct Stage4 {
    schema: SchemaFilling,
    registry: crate::db::schema_registry::SchemaRegistry,
}

impl Stage4 {
    /// Creates a new Stage4 instance without database pool (fallback mode)
    pub fn new() -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: crate::db::schema_registry::SchemaRegistry::without_pool(),
        }
    }

    /// Creates a new Stage4 instance with a database pool
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: crate::db::schema_registry::SchemaRegistry::new(pool),
        }
    }

    /// Creates a new Stage4 instance with an optional database pool
    pub fn with_optional_pool(pool: Option<sqlx::SqlitePool>) -> Self {
        Self {
            schema: SchemaFilling::default(),
            registry: crate::db::schema_registry::SchemaRegistry::with_optional_pool(pool),
        }
    }

    /// Processes input through Stage 4: Schema Filling
    /// Mode-aware: Gentle = 1 layer, Aggressive = 3 layers.
    pub async fn run(&self, out: &mut AlgorithmOutput) {
        // Resolve schema using ScenarioSignal if present, otherwise use fallback
        if let Some(ref signal) = out.scenario {
            let key = crate::db::schema_registry::SchemaLookupKey {
                domain: signal.domain.clone(),
                task_type: signal.task_type.clone(),
                expertise: signal.expertise.clone(),
            };
            out.resolved_schema = self.registry.resolve_schema_for_signal(&key).await;
        } else {
            out.resolved_schema = self.registry.get_general_fallback_schema().await;
        }

        // Extract all needed values to avoid borrow conflicts
        let wm_slots = out.wm_slots.clone();
        let delta_tokens = out.delta_tokens.clone();
        let clusters = out.clusters.clone();

        // Determine layers based on mode (default to Gentle if not set)
        let mode = out.mode.as_ref().unwrap_or(&crate::types::Mode::Gentle);
        let layers = match mode {
            crate::types::Mode::Gentle | crate::types::Mode::Ambiguous => 1,
            crate::types::Mode::Balanced => 2,
            crate::types::Mode::Aggressive => 3,
        };

        let result = self
            .schema
            .fill(&wm_slots, layers, &delta_tokens, &clusters, Some(out));

        // Merge the resolved schema from registry with the filling result
        out.resolved_schema.role = result.role.clone();
        out.resolved_schema.context = if result.context.is_empty() {
            out.resolved_schema.context.clone()
        } else {
            Some(result.context.join(" "))
        };
        out.resolved_schema.task = result.task.clone();
        out.resolved_schema.constraints = result.constraints;
        out.resolved_schema.output = result.output;

        out.null_fields = result.null_fields;
        out.task_inferred = result.task_inferred;
    }
}
