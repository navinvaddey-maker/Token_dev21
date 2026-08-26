// Removed unused serde imports
use chrono::{DateTime, Utc};
use anyhow::Result;

use crate::npae::schema::types::{
    ClarifyingQuestion, ConstraintsMeta, ExecutionPhase,
    HallucinationGuardConfig, LengthBound, PromptConstraints,
    PromptContext, PromptRole, StructuredPrompt, ValidationStep,
};
use super::intent::IntentProfile;

#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    Cli,
    Http { remote_addr: String, route: String },
    File { path: String, line_number: usize },
    Sdk { api_key_prefix: String, model: String },
    WebSocket { session_id: String },
    Queue { topic: String, message_id: String },
    Internal { caller: String },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum TrustLevel {
    High,
    Medium,
    Low,
    Untrusted
}

impl PromptSource {
    pub fn trust_level(&self) -> TrustLevel {
        match self {
            PromptSource::Internal { .. }                => TrustLevel::High,
            PromptSource::Sdk { .. }                     => TrustLevel::High,
            PromptSource::Http { .. }                    => TrustLevel::Medium,
            PromptSource::WebSocket { .. }               => TrustLevel::Medium,
            PromptSource::Cli                            => TrustLevel::Low,
            PromptSource::File { .. }                    => TrustLevel::Low,
            PromptSource::Queue { .. }                   => TrustLevel::Untrusted,
            PromptSource::Unknown                        => TrustLevel::Untrusted,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawInput {
    pub text: String,
    pub source: PromptSource,
    pub trust_level: TrustLevel,
    pub metadata: InputMetadata,
}

#[derive(Debug, Clone)]
pub struct InputMetadata {
    pub request_id: String,
    pub received_at: DateTime<Utc>,
    pub language_hint: Option<String>,
    pub max_tokens_hint: Option<usize>,
}

impl InputMetadata {
    pub fn new() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            received_at: Utc::now(),
            language_hint: None,
            max_tokens_hint: None,
        }
    }
}

pub trait Structurer: Send + Sync {
    fn collect(&self) -> Result<RawInput>;
    fn source_name(&self) -> &'static str;
    fn pre_validate(&self, _input: &RawInput) -> Result<()> {
        Ok(())
    }
}

pub struct CliStructurer {
    pub max_length: usize,
}

impl Structurer for CliStructurer {
    fn collect(&self) -> Result<RawInput> {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let text = line.trim().to_string();
        
        if text.is_empty() { return Err(anyhow::anyhow!("Empty prompt")); }
        if text.len() > self.max_length {
            return Err(anyhow::anyhow!("Prompt too long: {} > {}", text.len(), self.max_length));
        }

        Ok(RawInput {
            text,
            source: PromptSource::Cli,
            trust_level: TrustLevel::Low,
            metadata: InputMetadata::new(),
        })
    }

    fn source_name(&self) -> &'static str { "cli" }
}

pub struct HttpStructurer {
    pub route: String,
    pub remote_addr: String,
    pub body: String,
}

impl Structurer for HttpStructurer {
    fn collect(&self) -> Result<RawInput> {
        Ok(RawInput {
            text: self.body.clone(),
            source: PromptSource::Http {
                remote_addr: self.remote_addr.clone(),
                route: self.route.clone(),
            },
            trust_level: TrustLevel::Medium,
            metadata: InputMetadata::new(),
        })
    }

    fn source_name(&self) -> &'static str { "http" }

