use crate::types::{AlgorithmOutput, WmSlot};
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref SCOPE_INJECTIONS: HashMap<String, Vec<String>> = {
        let mut m = HashMap::new();
        m.insert(
            "authentication".to_string(),
            vec![
                "Consider OAuth 2.0 for secure authentication".to_string(),
                "Implement multi-factor authentication".to_string(),
                "Use JWT tokens for stateless auth".to_string(),
            ],
        );
        m.insert(
            "authorization".to_string(),
            vec![
                "Apply RBAC (Role-Based Access Control)".to_string(),
                "Use ABAC (Attribute-Based Access Control)".to_string(),
                "Implement permission levels".to_string(),
            ],
        );
        m.insert(
            "encryption".to_string(),
            vec![
                "Use AES-256 for data at rest".to_string(),
                "Apply TLS 1.3 for data in transit".to_string(),
                "Consider homomorphic encryption".to_string(),
            ],
        );
        m.insert(
            "compression".to_string(),
            vec![
                "Apply dictionary-based compression".to_string(),
                "Use adaptive entropy coding".to_string(),
                "Consider hybrid compression approaches".to_string(),
            ],
        );
        m.insert(
            "neural".to_string(),
            vec![
                "Use attention mechanisms for context".to_string(),
                "Apply transformer architectures".to_string(),
                "Consider neural-symbolic integration".to_string(),
            ],
        );
        m
    };
}

/// Injects deterministic scope suggestions based on inferred task/deliverable/context
#[derive(Debug, Default)]
pub struct DeterminismScopeInjector;

impl DeterminismScopeInjector {
    /// Returns scope injection suggestions for the given context
    pub fn inject(
        task: &Option<String>,
        deliverable: &Option<String>,
        context: &[String],
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Check task field
        if let Some(task_text) = task {
            let lower_task = task_text.to_lowercase();
            for (key, injections) in SCOPE_INJECTIONS.iter() {
                if lower_task.contains(key) {
                    suggestions.extend(injections.clone());
                }
            }
        }

        // Check deliverable field
        if let Some(deliverable_text) = deliverable {
            let lower_deliverable = deliverable_text.to_lowercase();
            for (key, injections) in SCOPE_INJECTIONS.iter() {
                if lower_deliverable.contains(key) {
                    suggestions.extend(injections.clone());
                }
            }
        }

        // Check context fields
        for ctx in context {
            let lower_ctx = ctx.to_lowercase();
            for (key, injections) in SCOPE_INJECTIONS.iter() {
                if lower_ctx.contains(key) {
                    suggestions.extend(injections.clone());
                }
            }
        }

        // Remove duplicates while preserving order
        suggestions.dedup();
        suggestions
    }
}

/// Schema Filling — resolves NULLs in task, deliverable, context using layered priors.
///
/// Layer 1: Local WM slots (current context)
/// Layer 2: Session history tokens (recent conversation)
/// Layer 3: Domain knowledge map (static vocabulary — inject from config)
#[derive(Debug, Default)]
pub struct SchemaFilling {
    // In a real system, this would be loaded from config or a domain knowledge base
    domain_knowledge: HashMap<String, String>,
}

impl SchemaFilling {
    pub fn new() -> Self {
        // Initialize with some domain knowledge examples
        let mut domain_knowledge = HashMap::new();
        domain_knowledge.insert(
            "task".to_string(),
            "Complete the requested action".to_string(),
        );
        domain_knowledge.insert(
            "deliverable".to_string(),
            "Provide the specified output".to_string(),
        );
        domain_knowledge.insert(
            "context".to_string(),
            "Relevant background information".to_string(),
        );
        domain_knowledge.insert(
            "authentication".to_string(),
            "User login and access control".to_string(),
        );
        domain_knowledge.insert(
            "authorization".to_string(),
            "Permission granting system".to_string(),
        );
        domain_knowledge.insert(
            "encryption".to_string(),
            "Data protection via cryptographic methods".to_string(),
        );
        domain_knowledge.insert(
            "compression".to_string(),
            "Data size reduction technique".to_string(),
        );
        domain_knowledge.insert(
            "neural".to_string(),
            "Brain-inspired computing approach".to_string(),
        );
        domain_knowledge.insert(
            "schema".to_string(),
            "Structured knowledge representation".to_string(),
        );

        Self { domain_knowledge }
    }

