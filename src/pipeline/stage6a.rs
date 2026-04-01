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
        // Use the compressed tokens from sparse coding if available (these have gone through salience filtering)
        // Otherwise fall back to lexical compression tokens
        // Otherwise generate from resolved fields as last resort

        let compressed_tokens = if !schema_filled.sparse_tokens.is_empty() {
            // Use sparse coding tokens (these have salience scores and represent the most important concepts)
            schema_filled
                .sparse_tokens
                .iter()
                .map(|token| token.text.clone())
                .collect::<Vec<String>>()
                .join(" ")
        } else if !schema_filled.clean_tokens.is_empty() {
            // Fall back to lexical compression tokens (basic stopword removal and normalization)
            schema_filled.clean_tokens.join(" ")
        } else {
            // Last resort: generate from resolved fields
            let mut parts = Vec::new();

            if let Some(task) = &schema_filled.resolved_task {
                parts.push(format!("task: {}", task));
            }

            if let Some(deliverable) = &schema_filled.resolved_deliverable {
                parts.push(format!("deliverable: {}", deliverable));
            }

            if !schema_filled.resolved_context.is_empty() {
                let context_str = schema_filled.resolved_context.join(", ");
                parts.push(format!("context: {}", context_str));
            }

            if parts.is_empty() {
                "compressed prompt".to_string()
            } else {
                parts.join("\n")
            }
        };

        Ok(compressed_tokens)
    }
}

impl Default for Stage6a {
    fn default() -> Self {
        Self::new()
    }
}