    fn pre_validate(&self, input: &RawInput) -> Result<()> {
        let lower = input.text.to_lowercase();
        let dangerous_patterns = [
            "<script", "javascript:", "onerror=", "onload=",
            "drop table", "delete from", "insert into", "union select",
            "'; --", "1=1", "or 1=1",
        ];
        if dangerous_patterns.iter().any(|p| lower.contains(p)) {
            return Err(anyhow::anyhow!("Potentially dangerous content detected"));
        }
        // Also check URL-decoded variants
        let decoded = urlencoding::decode(&input.text).unwrap_or_default();
        if dangerous_patterns.iter().any(|p| decoded.to_lowercase().contains(p)) {
            return Err(anyhow::anyhow!("Encoded dangerous content detected"));
        }
        Ok(())
    }
}

// ... Additional structurers (File, SDK, Queue) would be implemented similarly

// --- CRISP Prompt Renderer (v2 — token waste eliminated) ---

pub fn split_raw_input(raw: &str) -> (Option<String>, Option<String>, String) {
    let mut context = None;
    let mut rag = None;
    let mut user_prompt = raw.trim().to_string();

    if user_prompt.starts_with("Context: [") {
        if let Some(end_idx) = user_prompt.find("]\n\n") {
            context = Some(user_prompt[10..end_idx].to_string());
            user_prompt = user_prompt[end_idx + 3..].trim().to_string();
        }
    }

    if let Some(start_idx) = user_prompt.find("--- Retrieved Knowledge ---") {
        if let Some(end_idx) = user_prompt.find("--- End Retrieved Knowledge ---\n") {
            rag = Some(user_prompt[start_idx + "--- Retrieved Knowledge ---\n".len()..end_idx].trim().to_string());
            user_prompt = user_prompt[end_idx + "--- End Retrieved Knowledge ---\n".len()..].trim().to_string();
        }
    }

    (context, rag, user_prompt)
}

pub fn render_crisp_prompt(prompt: &StructuredPrompt, questions: &[ClarifyingQuestion], raw_input: &str) -> String {
    let (context_opt, rag_opt, user_prompt) = split_raw_input(raw_input);
    let mut out = String::new();
    
    // ROLE — High-resolution persona
    let is_role_valid = !prompt.role.primary.is_empty() 
        && prompt.role.primary.to_lowercase() != "unknown"
        && prompt.role.primary.to_lowercase() != "none"
        && prompt.role.primary.to_lowercase() != "default";

    let guardrails: &Vec<String> = &prompt.role.persona_constraints;

    if is_role_valid || !prompt.role.persona_anchor.is_empty() || !guardrails.is_empty() {
        if is_role_valid {
            out.push_str(&format!("# ROLE: {}\n", prompt.role.primary.to_uppercase()));
        }
        if !prompt.role.persona_anchor.is_empty() {
            out.push_str(&format!("Profile: {}\n", prompt.role.persona_anchor));
        }
        if !guardrails.is_empty() {
            out.push_str(&format!("Guardrails: {}\n", guardrails.join(", ")));
        }
        out.push('\n');
    }
 
    // CONTEXT — Dense metadata, no redundancy
    let is_ctx_desc_valid = !prompt.context.description.is_empty()
        && prompt.context.description.to_lowercase() != "unknown"
        && prompt.context.description.to_lowercase() != "none"
        && prompt.context.description.to_lowercase() != "default";

    let has_bg = !prompt.context.background.is_empty() && prompt.context.background != prompt.context.description;
    let has_assumptions = !prompt.context.assumptions.is_empty();

    if is_ctx_desc_valid || has_bg || has_assumptions || context_opt.is_some() {
        if is_ctx_desc_valid {
            out.push_str(&format!("# CONTEXT: {}\n", prompt.context.description));
        } else {
            out.push_str("# CONTEXT:\n");
        }
        
        let mut meta = Vec::new();
        let is_knowledge_valid = !prompt.context.user_knowledge_level.is_empty() && prompt.context.user_knowledge_level.to_lowercase() != "unknown";
        let is_domain_valid = !prompt.context.domain.is_empty() && prompt.context.domain.to_lowercase() != "unknown";
        
        if is_knowledge_valid || is_domain_valid {
            let knowledge = if is_knowledge_valid { &prompt.context.user_knowledge_level } else { "" };
            let domain = if is_domain_valid { &prompt.context.domain } else { "" };
            meta.push(format!("Expertise: {} {}", knowledge, domain).trim().to_string());
        }
        
        if prompt.context.temporal_scope != "unspecified" && prompt.context.temporal_scope != "immediate" && !prompt.context.temporal_scope.is_empty() {
            meta.push(format!("Scope: {}", prompt.context.temporal_scope));
        }
        
        if !meta.is_empty() {
            out.push_str(&meta.join(" | "));
            out.push('\n');
        }
 
        if has_bg {
            out.push_str(&format!("Details: {}\n", prompt.context.background));
        }
        if has_assumptions {
            out.push_str(&format!("Assumptions: {}\n", prompt.context.assumptions.join("; ")));
        }
        if let Some(ctx) = context_opt {
            out.push_str(&format!("Semantic Context: {}\n", ctx));
        }
        out.push('\n');
    }

    if let Some(rag) = rag_opt {
        out.push_str("# SOURCE KNOWLEDGE:\n");
        out.push_str(&rag);
        out.push_str("\n\n");
    }

    // CONSTRAINTS — only non-empty sections
    let has_inclusions = !prompt.constraints.required_inclusions.is_empty();
    let has_exclusions = !prompt.constraints.forbidden_topics.is_empty();
    let has_custom_format = prompt.constraints.output_format != "markdown" && !prompt.constraints.output_format.is_empty();
    let has_custom_tone = prompt.constraints.tone.to_lowercase() != "neutral" && !prompt.constraints.tone.is_empty() && prompt.constraints.tone.to_lowercase() != "default";

    if has_inclusions || has_exclusions || has_custom_format || has_custom_tone {
        out.push_str("# CONSTRAINTS:\n");
        for inc in &prompt.constraints.required_inclusions {
            out.push_str(&format!("- [STRICT] {}\n", inc));
        }
        for exc in &prompt.constraints.forbidden_topics {
            out.push_str(&format!("- [AVOID]  {}\n", exc));
        }
        
        if has_custom_format || has_custom_tone {
            let mut fmts = Vec::new();
            if has_custom_format {
                fmts.push(format!("Output Format: {}", prompt.constraints.output_format));
            }
            if has_custom_tone {
                fmts.push(format!("Tone: {}", prompt.constraints.tone));
            }
            out.push_str(&format!("{}\n", fmts.join(" | ")));
        }
        out.push('\n');
    }

    // CONSTRAINTS META — geography, risk, business type, team (only if detected)
    if let Some(ref meta) = prompt.constraints_meta {
        let mut meta_parts = Vec::new();
        if let Some(ref g) = meta.geography { meta_parts.push(format!("Geography: {}", g)); }
        if let Some(ref r) = meta.risk_tolerance { meta_parts.push(format!("Risk: {}", r)); }
        if let Some(ref b) = meta.business_type { meta_parts.push(format!("Type: {}", b)); }
        if let Some(ref t) = meta.team_composition { meta_parts.push(format!("Team: {}", t)); }
        if let Some(ref rev) = meta.revenue_expectations { meta_parts.push(format!("Revenue: {}", rev)); }
        if !meta_parts.is_empty() {
            out.push_str(&format!("# CONTEXT CONSTRAINTS:\n{}\n\n", meta_parts.join(" | ")));
        }
    }

    // EXECUTION PHASES — only if detected (roadmap vs ideas)
    if !prompt.execution_phases.is_empty() {
        out.push_str("# EXECUTION PHASES:\n");
        for phase in &prompt.execution_phases {
            out.push_str(&format!("{}. **{}** ({}): {}\n", 
                phase.phase_number, phase.name, phase.estimated_duration, phase.description));
            for d in &phase.deliverables {
                out.push_str(&format!("   - {}\n", d));
            }
        }
        out.push('\n');
    }

    // VALIDATION STEPS — only if detected
    if !prompt.validation_steps.is_empty() {
        out.push_str("# VALIDATION:\n");
        for step in &prompt.validation_steps {
            out.push_str(&format!("{}. {} — {} ({})\n", 
                step.step_number, step.name, step.criteria, step.validation_method));
        }
        out.push('\n');
    }

    // SUCCESS CRITERIA
    if !prompt.success_criteria.is_empty() {
        out.push_str("# SUCCESS CRITERIA:\n");
        for sc in &prompt.success_criteria {
            out.push_str(&format!("- {}\n", sc));
        }
        out.push('\n');
    }

    // CLARIFYING QUESTIONS — only if questions actually exist (no dead reference)
    if !questions.is_empty() {
        out.push_str("# CLARIFYING QUESTIONS:\n");
        for q in questions {
            out.push_str(&format!("- [{}] {}\n", q.priority.to_string().to_uppercase(), q.question));
        }
        out.push('\n');
    }

    // FINAL INSTRUCTION — adaptive based on intent
    out.push_str("# TASK / INSTRUCTION:\n");
    out.push_str(&user_prompt);
    out.push_str("\n\n---\n");
    out.push_str(&prompt.dynamic_instruction);
    if !questions.is_empty() {
        out.push_str(" Address CLARIFYING QUESTIONS in your response.");
    }
    if !prompt.execution_phases.is_empty() {
        out.push_str(" Structure your response according to the EXECUTION PHASES.");
    }
    if !prompt.validation_steps.is_empty() {
        out.push_str(" Include VALIDATION checkpoints for each deliverable.");
    }
    out.push('\n');

    out
}

impl std::fmt::Display for crate::npae::schema::types::Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            crate::npae::schema::types::Priority::High => write!(f, "high"),
            crate::npae::schema::types::Priority::Medium => write!(f, "medium"),
            crate::npae::schema::types::Priority::Low => write!(f, "low"),
        }
    }
}