    /// Fill NULLs in task, deliverable, context using specified number of layers.
    /// layers = 1 (Gentle): only use WM slots
    /// layers = 3 (Aggressive): WM + session history + domain knowledge
    /// If output is provided and layers == 3, scope_injections will be set in the output.
    pub fn fill(
        &self,
        wm_slots: &[WmSlot],
        layers: u8,
        delta_tokens: &[String],
        clusters: &[Vec<String>],
        output: Option<&mut AlgorithmOutput>,
    ) -> FillResult {
        // Extract text from WM slots
        let wm_texts: Vec<String> = wm_slots.iter().map(|s| s.content.clone()).collect();

        // Extract text from clusters (flatten)
        let cluster_texts: Vec<String> = clusters.iter().flatten().cloned().collect();

        // Combine sources based on layers
        let mut sources = Vec::new();
        sources.push(wm_texts.clone()); // Layer 1: always include WM

        if layers >= 2 {
            // Layer 2: would include session history (passed separately in real implementation)
            // For now, we'll use delta_tokens as a proxy for recent novel tokens
            sources.push(delta_tokens.to_vec());
        }

        if layers >= 3 {
            // Layer 3: domain knowledge
            sources.push(self.domain_knowledge.values().cloned().collect());
        }

        // Flatten all sources
        let all_sources: Vec<String> = sources.into_iter().flatten().collect();

        let task = self.infer_task(&wm_texts, &all_sources);
        let role = self.infer_role(&wm_texts, &all_sources);
        let context = self.infer_context(&wm_texts, &all_sources, &cluster_texts);

        // Determine what was inferred vs what was NULL
        let null_fields = self.detect_null_fields(&task, &context);
        let task_inferred = task.is_some();

        // If layers == 3 (Aggressive) and output is provided, compute and attach scope injections
        if layers == 3 {
            if let Some(out) = output {
                // Clone the values needed for scope injection to avoid moving them
                let task_clone = task.clone();
                let context_clone = context.clone();
                let injections = DeterminismScopeInjector::inject(
                    &task_clone,
                    &None,
                    &context_clone,
                );
                out.scope_injections = injections;
            }
        }

        FillResult {
            task,
            role,
            context,
            null_fields,
            task_inferred,
        }
    }

    /// Checks if all texts are vague/uninformative (e.g., "unknown", "thing", etc.)
    fn all_texts_vague(&self, texts: &[String]) -> bool {
        if texts.is_empty() {
            return true;
        }
        let vague_words = vec![
            "unknown",
            "thing",
            "something",
            "anything",
            "etc",
            "stuff",
            "item",
        ];
        texts.iter().all(|text| {
            let lower = text.to_lowercase();
            text.len() < 3 || vague_words.contains(&lower.as_str())
        })
    }

    fn infer_task(&self, wm_texts: &[String], _all_sources: &[String]) -> Option<String> {
        // Look for action-oriented keywords in WM or sources
        let task_keywords = [
            "build",
            "create",
            "make",
            "develop",
            "implement",
            "design",
            "write",
            "code",
        ];
        for keyword in task_keywords {
            if wm_texts.iter().any(|t| t.to_lowercase().contains(keyword)) {
                return Some(format!("Perform {} action", keyword));
            }
        }
        
        // Robust fallback chain:
        // Try domain knowledge first
        if let Some(t) = self.domain_knowledge.get("task") {
            return Some(t.clone());
        }
        
        // Then try first meaningful WM slot
        if !self.all_texts_vague(wm_texts) {
            if let Some(t) = wm_texts.first() {
                return Some(t.clone());
            }
        }
        
        // Final fallback to guarantee task resolution (prevent NULL task)
        Some("Analyze and process the provided context".to_string())
    }

    fn infer_role(&self, wm_texts: &[String], _all_sources: &[String]) -> Option<String> {
        // Look for domain/role keywords
        let role_map = [
            ("code", "Expert Software Engineer"),
            ("develop", "Expert Software Engineer"),
            ("design", "Architecture Specialist"),
            ("legal", "Legal Advisor"),
            ("finance", "Financial Analyst"),
            ("biology", "Research Biologist"),
            ("data", "Data Scientist"),
            ("write", "Professional Copywriter"),
        ];

        for (keyword, role) in role_map {
            if wm_texts.iter().any(|t| t.to_lowercase().contains(keyword)) {
                return Some(role.to_string());
            }
        }

        // Fallback to a general expert role
        Some("Domain Expert".to_string())
    }

