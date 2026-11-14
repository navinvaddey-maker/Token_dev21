//! Schema Filling & Deterministic Scope Injection (`ARCHITECTURE.md` §1.2, §1.4, §1.5).
//!
//! Resolves NULLs in task, role, context, constraints, and deliverables using layered
//! priors (`WmSlot`, session `delta_tokens`, and domain knowledge) combined with
//! `ScenarioSignal` across all 22 canonical domains.

use crate::classifier::signal::ScenarioSignal;
use crate::types::{AlgorithmOutput, Constraint, Deliverable, Mode, WmSlot};
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

/// Injects deterministic scope suggestions based on inferred task/deliverable/context.
#[derive(Debug, Default)]
pub struct DeterminismScopeInjector;

impl DeterminismScopeInjector {
    /// Returns scope injection suggestions for the given context.
    ///
    /// # Panics
    /// Never.
    #[must_use]
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
    /// Creates a new `SchemaFilling` instance with default domain vocabulary priors.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
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
    ///
    /// - `layers = 1` (Gentle): only use WM slots
    /// - `layers = 2` (Balanced): WM + session history + implicit domain deliverables
    /// - `layers = 3` (Aggressive): WM + session history + domain knowledge + scope injections
    ///
    /// # Panics
    /// Never.
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
            // Layer 2: session delta_tokens
            sources.push(delta_tokens.to_vec());
        }

        if layers >= 3 {
            // Layer 3: domain knowledge
            sources.push(self.domain_knowledge.values().cloned().collect());
        }

        // Flatten all sources
        let all_sources: Vec<String> = sources.into_iter().flatten().collect();

        let (locks, expected_output, mode, scenario) = if let Some(o) = output.as_ref() {
            (
                o.constraint_locks.clone(),
                o.expected_deliverables.clone(),
                o.mode.clone(),
                o.scenario.clone(),
            )
        } else {
            (Vec::new(), Vec::new(), None, None)
        };

        let constraints: Vec<Constraint> = locks
            .iter()
            .map(|l| Constraint {
                name: l.text.clone(),
            })
            .collect();
        let mut deliverables: Vec<Deliverable> = expected_output
            .iter()
            .map(|d| Deliverable { name: d.clone() })
            .collect();

        let task = self.infer_task(&wm_texts, &all_sources);
        let role = self.infer_role(&wm_texts, &all_sources, scenario.as_ref());
        let context = self.infer_context(&wm_texts, &all_sources, &cluster_texts);

        // Determine what was inferred vs what was NULL
        let null_fields = self.detect_null_fields(&task, &context);
        let task_inferred = task.is_some();

        if layers >= 2 {
            Self::infer_implicit_deliverables(
                &task,
                &constraints,
                &mode,
                scenario.as_ref(),
                &all_sources,
                &mut deliverables,
            );
        }

        // If layers == 3 (Aggressive) and output is provided, compute and attach scope injections
        if layers == 3 {
            if let Some(out) = output {
                let task_clone = task.clone();
                let context_clone = context.clone();
                let injections =
                    DeterminismScopeInjector::inject(&task_clone, &None, &context_clone);
                out.scope_injections = injections;
            }
        }

        FillResult {
            task,
            role,
            context,
            constraints,
            output: deliverables,
            null_fields,
            task_inferred,
        }
    }

    /// Infers concise domain-specific deliverables when `mode` is `Balanced` or `Aggressive`.
    ///
    /// # Panics
    /// Never.
    fn infer_implicit_deliverables(
        task: &Option<String>,
        constraints: &[Constraint],
        mode: &Option<Mode>,
        scenario: Option<&ScenarioSignal>,
        all_sources: &[String],
        deliverables: &mut Vec<Deliverable>,
    ) {
        let allow = matches!(mode, Some(Mode::Balanced) | Some(Mode::Aggressive));
        if !allow {
            return;
        }

        let task_text = task.as_deref().unwrap_or("").to_lowercase();
        let constraint_text: String = constraints
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let source_text: String = all_sources
            .iter()
            .map(|s| s.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let combined = format!("{task_text} {constraint_text} {source_text}");
        let domain = scenario.map(|s| s.domain.as_str()).unwrap_or("");

        let push_unique = |list: &mut Vec<Deliverable>, item: &str| {
            if !list.iter().any(|d| d.name.eq_ignore_ascii_case(item)) {
                list.push(Deliverable {
                    name: item.to_string(),
                });
            }
        };

        // 1. Nutrition / Dietary meal plan
        let meal_plan = domain == "nutrition"
            || combined.contains("meal")
            || combined.contains("nutrition")
            || (combined.contains("plan")
                && ["diet", "calorie", "protein", "macro"]
                    .iter()
                    .any(|k| combined.contains(k)));
        let dietary = [
            "fiber",
            "dairy",
            "nightshade",
            "carb",
            "macro",
            "calorie",
            "vegan",
            "keto",
        ]
        .iter()
        .any(|k| combined.contains(k));
        if meal_plan && dietary {
            push_unique(deliverables, "shopping_list");
            push_unique(deliverables, "substitution_guide");
        }

        // Only infer domain deliverables when deliverables list is still empty (or under 2 items)
        // to keep Balanced Mode concise.
        if deliverables.len() >= 2 {
            return;
        }

        if matches!(
            domain,
            "software" | "software-engineering" | "devops" | "devops-infra" | "tech" | "computers"
        ) || [
            "kubernetes",
            "microservice",
            "backend",
            "dockerfile",
            "pipeline",
            "rust",
            "python",
        ]
        .iter()
        .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "architecture_outline");
            push_unique(deliverables, "verification_steps");
        } else if domain == "finance"
            || [
                "portfolio",
                "asset allocation",
                "cashflow",
                "retirement",
                "sharpe",
            ]
            .iter()
            .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "risk_breakdown");
            push_unique(deliverables, "allocation_summary");
        } else if domain == "legal"
            || [
                "contract",
                "statute",
                "indemnification",
                "gdpr",
                "hipaa",
                "nda",
            ]
            .iter()
            .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "statutory_basis");
            push_unique(deliverables, "compliance_checklist");
        } else if matches!(domain, "medical" | "health" | "pharma")
            || ["clinical", "patient", "diagnosis", "symptom", "biomarker"]
                .iter()
                .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "clinical_considerations");
            push_unique(deliverables, "safety_precautions");
        } else if domain == "travel"
            || ["itinerary", "vacation", "sightseeing", "flight", "hotel"]
                .iter()
                .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "daily_itinerary");
            push_unique(deliverables, "logistics_summary");
        } else if domain == "education"
            || ["curriculum", "syllabus", "pedagogy", "lesson", "student"]
                .iter()
                .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "learning_objectives");
            push_unique(deliverables, "practice_exercises");
        } else if matches!(
            domain,
            "business" | "business-strategy" | "marketing" | "ecommerce"
        ) || ["go-to-market", "monetize", "campaign", "revenue", "startup"]
            .iter()
            .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "action_plan");
            push_unique(deliverables, "kpi_metrics");
        } else if domain == "real-estate"
            || ["property", "mortgage", "cap rate", "reit", "multifamily"]
                .iter()
                .any(|k| combined.contains(k))
        {
            push_unique(deliverables, "market_comparables");
            push_unique(deliverables, "roi_projection");
        }
    }

    /// Checks if all texts are vague/uninformative (e.g., "unknown", "thing", etc.).
    fn all_texts_vague(&self, texts: &[String]) -> bool {
        if texts.is_empty() {
            return true;
        }
        let vague_words = [
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
        let task_keywords = [
            "build",
            "create",
            "make",
            "develop",
            "implement",
            "design",
            "write",
            "code",
            "analyze",
            "evaluate",
            "plan",
            "debug",
            "optimize",
            "explain",
            "summarize",
            "review",
        ];
        for keyword in task_keywords {
            if wm_texts.iter().any(|t| t.to_lowercase().contains(keyword)) {
                return Some(format!("Perform {} action", keyword));
            }
        }

        if let Some(t) = self.domain_knowledge.get("task") {
            return Some(t.clone());
        }

        if !self.all_texts_vague(wm_texts) {
            if let Some(t) = wm_texts.first() {
                return Some(t.clone());
            }
        }

        Some("Analyze and process the provided context".to_string())
    }

    /// Infers the most specific domain persona/role from `ScenarioSignal` first,
    /// then falls back to expanded keyword matching across all 22 canonical domains.
    fn infer_role(
        &self,
        wm_texts: &[String],
        all_sources: &[String],
        scenario: Option<&ScenarioSignal>,
    ) -> Option<String> {
        // 1. Check ScenarioSignal first (`primary_role` or `domain` mapping across all 22 canonical domains)
        if let Some(sig) = scenario {
            if let Some(ref primary) = sig.primary_role {
                let trimmed = primary.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(mapped) = Self::role_for_canonical_domain(&sig.domain) {
                return Some(mapped.to_string());
            }
        }

        // 2. Expanded keyword matching across WM slots and all sources
        let role_map: &[(&str, &str)] = &[
            ("kubernetes", "DevOps & Cloud Architect"),
            ("docker", "DevOps & Cloud Architect"),
            ("devops", "DevOps & Cloud Architect"),
            ("terraform", "DevOps & Cloud Architect"),
            ("pipeline", "DevOps & Cloud Architect"),
            ("cybersecurity", "Cybersecurity Architect"),
            ("vulnerability", "Cybersecurity Architect"),
            ("encryption", "Cybersecurity Architect"),
            ("firewall", "Cybersecurity Architect"),
            ("security", "Cybersecurity Architect"),
            ("transformer", "AI/ML Systems Architect"),
            ("neural", "AI/ML Systems Architect"),
            ("llm", "AI/ML Systems Architect"),
            ("machine learning", "AI/ML Systems Architect"),
            ("dataset", "Senior Data Scientist"),
            ("regression", "Senior Data Scientist"),
            ("nutrition", "Registered Sports & Clinical Dietitian"),
            ("meal", "Registered Sports & Clinical Dietitian"),
            ("calorie", "Registered Sports & Clinical Dietitian"),
            ("macro", "Registered Sports & Clinical Dietitian"),
            ("diet", "Registered Sports & Clinical Dietitian"),
            ("pharma", "Pharmaceutical Strategy & Regulatory Specialist"),
            ("biotech", "Pharmaceutical Strategy & Regulatory Specialist"),
            ("clinical", "Clinical Medical Specialist"),
            ("patient", "Clinical Medical Specialist"),
            ("diagnosis", "Clinical Medical Specialist"),
            ("medical", "Clinical Medical Specialist"),
            ("hospital", "Clinical Medical Specialist"),
            ("health", "Healthcare & Wellness Specialist"),
            ("wellness", "Healthcare & Wellness Specialist"),
            ("itinerary", "Senior Travel & Logistics Planner"),
            ("travel", "Senior Travel & Logistics Planner"),
            ("vacation", "Senior Travel & Logistics Planner"),
            ("flight", "Senior Travel & Logistics Planner"),
            ("hotel", "Senior Travel & Logistics Planner"),
            ("curriculum", "Curriculum & Instructional Designer"),
            ("syllabus", "Curriculum & Instructional Designer"),
            ("pedagogy", "Curriculum & Instructional Designer"),
            ("teach", "Curriculum & Instructional Designer"),
            ("course", "Curriculum & Instructional Designer"),
            ("mortgage", "Real Estate Investment Strategist"),
            ("reit", "Real Estate Investment Strategist"),
            ("property", "Real Estate Investment Strategist"),
            ("real estate", "Real Estate Investment Strategist"),
            ("legal", "Senior Legal Advisor"),
            ("contract", "Senior Legal Advisor"),
            ("statute", "Senior Legal Advisor"),
            ("compliance", "Senior Legal Advisor"),
            ("portfolio", "Chartered Financial Analyst"),
            ("investment", "Chartered Financial Analyst"),
            ("revenue", "Chartered Financial Analyst"),
            ("finance", "Chartered Financial Analyst"),
            ("ecommerce", "E-Commerce Growth Architect"),
            ("shopify", "E-Commerce Growth Architect"),
            ("storefront", "E-Commerce Growth Architect"),
            ("marketing", "Growth Marketing Strategist"),
            ("campaign", "Growth Marketing Strategist"),
            ("seo", "Growth Marketing Strategist"),
            ("brand", "Growth Marketing Strategist"),
            ("business", "Senior Business Strategist"),
            ("startup", "Senior Business Strategist"),
            ("monetize", "Senior Business Strategist"),
            ("kernel", "Computer Systems Architect"),
            ("cpu", "Computer Systems Architect"),
            ("linux", "Computer Systems Architect"),
            ("physics", "Lead Research Scientist"),
            ("biology", "Research Biologist"),
            ("chemistry", "Lead Research Scientist"),
            ("experiment", "Lead Research Scientist"),
            ("code", "Expert Software Engineer"),
            ("python", "Expert Software Engineer"),
            ("rust", "Expert Software Engineer"),
            ("develop", "Expert Software Engineer"),
            ("function", "Expert Software Engineer"),
            ("design", "Architecture Specialist"),
            ("data", "Senior Data Scientist"),
            ("write", "Professional Copywriter"),
        ];

        for (keyword, role) in role_map {
            if wm_texts.iter().any(|t| t.to_lowercase().contains(keyword))
                || all_sources
                    .iter()
                    .any(|t| t.to_lowercase().contains(keyword))
            {
                return Some((*role).to_string());
            }
        }

        // Fallback to a general expert role
        Some("Domain Expert".to_string())
    }

    /// Maps any of the 22 canonical domains (and common aliases) to a domain-accurate persona title.
    #[must_use]
    pub fn role_for_canonical_domain(domain: &str) -> Option<&'static str> {
        match domain.to_lowercase().as_str() {
            "software" | "software-engineering" | "tech" => Some("Principal Software Architect"),
            "computers" => Some("Computer Systems Architect"),
            "devops" | "devops-infra" => Some("Staff Site Reliability & DevOps Engineer"),
            "ai-ml" => Some("AI/ML Systems Architect"),
            "data-science" => Some("Principal Data Scientist"),
            "cybersecurity" => Some("Principal Security Architect"),
            "finance" => Some("Chartered Financial Analyst"),
            "legal" => Some("Senior Legal Counsel"),
            "business" | "business-strategy" => Some("Senior Business Strategist"),
            "marketing" => Some("Growth Marketing Strategist"),
            "ecommerce" => Some("E-Commerce Growth Architect"),
            "real-estate" => Some("Real Estate Investment Strategist"),
            "medical" => Some("Clinical Medical Specialist"),
            "health" | "health-fitness" => Some("Clinical Health & Wellness Specialist"),
            "pharma" => Some("Pharmaceutical Strategy & Regulatory Specialist"),
            "nutrition" | "sports-nutrition" => Some("Registered Sports & Clinical Dietitian"),
            "education" => Some("Curriculum & Instructional Designer"),
            "travel" => Some("Senior Travel & Logistics Planner"),
            "science" | "scientific-research" => Some("Lead Research Scientist"),
            "workplace-productivity" => Some("Organizational & Productivity Strategist"),
            "creative" | "creative-writing" => Some("Principal Creative Director & Writer"),
            "career-growth" => Some("Executive Career Strategist"),
            _ => None,
        }
    }

    fn infer_context(
        &self,
        wm_texts: &[String],
        _all_sources: &[String],
        cluster_texts: &[String],
    ) -> Vec<String> {
        let mut context = Vec::new();
        let vague_words = ["unknown", "thing", "something", "anything", "etc"];

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

        for text in cluster_texts.iter().take(2) {
            context.push(text.clone());
        }

        context
    }

    fn detect_null_fields(&self, task: &Option<String>, context: &[String]) -> Vec<String> {
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

/// Output produced by [`SchemaFilling::fill`].
#[derive(Debug, Default)]
pub struct FillResult {
    pub task: Option<String>,
    pub role: Option<String>,
    pub context: Vec<String>,
    pub constraints: Vec<Constraint>,
    pub output: Vec<Deliverable>,
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
            salience,
            source: crate::types::SlotSource::Delta,
            is_protected: false,
            last_accessed: std::time::Instant::now(),
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