pub fn build(profile: &IntentProfile, raw: &str, resolved: &super::resolver::ResolvedPrompt, config: Option<&super::config::UnifiedConfig>) -> std::result::Result<StructuredPrompt, String> {
    let role_primary = resolved.role.clone();
    let inclusions = resolved.inclusions.clone();
    let forbidden = resolved.forbidden.clone();

    let user_level_str = match profile.user_knowledge {
        super::intent::KnowledgeLevel::Novice => "novice",
        super::intent::KnowledgeLevel::Intermediate => "intermediate",
        super::intent::KnowledgeLevel::Expert => "expert",
    }.to_string();

    let intent_type = match profile.primary_intent {
        super::intent::IntentClass::Build => if profile.domain == "software-engineering" || profile.domain == "devops-infra" { "development/construction" } else { "creation/design" },
        super::intent::IntentClass::Explain => "educational/instructional",
        super::intent::IntentClass::Debug => "troubleshooting/optimization",
        super::intent::IntentClass::Analyze => "analytical/discovery",
        super::intent::IntentClass::Transform => "refactoring/conversion",
    };

    // Extract a concise task description (not just raw first line)
    let description = extract_task_description(raw, intent_type);

    // Build a non-redundant background — includes goal decomposition & context unpacking
    let background = build_background(profile, &user_level_str, raw);

    // Infer execution phases based on temporal scope, domain templates, and intent
    let execution_phases = infer_execution_phases(profile, raw, config);

    // Infer validation steps if validation signals detected
    let validation_steps = infer_validation_steps(profile, raw);

    // Infer success criteria (Intent-driven)
    let success_criteria = infer_success_criteria(profile, raw);

    // Infer implicit assumptions based on context/domain
    let assumptions = infer_assumptions(profile, raw);

    // Build dynamic instruction
    let dynamic_instruction = derive_dynamic_instruction(profile, raw);

    // Build constraints meta from detected signals
    let constraints_meta = build_constraints_meta(profile);

    // Derive appropriate tone from domain + knowledge level
    let tone = derive_tone(profile);

    // Derive high-resolution persona anchor
    let persona_anchor = derive_persona_anchor(&profile.domain, &role_primary, config);

    // GAP-13: Derive length bounds from input complexity and intent
    let word_count = raw.split_whitespace().count();
    let length_bound = match (&profile.primary_intent, word_count) {
        (super::intent::IntentClass::Explain, 0..=15) => LengthBound { min: 100, max: 500 },
        (super::intent::IntentClass::Explain, _)      => LengthBound { min: 200, max: 1000 },
        (super::intent::IntentClass::Build,   0..=30) => LengthBound { min: 300, max: 1500 },
        (super::intent::IntentClass::Build,   _)      => LengthBound { min: 500, max: 3000 },
        (super::intent::IntentClass::Debug,   _)      => LengthBound { min: 150, max: 800 },
        (super::intent::IntentClass::Analyze, _)      => LengthBound { min: 300, max: 2000 },
        (super::intent::IntentClass::Transform, _)    => LengthBound { min: 100, max: 1000 },
    };

    Ok(StructuredPrompt {
        role: PromptRole {
            primary: role_primary,
            expertise_domains: vec![profile.domain.clone()],
            persona_constraints: match profile.domain.as_str() {
                "software-engineering" | "devops-infra" => vec![
                    "production_grade_only".into(),
                    "no_placeholder_code".into(),
                ],
                "medical" | "pharma" => vec![
                    "evidence_based_only".into(),
                    "cite_sources_required".into(),
                    "no_medical_advice_disclaimer".into(),
                ],
                "legal" => vec![
                    "jurisdiction_aware".into(),
                    "cite_statutes".into(),
                ],
                "creative" => vec![
                    "maintain_narrative_voice".into(),
                    "show_dont_tell".into(),
                ],
                _ => vec![
                    "accurate_and_thorough".into(),
                ],
            },
            persona_anchor,
        },
        objective: None,
        context: PromptContext {
            domain: profile.domain.clone(),
            description,
            background,
            user_knowledge_level: user_level_str,
            audience: "general".to_string(),
            temporal_scope: profile.temporal_scope.clone(),
            intent_vector: vec![profile.confidence],
            assumptions,
        },
        constraints: PromptConstraints {
            output_format: profile.output_preference.clone(),
            length_bound,
            forbidden_topics: forbidden,
            required_inclusions: inclusions,
            tone,
        },
        hallucination_guard: HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into(), "cite_uncertainty".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
        },
        output_spec: None,
        execution_phases,
        validation_steps,
        success_criteria,
        constraints_meta,
        dynamic_instruction,
    })
}

/// Extract a meaningful task description — summary based, not raw snippet
fn extract_task_description(raw: &str, intent_type: &str) -> String {
    let (_, _, user_prompt) = split_raw_input(raw);
    
    // Attempt to extract a short summary (first 10 words or first sentence)
    let summary = user_prompt.split(|c: char| c == '.' || c == '\n')
        .next()
        .unwrap_or(&user_prompt)
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ");

    format!("{} [Type: {}]", summary, intent_type)
}

/// Build background that adds NEW information — includes context unpacking for vagueness resolution
fn build_background(profile: &IntentProfile, user_level: &str, raw: &str) -> String {
    let mut parts = Vec::new();
    
    parts.push(format!("Domain: {} | Audience: {}", profile.domain, user_level));
    
    // Vagueness Resolution / Context Unpacking
    if let Some(roadmap) = unpack_context(profile, raw) {
        parts.push(format!("Roadmap: {}", roadmap));
    }

    if profile.has_timeline {
        parts.push(format!("Timeline: {}", profile.temporal_scope));
    }
    if profile.has_phases {
        parts.push("Phased execution requested".to_string());
    }
    if profile.has_validation {
        parts.push("Validation/QA gates required".to_string());
    }
    if let Some(ref geo) = profile.detected_geography {
        parts.push(format!("Geography: {}", geo));
    }
    if let Some(ref biz) = profile.detected_biz_type {
        parts.push(format!("Business type: {}", biz));
    }
    
    parts.join(". ")
}

