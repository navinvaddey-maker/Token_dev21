use crate::types::AlgorithmOutput;

/// Handles Stage 6A: Output Generation
pub struct Stage6a;

impl Stage6a {
    /// Creates a new Stage6a instance
    pub fn new() -> Self {
        Self
    }

    /// Processes input through Stage 6A: Output Generation
    /// Generates the final compressed prompt from the schema-filled data.
    pub fn run(
        &self,
        schema_filled: &AlgorithmOutput,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Primary: Generate professional, structured output from the resolved schema
        let mut parts = Vec::new();

        if let Some(role) = &schema_filled.resolved_schema.role {
            parts.push(format!("**Role:** {}", role));
        }

        if let Some(task) = &schema_filled.resolved_schema.task {
            parts.push(format!("**Task:** {}", task));
        }

        if let Some(norm) = &schema_filled.normalization {
            parts.push(format!("**Context:** {}", norm.normalized_text));
        } else if let Some(context) = &schema_filled.resolved_schema.context {
            // Only add context if it's not a generic placeholder
            if context.len() > 3 {
                parts.push(format!("**Context:** {}", context));
            }
        }

        if !schema_filled.scope_injections.is_empty() {
            let scopes = schema_filled.scope_injections.join(", ");
            parts.push(format!("**Directives:** {}", scopes));
        }

        let compressed_tokens = if !parts.is_empty() {
            parts.join("\n")
        } else if !schema_filled.sparse_tokens.is_empty() {
            // Fall back to sparse coding tokens if schema failed to populate
            schema_filled
                .sparse_tokens
                .iter()
                .map(|token| token.text.clone())
                .collect::<Vec<String>>()
                .join(" ")
        } else if !schema_filled.clean_tokens.is_empty() {
            // Fall back to lexical compression tokens
            schema_filled.clean_tokens.join(" ")
        } else {
            "Please clarify your request.".to_string()
        };

        Ok(compressed_tokens)
    }
}

impl Default for Stage6a {
    fn default() -> Self {
        Self::new()
    }
}
