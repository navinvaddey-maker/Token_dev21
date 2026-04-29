use crate::{algorithms::schema_filling::SchemaFilling, types::AlgorithmOutput};

/// Handles Stage 4: Schema Filling (NULL Resolution)
pub struct Stage4 {
    schema: SchemaFilling,
}

impl Stage4 {
    /// Creates a new Stage4 instance
    pub fn new() -> Self {
        Self {
            schema: SchemaFilling::default(),
        }
    }

    /// Processes input through Stage 4: Schema Filling
    /// Mode-aware: Gentle = 1 layer, Aggressive = 3 layers.
    pub fn run(&self, out: &mut AlgorithmOutput) {
        // Extract all needed values to avoid borrow conflicts
        let wm_slots = out.wm_slots.clone();
        let delta_tokens = out.delta_tokens.clone();
        let clusters = out.clusters.clone();

        // Determine layers based on mode (default to Gentle if not set)
        let mode = out.mode.as_ref().unwrap_or(&crate::types::Mode::Gentle);
        let layers = match mode {
            crate::types::Mode::Gentle | crate::types::Mode::Ambiguous | crate::types::Mode::Balanced => 1,
            crate::types::Mode::Aggressive => 3,
        };

        let result = self
            .schema
            .fill(&wm_slots, layers, &delta_tokens, &clusters, Some(out));

        out.resolved_schema = crate::types::CompressionSchema {
            role: result.role.clone(),
            context: if result.context.is_empty() { None } else { Some(result.context.join(" ")) },
            task: result.task.clone(),
        };


        out.null_fields = result.null_fields;
        out.task_inferred = result.task_inferred;

    }
}