/// Infer execution phases from temporal scope, config templates, and raw prompt
fn infer_execution_phases(profile: &IntentProfile, raw: &str, config: Option<&super::config::UnifiedConfig>) -> Vec<ExecutionPhase> {
    if !profile.has_phases && !profile.has_timeline {
        // No temporal signals — check if this is a build/planning task
        if profile.primary_intent != super::intent::IntentClass::Build 
            && profile.primary_intent != super::intent::IntentClass::Transform {
            return vec![];
        }
    }

    // 1. Generic dynamic lookup from config.domain_taxonomy.phase_templates
    let intent_key = match profile.primary_intent {
        super::intent::IntentClass::Build => "build",
        super::intent::IntentClass::Analyze => "analyze",
        super::intent::IntentClass::Explain => "explain",
        super::intent::IntentClass::Debug => "debug",
        super::intent::IntentClass::Transform => "transform",
    };

    if let Some(cfg) = config {
        // Resolve any alias → canonical domain name before lookup.
        let canonical = super::config::normalize_domain(&profile.domain, &cfg.domain_taxonomy);
        if let Some(tax) = cfg.domain_taxonomy.iter().find(|t| t.domain == canonical) {
            if let Some(ref templates) = tax.phase_templates {
                if let Some(phases) = templates.get(intent_key) {
                    return phases.clone();
                }
            }
        }
    }

    let lower = raw.to_lowercase();
    let mut phases = Vec::new();

    // Resolve domain alias → canonical name for consistent matching.
    // Falls back to profile.domain if no config is available.
    let resolved_domain = if let Some(cfg) = config {
        super::config::normalize_domain(&profile.domain, &cfg.domain_taxonomy).to_owned()
    } else {
        profile.domain.clone()
    };

    match resolved_domain.as_str() {
        "finance" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Financial & Asset Audit (30 Days)".into(),
                description: "Evaluate current active/passive income sources, baseline burn rate, balance sheet assets/debt, and human capital monetization inventory.".into(),
                estimated_duration: "30 days".into(),
                deliverables: vec!["Financial Baseline & Burn Rate Audit".into(), "Human Capital & Monetization Matrix".into(), "Target Definition & Wealth Gap Calculation".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Strategy & Vehicle Selection (90 Days)".into(),
                description: "Select and launch primary high-leverage monetization vehicles (career advancement, high-ticket services, business equity, or dividend/index investing).".into(),
                estimated_duration: "90 days".into(),
                deliverables: vec!["Vehicle Feasibility Matrix".into(), "Income Growth Blueprint".into(), "First Revenue Milestone Report".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Execution & Reinvestment (6-12 Months)".into(),
                description: "Launch revenue-generating initiatives, secure initial cash flows, and configure automated compounding reinvestment.".into(),
                estimated_duration: "6-12 months".into(),
                deliverables: vec!["Execution Action Checklist".into(), "Cash Flow & Surplus Automation Blueprint".into(), "Diversified Asset Allocation Strategy".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 4,
                name: "Equity & Asset Expansion (3-5 Years)".into(),
                description: "Scale business equity, expand productive assets, and optimize capital compounding across market cycles.".into(),
                estimated_duration: "3-5 years".into(),
                deliverables: vec!["Equity & Portfolio Growth Model".into(), "Scenario Stress-Test Report (Conservative/Base/Aggressive)".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 5,
                name: "Wealth Compounding & Preservation (5-10 Years)".into(),
                description: "Maximize compound growth, achieve targeted net worth milestone, and implement estate, tax, and capital preservation structures.".into(),
                estimated_duration: "5-10 years".into(),
                deliverables: vec!["Target Achievement & Wealth Preservation Blueprint".into(), "Financial Freedom Audit".into()],
            });
        }
        "computers" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "System Architecture & Specs".into(),
                description: "Define hardware/software compute requirements, throughput constraints, and interface specifications.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["System Architecture Specification".into(), "Compute & Memory Budget".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Core System Implementation".into(),
                description: "Develop core modules, memory management, concurrency models, and driver/OS integrations.".into(),
                estimated_duration: "1-3 weeks".into(),
                deliverables: vec!["Working System Implementation".into(), "Benchmark Test Suite".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Optimization & Deployment".into(),
                description: "Profile bottlenecks, optimize cache/memory locality, and configure production telemetry.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Performance Audit Report".into(), "Production Configuration".into()],
            });
        }
        "science" | "scientific-research" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Hypothesis & Experimental Design".into(),
                description: "Formulate falsifiable hypotheses, define control variables, and design experimental protocols.".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Formal Hypothesis Document".into(), "Experimental Protocol Blueprint".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Data Acquisition & Execution".into(),
                description: "Execute trials, collect empirical sensor/laboratory data, and ensure methodological reproducibility.".into(),
                estimated_duration: "2-6 weeks".into(),
                deliverables: vec!["Raw Empirical Dataset".into(), "Execution Telemetry Log".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Statistical Analysis & Validation".into(),
                description: "Apply statistical significance testing, error analysis, and peer-review synthesis.".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Statistical Validation Report".into(), "Publication-Ready Synthesis".into()],
            });
        }
        "health" | "medical" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Health Assessment & Baseline Audit".into(),
                description: "Evaluate patient history, baseline biomarkers, physiological symptoms, and lifestyle factors.".into(),
                estimated_duration: "1-3 days".into(),
                deliverables: vec!["Comprehensive Assessment Profile".into(), "Biomarker Baseline Report".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Intervention & Protocol Design".into(),
                description: "Develop evidence-based therapeutic interventions, dosage protocols, and lifestyle adjustments.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Therapeutic Protocol Specification".into(), "Risk & Safety Mitigation Plan".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Monitoring & Long-Term Adaptation".into(),
                description: "Execute health protocol, track biomarker responses, and optimize long-term outcomes.".into(),
                estimated_duration: "2-8 weeks".into(),
                deliverables: vec!["Progress Tracking Log".into(), "Outcome Analysis Report".into()],
            });
        }
        "business" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Research & Validation".into(),
                description: "Market research, competitor analysis, target audience identification".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Market analysis report".into(), "Competitor matrix".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Strategy & Planning".into(),
                description: "Business model design, revenue strategy, operational planning".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Business model canvas".into(), "Revenue model".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Execution & Launch".into(),
                description: "Implementation, go-to-market, initial customer acquisition".into(),
                estimated_duration: "2-4 weeks".into(),
                deliverables: vec!["Launch plan".into(), "First 30-day action items".into()],
            });
        }
        "real-estate" | "real estate" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Licensing & Pre-Registration".into(),
                description: "Complete pre-licensing education coursework, pass state/regional real estate exam, and secure broker sponsorship.".into(),
                estimated_duration: "1-3 months".into(),
                deliverables: vec!["Real Estate License".into(), "Broker Sponsorship Agreement".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Business Setup & GTM Strategy".into(),
                description: "Form business entity (LLC), set up MLS & CRM systems, build personal brand, and establish marketing funnel.".into(),
                estimated_duration: "2-4 weeks".into(),
                deliverables: vec!["Business Entity (LLC)".into(), "Lead Generation & CRM Setup".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Client Acquisition & Expansion".into(),
                description: "Execute prospecting campaigns, handle buyer/seller representations, secure initial listings, and scale brokerage operations.".into(),
                estimated_duration: "Ongoing".into(),
                deliverables: vec!["First Closed Transactions".into(), "Client Referral Pipeline".into()],
            });
        }
        "software" | "devops" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Design & Architecture".into(),
                description: "System design, technology selection, architecture decisions".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Architecture document".into(), "Tech stack decision".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Implementation".into(),
                description: "Core development, testing, iteration".into(),
                estimated_duration: "1-3 weeks".into(),
                deliverables: vec!["Working codebase".into(), "Test suite".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Deployment & Validation".into(),
                description: "Staging deployment, performance testing, production release".into(),
                estimated_duration: "2-3 days".into(),
                deliverables: vec!["Deployed service".into(), "Monitoring setup".into()],
            });
        }
        "nutrition" | "health-fitness" => {
            if lower.contains("week") || lower.contains("day") || lower.contains("plan") {
                phases.push(ExecutionPhase {
                    phase_number: 1,
                    name: "Assessment".into(),
                    description: "Current state analysis, goal setting, constraint identification".into(),
                    estimated_duration: "Day 1".into(),
                    deliverables: vec!["Baseline assessment".into(), "Goal targets".into()],
                });
                phases.push(ExecutionPhase {
                    phase_number: 2,
                    name: "Plan Design".into(),
                    description: "Detailed plan creation with daily/weekly breakdowns".into(),
                    estimated_duration: "Day 1-2".into(),
                    deliverables: vec!["Detailed plan".into(), "Macro targets".into()],
                });
                phases.push(ExecutionPhase {
                    phase_number: 3,
                    name: "Execution & Adjustment".into(),
                    description: "Follow plan, monitor progress, adjust based on feedback".into(),
                    estimated_duration: "Ongoing".into(),
                    deliverables: vec!["Progress tracking".into(), "Adjustment notes".into()],
                });
            }
        }
        "workplace-productivity" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Data Collection & Audit".into(),
                description: "Gather quantitative (output, hours) and qualitative (surveys, sentiment) data".into(),
                estimated_duration: "1 week".into(),
                deliverables: vec!["Data collection framework".into(), "Initial sentiment report".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Comparative Analysis".into(),
                description: "Compare remote vs in-office benchmarks, analyze 'perception vs reality' gap".into(),
                estimated_duration: "1 week".into(),
                deliverables: vec!["Productivity gap analysis".into(), "Variable correlation matrix".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Strategic Recommendation".into(),
                description: "Propose hybrid/remote optimizations, cultural adjustments, and KPI realignment".into(),
                estimated_duration: "1 week".into(),
                deliverables: vec!["Strategic optimization roadmap".into(), "Policy adjustment guide".into()],
            });
        }
        "education" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Knowledge Mapping".into(),
                description: "Identify core concepts, current knowledge gaps, and learning objectives".into(),
                estimated_duration: "Immediate".into(),
                deliverables: vec!["Concept map".into(), "Gap analysis".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Skill Acquisition".into(),
                description: "Implement high-leverage learning techniques (e.g., active recall, interleaving)".into(),
                estimated_duration: "Daily".into(),
                deliverables: vec!["Study protocol".into(), "Practice modules".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Application & Mastery".into(),
                description: "Verify understanding through teaching, problem-solving, and real-world application".into(),
                estimated_duration: "Ongoing".into(),
                deliverables: vec!["Mastery check".into(), "Feedback loop".into()],
            });
        }
        _ => {
            // INTENT-DRIVEN TEMPLATES for unknown domains (All-Type Prompt Support)
            let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "Project".into());
            
            match profile.primary_intent {
                super::intent::IntentClass::Analyze => {
                    phases.push(ExecutionPhase {
                        phase_number: 1,
                        name: format!("{} Discovery", subject),
                        description: format!("Identify key variables, stakeholders, and data sources for {}.", subject).into(),
                        estimated_duration: "Phase 1".into(),
                        deliverables: vec![format!("{} assessment report", subject).into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 2,
                        name: format!("{} Deep Dive", subject),
                        description: format!("Perform detailed analytical investigation into {} dynamics.", subject).into(),
                        estimated_duration: "Phase 2".into(),
                        deliverables: vec![format!("{} analytical model", subject).into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 3,
                        name: "Insights & Strategy".into(),
                        description: format!("Synthesize findings into actionable recommendations for {}.", subject).into(),
                        estimated_duration: "Phase 3".into(),
                        deliverables: vec!["Strategic recommendation brief".into()],
                    });
                }
                super::intent::IntentClass::Build => {
                    let is_software_like = matches!(resolved_domain.as_str(),
                        "software" | "devops" | "ai-ml");

                    if is_software_like {
                        phases.push(ExecutionPhase {
                            phase_number: 1,
                            name: "Architecture & Design".into(),
                            description: format!("Define structural requirements and design for {}.", subject).into(),
                            estimated_duration: "Design Phase".into(),
                            deliverables: vec![format!("{} blueprint", subject).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 2,
                            name: "Core Implementation".into(),
                            description: format!("Construct the functional components of {}.", subject).into(),
                            estimated_duration: "Build Phase".into(),
                            deliverables: vec![format!("Functional {} prototype", subject).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 3,
                            name: "Validation & Polish".into(),
                            description: format!("Test, refine, and finalize {} for production.", subject).into(),
                            estimated_duration: "Final Phase".into(),
                            deliverables: vec![format!("Optimized {}", subject).into()],
                        });
                    } else {
                        phases.push(ExecutionPhase {
                            phase_number: 1,
                            name: "Planning & Strategy".into(),
                            description: format!("Define goals, foundational requirements, and operational roadmap for {}.", subject).into(),
                            estimated_duration: "Phase 1".into(),
                            deliverables: vec![format!("{} strategy roadmap", subject).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 2,
                            name: "Execution & Setup".into(),
                            description: format!("Establish core operational assets, compliance, and systems for {}.", subject).into(),
                            estimated_duration: "Phase 2".into(),
                            deliverables: vec![format!("Functional {} setup", subject).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 3,
                            name: "Launch & Optimization".into(),
                            description: format!("Deploy, monitor initial outcomes, and optimize performance for {}.", subject).into(),
                            estimated_duration: "Phase 3".into(),
                            deliverables: vec![format!("Launched {}", subject).into()],
                        });
                    }
                }
                _ => {
                    // Minimal fallback for other intents
                    phases.push(ExecutionPhase {
                        phase_number: 1,
                        name: "Preparation".into(),
                        description: format!("Set up environment and context for {}.", subject).into(),
                        estimated_duration: "Start".into(),
                        deliverables: vec!["Initial setup".into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 2,
                        name: "Execution".into(),
                        description: format!("Execute core {} tasks.", subject).into(),
                        estimated_duration: "Execution".into(),
                        deliverables: vec![format!("{} core output", subject).into()],
                    });
                }
            }
        }
    }

    phases
}

/// Infer validation steps if validation signals detected
fn infer_validation_steps(profile: &IntentProfile, _raw: &str) -> Vec<ValidationStep> {
    if !profile.has_validation {
        return vec![];
    }

    match profile.domain.as_str() {
        "software-engineering" | "devops-infra" => vec![
            ValidationStep { step_number: 1, name: "Unit Tests".into(), criteria: "All unit tests pass".into(), validation_method: "Automated test suite".into() },
            ValidationStep { step_number: 2, name: "Integration Test".into(), criteria: "End-to-end flow verified".into(), validation_method: "Integration test suite".into() },
            ValidationStep { step_number: 3, name: "Performance Check".into(), criteria: "Meets latency/throughput targets".into(), validation_method: "Load testing".into() },
        ],
        "business-strategy" => vec![
            ValidationStep { step_number: 1, name: "Market Fit".into(), criteria: "Target market size validated".into(), validation_method: "TAM/SAM/SOM analysis".into() },
            ValidationStep { step_number: 2, name: "Financial Viability".into(), criteria: "Unit economics positive".into(), validation_method: "Financial model review".into() },
            ValidationStep { step_number: 3, name: "Competitive Position".into(), criteria: "Clear differentiation identified".into(), validation_method: "Competitor analysis".into() },
        ],
        _ => vec![
            ValidationStep { step_number: 1, name: "Completeness".into(), criteria: "All deliverables produced".into(), validation_method: "Checklist review".into() },
            ValidationStep { step_number: 2, name: "Quality".into(), criteria: "Meets stated requirements".into(), validation_method: "Peer review".into() },
        ],
    }
}

/// Infer success criteria from profile and raw prompt — intent and domain driven
fn infer_success_criteria(profile: &IntentProfile, raw: &str) -> Vec<String> {
    let mut criteria = Vec::new();
    let lower = raw.to_lowercase();
    let is_financial = profile.domain == "finance" || lower.contains("earn") || lower.contains("wealth") || lower.contains("million") || lower.contains("billion") || lower.contains("rich") || lower.contains("financial freedom") || lower.contains("money") || lower.contains("passive income") || lower.contains("retire early") || lower.contains("make $");
    let is_business = profile.domain == "business" || lower.contains("business") || lower.contains("startup") || lower.contains("company") || lower.contains("enterprise");

    if is_financial && !profile.domain.starts_with("workplace") {
        criteria.push("Target is clearly defined with explicit amount ($), currency, time horizon, and target type (Distinguish: Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth)".into());
        criteria.push("Baseline financial and human capital inventory is fully quantified with transparent assumptions".into());
        criteria.push("Wealth gap, required annual savings, and required CAGR are calculated with mathematical rigor".into());
        criteria.push("Income engines and leverage vectors (skills, technology, capital, equity) are evaluated and ranked by ROI and scalability".into());
        criteria.push("Capital allocation framework balances liquidity reserves, asset compounding, and business reinvestment".into());
        criteria.push("Conservative, Base Case, and Aggressive scenario models are articulated with distinct assumptions".into());
        criteria.push("Downside risks (income, market, debt, tax, concentration) have concrete mitigation strategies".into());
        criteria.push("Execution timeline outlines 30-day, 90-day, 12-month, 3-year, and 5-10-year milestones with measurable KPIs".into());
        criteria.push("Includes explicit Reality-Check verdict assessing mathematical feasibility against stated constraints".into());
        return criteria;
    }

    if is_business {
        criteria.push("Business model defines clear revenue streams, pricing tiers, and positive unit economics (LTV/CAC, gross margin)".into());
        criteria.push("Scalability vectors (distribution channels, technology leverage, team delegation) are explicitly mapped".into());
        criteria.push("Valuation roadmap differentiates annual revenue/EBITDA from enterprise valuation multiples".into());
        criteria.push("Capital requirements and cash flow runway are modeled across growth phases".into());
        criteria.push("Includes measurable revenue, customer acquisition, and retention KPIs".into());
        return criteria;
    }

    // Domain-Aware Success Criteria for other domains
    match (profile.primary_intent.clone(), profile.domain.as_str()) {
        (super::intent::IntentClass::Build, "software") | (super::intent::IntentClass::Build, "software-engineering") | (super::intent::IntentClass::Build, "devops") | (super::intent::IntentClass::Build, "devops-infra") => {
            criteria.push("Functional, bug-free implementation with optimized performance".into());
            criteria.push("Adheres to industry-standard architectural patterns and best practices".into());
            criteria.push("Includes necessary technical documentation and test coverage".into());
        }
        (super::intent::IntentClass::Build, "ai-ml") | (super::intent::IntentClass::Build, "data-science") => {
            criteria.push("Model performance meets or exceeds stated benchmarks".into());
            criteria.push("Alignment, safety, and ethical considerations are explicitly addressed".into());
            criteria.push("Data handling and inference pipelines are robust and scalable".into());
        }
        (super::intent::IntentClass::Build, "health") | (super::intent::IntentClass::Build, "medical") | (super::intent::IntentClass::Build, "pharma") => {
            criteria.push("Clinically accurate protocols grounded in peer-reviewed evidence".into());
            criteria.push("Strict adherence to regulatory (FDA/EMA) and ethical guidelines".into());
            criteria.push("Risk-benefit analysis is comprehensive and clearly stated".into());
        }
        (super::intent::IntentClass::Build, "nutrition") | (super::intent::IntentClass::Build, "sports-nutrition") | (super::intent::IntentClass::Build, "health-fitness") => {
            criteria.push("Plan is physiologically sound and tailored to specific goals".into());
            criteria.push("Macros and micronutrients are balanced according to activity level".into());
            criteria.push("Includes clear instructions for tracking and adjustment".into());
        }
        (_, "workplace-productivity") => {
            criteria.push("Differentiates between objective output and subjective sentiment".into());
            criteria.push("Identifies hidden bottlenecks in distributed collaboration".into());
            criteria.push("Provides actionable recommendations for burnout prevention and engagement".into());
            criteria.push("Aligns productivity metrics with organizational culture goals".into());
        }
        (_, "education") => {
            criteria.push("Learning objectives are clearly defined and measurable".into());
            criteria.push("Techniques used are evidence-based and optimized for retention".into());
            criteria.push("Roadmap addresses both theoretical understanding and practical application".into());
            criteria.push("Progress can be tracked through objective mastery checks".into());
        }
        (super::intent::IntentClass::Build, _) => {
            let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "deliverable".into());
            criteria.push(format!("Functional, high-quality {} is produced with modular design.", subject).into());
            criteria.push("Strict adherence to specified requirements and best practices.".into());
        }

        // EXPLAIN Intent (Domain-Agnostic but pedagogical)
        (super::intent::IntentClass::Explain, _) => {
            let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "concept".into());
            criteria.push(format!("Conceptually clear explanation of {} at the target audience level.", subject).into());
            criteria.push("Balances technical depth with intuitive clarity.".into());
            criteria.push("Covers all major dimensions (capabilities, risks, and implications)".into());
        }

        // DEBUG Intent
        (super::intent::IntentClass::Debug, _) => {
            criteria.push("Root cause of the issue is correctly identified and explained".into());
            criteria.push("Permanent fix is implemented and verified for correctness".into());
            criteria.push("No regressions or side-effects are introduced".into());
        }

        // ANALYZE Intent
        (super::intent::IntentClass::Analyze, _) => {
            let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "data".into());
            criteria.push(format!("Deep insights and actionable patterns are extracted for {}.", subject).into());
            criteria.push("Reasoning is clear, logical, and supported by evidence.".into());
            criteria.push("Conclusions are prioritized by impact and feasibility".into());
        }

        // TRANSFORM Intent
        (super::intent::IntentClass::Transform, _) => {
            criteria.push("Successful transformation with 100% data/logic integrity".into());
            criteria.push("Output format strictly adheres to the requested specification".into());
            criteria.push("Redundancy is eliminated while preserving essential context".into());
        }
    }

    if profile.has_phases {
        criteria.push("All execution phases have clear deliverables".into());
    }
    if profile.has_validation {
        criteria.push("All validation checkpoints pass".into());
    }

    criteria
}

/// Build constraints meta from detected signals in profile
fn build_constraints_meta(profile: &IntentProfile) -> Option<ConstraintsMeta> {
    let meta = ConstraintsMeta {
        geography: profile.detected_geography.clone(),
        risk_tolerance: profile.detected_risk.clone(),
        business_type: profile.detected_biz_type.clone(),
        revenue_expectations: None, // future: extract from raw input
        team_composition: profile.detected_team.clone(),
    };

    // Only include if at least one field is populated
    if meta.geography.is_some() || meta.risk_tolerance.is_some() || meta.business_type.is_some() || meta.team_composition.is_some() {
        Some(meta)
    } else {
        None
    }
}

/// Derive appropriate tone from domain and knowledge level
fn derive_tone(profile: &IntentProfile) -> String {
    match (profile.domain.as_str(), &profile.user_knowledge) {
        ("business" | "business-strategy", _) => "strategic-actionable-direct".to_string(),
        ("creative" | "creative-writing", _) => "creative-expressive-engaging".to_string(),
        ("education", super::intent::KnowledgeLevel::Novice) => "clear-supportive-step-by-step".to_string(),
        ("legal", _) => "precise-formal-referenced".to_string(),
        ("marketing", _) => "persuasive-data-driven-concise".to_string(),
        (_, super::intent::KnowledgeLevel::Expert) => "technical-precise-concise".to_string(),
        (_, super::intent::KnowledgeLevel::Novice) => "clear-approachable-detailed".to_string(),
        _ => "professional-thorough-structured".to_string(),
    }
}

/// Vagueness Resolution: Unpacks generic requests into structured roadmaps
fn unpack_context(profile: &IntentProfile, raw: &str) -> Option<String> {
    let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "the core topic".into());
    let lower = raw.to_lowercase();
    let is_financial = profile.domain == "finance" || lower.contains("earn") || lower.contains("wealth") || lower.contains("million") || lower.contains("billion") || lower.contains("rich") || lower.contains("financial freedom") || lower.contains("money") || lower.contains("passive income") || lower.contains("retire early") || lower.contains("make $");
    let is_business = profile.domain == "business" || lower.contains("business") || lower.contains("startup") || lower.contains("company") || lower.contains("enterprise");

    if is_financial && !profile.domain.starts_with("workplace") {
        return Some(format!(
            "Target Definition (Disambiguate: Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth; quantify target amount, currency, and horizon). Current-State Baseline (Audit income, burn rate, assets, liabilities, and human capital monetization). Wealth Gap & Quantitative Modeling (Calculate Net Worth Gap, Required Annual Savings, Required Gross/Net Income, and Required CAGR). Income Engine & Leverage (Rank monetization vehicles: career, high-ticket services, equity, tech leverage by ROI). Capital Allocation & Risk Framework (Liquidity buffers, productive asset compounding, and downside mitigation across Conservative, Base, and Aggressive scenarios). Reality-Check (Quantify feasibility against constraints and identify required variable adjustments for {}).",
            subject
        ));
    }
    if is_business {
        return Some(format!(
            "Define target enterprise valuation and revenue milestones for {}. Validate unit economics (LTV/CAC, gross margin), business model canvas, go-to-market scalability, and capitalization roadmap across multi-year horizon.",
            subject
        ));
    }
    match profile.domain.as_str() {
        "computers" => Some(format!("Cover computing architecture for {}, low-level hardware-software interaction, memory constraints, and runtime efficiency.", subject)),
        "science" | "scientific-research" => Some(format!("Cover empirical foundations of {}, theoretical framework, experimental methodology, and potential impact.", subject)),
        "health" | "medical" => Some(format!("Cover physiological mechanisms of {}, clinical presentation, evidence-based interventions, and long-term outcomes.", subject)),
        "finance" => Some(format!("Cover capital dynamics of {}, market mechanisms, risk-adjusted returns, and actionable execution strategies.", subject)),
        "ai-ml" => Some(format!("Cover definition of {}, how it differs from current AI, how it might work, key challenges, risks, and real-world implications.", subject)),
        "workplace-productivity" => Some(format!("Analyze objective output vs subjective perception for {}, remote/hybrid dynamics, and cultural impact.", subject)),
        "education" => Some(format!("Cover pedagogical foundations of {}, cognitive load optimization, retention strategies, and application milestones.", subject)),
        "software" | "software-engineering" | "devops" | "devops-infra" => Some(format!("Cover system architecture for {}, deployment strategy, scalability bottlenecks, and security considerations.", subject)),
        _ => Some(format!("Break down {} into fundamental components, current state, key challenges, and future implications.", subject)),
    }
}