    fn infer_context(
        &self,
        wm_texts: &[String],
        _all_sources: &[String],
        cluster_texts: &[String],
    ) -> Vec<String> {
        // Context is remaining relevant information
        let mut context = Vec::new();

        // Define words that are too vague to be considered context
        let vague_words = vec!["unknown", "thing", "something", "anything", "etc"];

        // Add WM texts that aren't task, have sufficient length, and are not vague
        for text in wm_texts {
            let lower = text.to_lowercase();
            if text.len() >= 3
                && !vague_words.contains(&lower.as_str())
                && !lower.contains("build")
                && !lower.contains("create")
            {
                context.push(text.clone());
            }
        }

        // Add some cluster labels as context (up to 2)
        for text in cluster_texts.iter().take(2) {
            context.push(text.clone());
        }

        context
    }

    fn detect_null_fields(
        &self,
        task: &Option<String>,
        context: &[String],
    ) -> Vec<String> {
        let mut null_fields = Vec::new();
        if task.is_none() {
            null_fields.push("task".to_string());
        }
        if context.is_empty() {
            null_fields.push("context".to_string());
        }
        null_fields
    }
}

#[derive(Debug, Default)]
pub struct FillResult {
    pub task: Option<String>,
    pub role: Option<String>,
    pub context: Vec<String>,
    pub null_fields: Vec<String>,
    pub task_inferred: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WmSlot;

    fn make_slot(content: &str, salience: f32) -> WmSlot {
        WmSlot {
            content: content.to_string(),
            salience: salience,
            source: crate::types::SlotSource::Delta,
            is_protected: false,
        }
    }

    #[test]
    fn gentle_layers_only_wm() {
        let sf = SchemaFilling::new();
        let wm_slots = vec![
            make_slot("build authentication system", 0.9),
            make_slot("user login", 0.8),
            make_slot("secure access", 0.7),
        ];
        let result = sf.fill(&wm_slots, 1, &[], &[], None);
        assert!(
            result.task.is_some(),
            "Task should be inferred in gentle mode"
        );
        assert!(
            !result.context.is_empty(),
            "Context should have some content"
        );
        assert_eq!(
            result.null_fields.len(),
            0,
            "No null fields should remain in gentle mode with sufficient WM"
        );
    }

    #[test]
    fn aggressive_uses_all_layers() {
        let sf = SchemaFilling::new();
        let wm_slots = vec![make_slot("OAuth2", 0.9), make_slot("PKCE", 0.8)];
        let delta_tokens = vec!["multi-tenant".to_string(), "HSM".to_string()];
        let clusters = vec![vec!["auth".to_string(), "security".to_string()]];

        let result = sf.fill(&wm_slots, 3, &delta_tokens, &clusters, None);
        assert!(result.task.is_some(), "Task should be inferred");
        // Should have used all layers, so context should be richer
        assert!(
            result.context.len() >= 2,
            "Context should benefit from all layers"
        );
    }

    #[test]
    fn null_detection() {
        let sf = SchemaFilling::new();
        let wm_slots = vec![make_slot("unknown", 0.5)];
        let result = sf.fill(&wm_slots, 1, &[], &[], None);
        assert!(result.null_fields.contains(&"context".to_string()));
    }

    #[test]
    fn scope_injections_in_aggressive_mode() {
        let sf = SchemaFilling::new();
        // Use terms that will match our scope injection mappings
        let wm_slots = vec![
            make_slot("build authentication system", 0.9),
            make_slot("user authorization", 0.8),
        ];
        let delta_tokens = vec!["multi-tenant".to_string(), "HSM".to_string()];
        let clusters = vec![vec!["security".to_string(), "encryption".to_string()]];

        let mut output = AlgorithmOutput::default();
        let result = sf.fill(&wm_slots, 3, &delta_tokens, &clusters, Some(&mut output));

        // Debug: print what we actually got
        println!("Task: {:?}", result.task);
        println!("Context: {:?}", result.context);
        println!("Scope injections: {:?}", output.scope_injections);

        // Should have inferred task
        assert!(result.task.is_some());

        // Should have scope injections because we have auth-related terms
        assert!(
            !output.scope_injections.is_empty(),
            "Expected scope injections for auth-related terms"
        );

        // Check that we have some expected injections - look for auth or authorization related terms
        let has_auth_injection = output.scope_injections.iter().any(|s| {
            s.contains("OAuth")
                || s.contains("authentication")
                || s.contains("authorization")
                || s.contains("RBAC")
                || s.contains("ABAC")
        });
        assert!(
            has_auth_injection,
            "Expected at least one auth/authorization related scope injection"
        );
    }
}