/// Infer missing assumptions to anchor the LLM
fn infer_assumptions(profile: &IntentProfile, raw: &str) -> Vec<String> {
    let mut assumptions = Vec::new();
    let lower = raw.to_lowercase();
    let is_financial = profile.domain == "finance" || lower.contains("earn") || lower.contains("wealth") || lower.contains("million") || lower.contains("billion") || lower.contains("rich") || lower.contains("financial freedom") || lower.contains("money") || lower.contains("passive income") || lower.contains("retire early") || lower.contains("make $");
    let is_business = profile.domain == "business" || lower.contains("business") || lower.contains("startup") || lower.contains("company") || lower.contains("enterprise");

    if is_financial && !profile.domain.starts_with("workplace") {
        assumptions.push("Assume Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth".into());
        assumptions.push("Assume unstated financial baselines (income, expenses, assets, liabilities) must be explicitly framed with transparent assumptions rather than arbitrary invention".into());
        assumptions.push("Assume mathematical consistency: calculate required CAGR and savings rate; flag any feasibility mismatch rather than fabricating unrealistic returns".into());
        assumptions.push("Assume primary income engine and skill/business leverage must precede passive portfolio compounding for zero-to-wealth trajectories".into());
        assumptions.push("Assume tax efficiency (capital gains vs ordinary income) and inflation-adjusted real returns must be accounted for across all scenario projections".into());
    } else if is_business {
        assumptions.push("Assume business enterprise valuation is separate from personal liquid net worth and depends on revenue multiples and profit margins".into());
        assumptions.push("Assume focus on sustainable unit economics (LTV/CAC > 3), gross margin health, and capital efficiency".into());
        assumptions.push("Assume scalable operational infrastructure, defensible moat, and equity retention".into());
    } else {
        // Domain assumptions
        match profile.domain.as_str() {
            "computers" => {
                assumptions.push("Assume standard system architecture and memory hierarchy unless specified".into());
                assumptions.push("Assume high efficiency, reliability, and correctness requirements".into());
            },
            "science" | "scientific-research" => {
                assumptions.push("Assume adherence to the scientific method and empirical reproducibility".into());
                assumptions.push("Assume peer-reviewed standards for statistical validity".into());
            },
            "health" | "medical" | "pharma" => {
                assumptions.push("Assume evidence-based clinical practices and safety-first protocols".into());
                assumptions.push("Assume compliance with relevant healthcare regulations and ethics".into());
            },
            "software" | "software-engineering" | "devops" | "devops-infra" => {
                if !lower.contains("legacy") && !lower.contains("old") {
                    assumptions.push("Assume modern, idiomatic technology stack and best practices".into());
                }
                assumptions.push("Assume production-grade requirements (security, logging, error handling)".into());
            },
            "data-science" | "ai-ml" => {
                assumptions.push("Assume data is imperfect and requires preprocessing/cleaning".into());
                assumptions.push("Assume model scalability and ethical considerations are paramount".into());
            },
            _ => {}
        }
    }
    
    // Intent-based assumptions
    match profile.primary_intent {
        super::intent::IntentClass::Build => {
            assumptions.push("Assume output should be immediately actionable and structured".into());
        },
        super::intent::IntentClass::Explain => {
            assumptions.push("Assume the audience needs foundational concepts clarified before deep dives".into());
        },
        super::intent::IntentClass::Debug => {
            assumptions.push("Assume underlying systems are standard unless otherwise specified".into());
        },
        _ => {}
    }
    
    assumptions
}

/// Derive dynamic final instruction based on intent and goal engineering
fn derive_dynamic_instruction(profile: &IntentProfile, raw: &str) -> String {
    let lower = raw.to_lowercase();
    let is_financial = profile.domain == "finance" || lower.contains("earn") || lower.contains("wealth") || lower.contains("million") || lower.contains("billion") || lower.contains("rich") || lower.contains("financial freedom") || lower.contains("money") || lower.contains("passive income") || lower.contains("retire early") || lower.contains("make $");
    let is_business = profile.domain == "business" || lower.contains("business") || lower.contains("startup") || lower.contains("company") || lower.contains("enterprise");

    if is_financial && !profile.domain.starts_with("workplace") {
        return "Execute a comprehensive Goal-Decomposition and Wealth-Engineering plan for the user's objective:\n1. TARGET DEFINITION: Disambiguate the goal into exact Target Amount ($), Currency, Target Type (Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth), and Horizon.\n2. CURRENT-STATE BASELINE: Audit financial baseline (income, expenses, cash, investments, debt) and human capital (skills, network, market value).\n3. WEALTH GAP CALCULATION: Calculate Net Worth Gap, Required Annual Savings, Required Gross/Net Income, and Required CAGR with quantitative modeling.\n4. INCOME ENGINE & LEVERAGE: Rank scalable monetization vehicles (career advancement, high-ticket services, business ownership, AI leverage) by time-to-revenue and ROI.\n5. CAPITAL ALLOCATION & RISK: Define liquidity buffers, diversified compounding, and comprehensive risk mitigation (income, market, debt, tax).\n6. SCENARIO MODELING: Provide Conservative, Base Case, and Aggressive projections.\n7. EXECUTION ROADMAP & KPIS: Detail milestones across 30-day, 90-day, 6-12 month, 3-year, and 5-10 year horizons with quantitative KPIs.\n8. REALITY CHECK: If the target is mathematically inconsistent with current constraints, explicitly quantify the gap, explain which variables must change, and state the feasibility verdict.".into();
    }

    if is_business {
        return "Execute an Enterprise & Business Growth Engineering plan:\n1. TARGET & VALUATION: Define target revenue, EBITDA, and enterprise valuation multiple over the target horizon.\n2. BUSINESS MODEL & UNIT ECONOMICS: Specify product/service offering, pricing model, gross margins, and customer acquisition economics (LTV/CAC).\n3. GO-TO-MARKET & SCALABILITY: Detail distribution leverage, sales channels, and technology/AI operational moats.\n4. CAPITAL & RESOURCE ALLOCATION: Outline capital requirements, funding strategy (bootstrapped vs equity), and reinvestment roadmap.\n5. SCENARIOS & RISKS: Model Conservative, Base, and Aggressive growth cases with competition and execution risk mitigation.\n6. EXECUTION TIMELINE & KPIS: Structure roadmap across PMF, initial scale, team expansion, and valuation milestones with measurable business KPIs.".into();
    }

    match profile.primary_intent {
        super::intent::IntentClass::Explain => "Provide a well-structured explanation that balances simplicity with depth.".into(),
        super::intent::IntentClass::Build => {
            if profile.domain == "software-engineering" || profile.domain == "devops-infra" {
                "Implement the requested solution with modular, high-quality code.".into()
            } else {
                "Implement the requested solution with a comprehensive and actionable plan.".into()
            }
        },
        super::intent::IntentClass::Debug => "Identify and resolve the issue while explaining the underlying cause.".into(),
        super::intent::IntentClass::Analyze => "Provide deep insights and actionable recommendations based on the analysis.".into(),
        super::intent::IntentClass::Transform => "Efficiently transform the input while maintaining data integrity and accuracy.".into(),
    }
}

/// Derive a high-resolution persona anchor for the role
fn derive_persona_anchor(domain: &str, role: &str, config: Option<&super::config::UnifiedConfig>) -> String {
    if let Some(cfg) = config {
        if let Some(tax) = cfg.domain_taxonomy.iter().find(|t| t.domain == domain) {
            if let Some(ref template) = tax.persona_template {
                return template.replace("{role}", role);
            }
        }
    }

    match domain {
        "business-strategy" => format!("Senior {} with specialization in market scaling, resource optimization, and strategic growth.", role),
        "real-estate" => format!("Experienced {} specializing in licensing compliance, property marketing, client acquisition, and agency operations.", role),
        "software-engineering" => format!("Expert {} focused on scalable architecture, clean code principles, and performance optimization.", role),
        "ai-ml" => format!("Senior {} specializing in large-scale model alignment, safety protocols, and LLM architecture.", role),
        "medical" => format!("Specialized {} with clinical expertise, diagnostic precision, and evidence-based practice.", role),
        "finance" => format!("Senior {} focused on quantitative analysis, risk management, and financial modeling.", role),
        "cybersecurity" => format!("Security {} specializing in threat detection, vulnerability research, and incident response.", role),
        "legal" => format!("Senior {} with expertise in regulatory compliance, contract law, and strategic advisory.", role),
        "workplace-productivity" => format!("Senior {} specializing in organizational dynamics, distributed team performance, and workplace culture.", role),
        _ => format!("Professional {} with deep expertise in the {} domain and related methodologies.", role, domain),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_structurer_pre_validate_valid() {
        let structurer = HttpStructurer {
            route: "/prompt".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: "How do I optimize SQL queries?".to_string(),
        };
        let input = structurer.collect().unwrap();
        assert!(structurer.pre_validate(&input).is_ok());
    }

    #[test]
    fn test_http_structurer_pre_validate_case_variations() {
        let structurer = HttpStructurer {
            route: "/prompt".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: "".to_string(),
        };

        let xss_inputs = [
            "<script>alert(1)</script>",
            "<SCRIPT src='malicious.js'></SCRIPT>",
            "<ScRiPt>console.log(1)</sCrIpT>",
            "<img src=x onerror=alert(1)>",
            "<body ONLOAD=evil()>",
            "javascript:void(0)",
            "JaVaScRiPt:alert(1)",
        ];

        for text in xss_inputs {
            let input = RawInput {
                text: text.to_string(),
                source: PromptSource::Http {
                    remote_addr: "127.0.0.1".to_string(),
                    route: "/prompt".to_string(),
                },
                trust_level: TrustLevel::Medium,
                metadata: InputMetadata::new(),
            };
            assert!(structurer.pre_validate(&input).is_err(), "Expected rejection for: {}", text);
        }

        let sqli_inputs = [
            "DROP TABLE users;",
            "drop table customers",
            "dRoP tAbLe logs",
            "DELETE FROM accounts",
            "insert into users values (1)",
            "UNION SELECT * FROM credentials",
            "admin' OR 1=1 --",
            "test'; --",
        ];

        for text in sqli_inputs {
            let input = RawInput {
                text: text.to_string(),
                source: PromptSource::Http {
                    remote_addr: "127.0.0.1".to_string(),
                    route: "/prompt".to_string(),
                },
                trust_level: TrustLevel::Medium,
                metadata: InputMetadata::new(),
            };
            assert!(structurer.pre_validate(&input).is_err(), "Expected rejection for: {}", text);
        }
    }

    #[test]
    fn test_http_structurer_pre_validate_url_encoded() {
        let structurer = HttpStructurer {
            route: "/prompt".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: "".to_string(),
        };

        let encoded_attacks = [
            "%3Cscript%3Ealert(1)%3C/script%3E",
            "%3CSCRIPT%3Ealert(1)%3C/SCRIPT%3E",
            "DROP%20TABLE%20users",
            "drop%20table%20users",
            "admin%27%20OR%201%3D1%20--",
            "union%20select%201,2,3",
            "%6a%61%76%61%73%63%72%69%70%74%3aalert(1)", // javascript:
        ];

        for text in encoded_attacks {
            let input = RawInput {
                text: text.to_string(),
                source: PromptSource::Http {
                    remote_addr: "127.0.0.1".to_string(),
                    route: "/prompt".to_string(),
                },
                trust_level: TrustLevel::Medium,
                metadata: InputMetadata::new(),
            };
            assert!(structurer.pre_validate(&input).is_err(), "Expected rejection for URL encoded: {}", text);
        }
    }
}
