// Removed unused serde imports
use chrono::{DateTime, Utc};
use anyhow::Result;

use crate::npae::schema::types::{
    ClarifyingQuestion, ConstraintsMeta, ExecutionPhase, HallucinationGuardConfig,
    LengthBound, PromptConstraints, PromptContext, PromptRole, StructuredPrompt,
    ValidationStep,
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
        if input.text.contains("<script") || input.text.contains("DROP TABLE") {
            return Err(anyhow::anyhow!("Forbidden content detected"));
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
    let mut out = String::new();    // ROLE — High-resolution persona
    out.push_str(&format!("# ROLE: {}\n", prompt.role.primary.to_uppercase()));
    if !prompt.role.persona_anchor.is_empty() {
        out.push_str(&format!("Profile: {}\n", prompt.role.persona_anchor));
    }
    if prompt.role.persona_constraints.iter().any(|c| c != "no_external_api_calls" && c != "production_grade_only") {
        out.push_str(&format!("Guardrails: {}\n", prompt.role.persona_constraints.join(", ")));
    }
    out.push('\n');
 
    // CONTEXT — Dense metadata, no redundancy
    out.push_str(&format!("# CONTEXT: {}\n", prompt.context.description));
    let mut meta = vec![
        format!("Expertise: {} {}", prompt.context.user_knowledge_level, prompt.context.domain)
    ];
    if prompt.context.temporal_scope != "unspecified" && prompt.context.temporal_scope != "immediate" {
        meta.push(format!("Scope: {}", prompt.context.temporal_scope));
    }
    out.push_str(&meta.join(" | "));
    out.push('\n');
 
    if !prompt.context.background.is_empty() && prompt.context.background != prompt.context.description {
        out.push_str(&format!("Details: {}\n", prompt.context.background));
    }
    if !prompt.context.assumptions.is_empty() {
        out.push_str(&format!("Assumptions: {}\n", prompt.context.assumptions.join("; ")));
    }
    if let Some(ctx) = context_opt {
        out.push_str(&format!("Semantic Context: {}\n", ctx));
    }
    out.push('\n');

    if let Some(rag) = rag_opt {
        out.push_str("# SOURCE KNOWLEDGE:\n");
        out.push_str(&rag);
        out.push_str("\n\n");
    }

    // CONSTRAINTS — only non-empty sections
    if !prompt.constraints.required_inclusions.is_empty() || !prompt.constraints.forbidden_topics.is_empty() {
        out.push_str("# CONSTRAINTS:\n");
        for inc in &prompt.constraints.required_inclusions {
            out.push_str(&format!("- [STRICT] {}\n", inc));
        }
        for exc in &prompt.constraints.forbidden_topics {
            out.push_str(&format!("- [AVOID]  {}\n", exc));
        }
        // Output format: only emit if it's not the default
        if prompt.constraints.output_format != "markdown" {
            out.push_str(&format!("Output Format: {} | Tone: {}\n", 
                prompt.constraints.output_format,
                prompt.constraints.tone));
        } else {
            out.push_str(&format!("Tone: {}\n", prompt.constraints.tone));
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

pub fn build(profile: &IntentProfile, raw: &str, resolved: &super::resolver::ResolvedPrompt) -> std::result::Result<StructuredPrompt, String> {
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

    // Build a non-redundant background — only adds new info beyond the description
    let background = build_background(profile, &user_level_str);

    // Infer execution phases based on temporal scope and intent
    let execution_phases = infer_execution_phases(profile, raw);

    // Infer validation steps if validation signals detected
    let validation_steps = infer_validation_steps(profile, raw);

    // Infer success criteria (Intent-driven)
    let success_criteria = infer_success_criteria(profile, raw);

    // Infer implicit assumptions based on context/domain
    let assumptions = infer_assumptions(profile, raw);

    // Build dynamic instruction
    let dynamic_instruction = derive_dynamic_instruction(profile);

    // Build constraints meta from detected signals
    let constraints_meta = build_constraints_meta(profile);

    // Derive appropriate tone from domain + knowledge level
    let tone = derive_tone(profile);

    // Derive high-resolution persona anchor
    let persona_anchor = derive_persona_anchor(&profile.domain, &role_primary);

    Ok(StructuredPrompt {
        role: PromptRole {
            primary: role_primary,
            expertise_domains: vec![profile.domain.clone()],
            persona_constraints: vec![
                "no_external_api_calls".into(),
                "production_grade_only".into(),
            ],
            persona_anchor,
        },
        context: PromptContext {
            domain: profile.domain.clone(),
            description,
            background,
            user_knowledge_level: user_level_str,
            temporal_scope: profile.temporal_scope.clone(),
            intent_vector: vec![profile.confidence],
            assumptions,
        },
        constraints: PromptConstraints {
            output_format: profile.output_preference.clone(),
            length_bound: LengthBound {
                min: 200,
                max: 2000,
            },
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
fn build_background(profile: &IntentProfile, user_level: &str) -> String {
    let mut parts = Vec::new();
    
    parts.push(format!("Domain: {} | Audience: {}", profile.domain, user_level));
    
    // Vagueness Resolution / Context Unpacking
    if let Some(roadmap) = unpack_context(profile) {
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

/// Infer execution phases from temporal scope and raw prompt
fn infer_execution_phases(profile: &IntentProfile, raw: &str) -> Vec<ExecutionPhase> {
    if !profile.has_phases && !profile.has_timeline {
        // No temporal signals — check if this is a build/planning task
        if profile.primary_intent != super::intent::IntentClass::Build 
            && profile.primary_intent != super::intent::IntentClass::Transform {
            return vec![];
        }
    }

    let lower = raw.to_lowercase();
    let mut phases = Vec::new();

    // If the user explicitly mentions phases, we create a generic phased structure
    // If the domain gives us clues, we can be more specific
    match profile.domain.as_str() {
        "business-strategy" => {
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
        "software-engineering" | "devops-infra" => {
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
        "sports-nutrition" | "health-fitness" => {
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

/// Infer success criteria from profile and raw prompt — intent-driven
fn infer_success_criteria(profile: &IntentProfile, _raw: &str) -> Vec<String> {
    let mut criteria = Vec::new();
    
    // Domain-Aware Success Criteria
    match (profile.primary_intent.clone(), profile.domain.as_str()) {
        // BUILD Intent
        (super::intent::IntentClass::Build, "business-strategy") | (super::intent::IntentClass::Build, "finance") => {
            criteria.push("Business model is viable, scalable, and grounded in market reality".into());
            criteria.push("Resource allocation (time/budget) is realistic and optimized".into());
            criteria.push("Strategic roadmap provides clear, actionable milestones".into());
        }
        (super::intent::IntentClass::Build, "software-engineering") | (super::intent::IntentClass::Build, "devops-infra") => {
            criteria.push("Functional, bug-free implementation with optimized performance".into());
            criteria.push("Adheres to industry-standard architectural patterns and best practices".into());
            criteria.push("Includes necessary technical documentation and test coverage".into());
        }
        (super::intent::IntentClass::Build, "ai-ml") | (super::intent::IntentClass::Build, "data-science") => {
            criteria.push("Model performance meets or exceeds stated benchmarks".into());
            criteria.push("Alignment, safety, and ethical considerations are explicitly addressed".into());
            criteria.push("Data handling and inference pipelines are robust and scalable".into());
        }
        (super::intent::IntentClass::Build, "medical") | (super::intent::IntentClass::Build, "pharma") => {
            criteria.push("Clinically accurate protocols grounded in peer-reviewed evidence".into());
            criteria.push("Strict adherence to regulatory (FDA/EMA) and ethical guidelines".into());
            criteria.push("Risk-benefit analysis is comprehensive and clearly stated".into());
        }
        (super::intent::IntentClass::Build, "sports-nutrition") | (super::intent::IntentClass::Build, "health-fitness") => {
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
        ("business-strategy", _) => "strategic-actionable-direct".to_string(),
        ("creative-writing", _) => "creative-expressive-engaging".to_string(),
        ("education", super::intent::KnowledgeLevel::Novice) => "clear-supportive-step-by-step".to_string(),
        ("legal", _) => "precise-formal-referenced".to_string(),
        ("marketing", _) => "persuasive-data-driven-concise".to_string(),
        (_, super::intent::KnowledgeLevel::Expert) => "technical-precise-concise".to_string(),
        (_, super::intent::KnowledgeLevel::Novice) => "clear-approachable-detailed".to_string(),
        _ => "professional-thorough-structured".to_string(),
    }
}

/// Vagueness Resolution: Unpacks generic requests into structured roadmaps
fn unpack_context(profile: &IntentProfile) -> Option<String> {
    let subject = profile.dynamic_subject.clone().unwrap_or_else(|| "the core topic".into());
    match profile.domain.as_str() {
        "ai-ml" => Some(format!("Cover definition of {}, how it differs from current AI, how it might work, key challenges, risks, and real-world implications.", subject)),
        "finance" => Some(format!("Cover key concepts of {}, market trends, regulatory environment, and strategic recommendations.", subject)),
        "medical" => Some(format!("Cover etiology of {}, clinical presentation, diagnostic criteria, treatment options, and prognosis.", subject)),
        "scientific-research" => Some(format!("Cover methodology for {}, data analysis, ethical considerations, and potential impact.", subject)),
        "workplace-productivity" => Some(format!("Analyze objective output vs subjective perception for {}, remote/hybrid dynamics, and cultural impact.", subject)),
        "education" => Some(format!("Cover pedagogical foundations of {}, cognitive load optimization, retention strategies, and application milestones.", subject)),
        "software-engineering" | "devops-infra" => Some(format!("Cover system architecture for {}, deployment strategy, scalability bottlenecks, and security considerations.", subject)),
        _ => Some(format!("Break down {} into fundamental components, current state, key challenges, and future implications.", subject)),
    }
}

/// Infer missing assumptions to anchor the LLM
fn infer_assumptions(profile: &IntentProfile, raw: &str) -> Vec<String> {
    let mut assumptions = Vec::new();
    let lower = raw.to_lowercase();
    
    // Domain assumptions
    match profile.domain.as_str() {
        "software-engineering" | "devops-infra" => {
            if !lower.contains("legacy") && !lower.contains("old") {
                assumptions.push("Assume modern, idiomatic technology stack and best practices".into());
            }
            assumptions.push("Assume production-grade requirements (security, logging, error handling)".into());
        },
        "business-strategy" => {
            assumptions.push("Assume resource constraints (time/budget) typical of the specified team size".into());
            assumptions.push("Assume focus on ROI and measurable business outcomes".into());
        },
        "data-science" | "ai-ml" => {
            assumptions.push("Assume data is imperfect and requires preprocessing/cleaning".into());
            assumptions.push("Assume model scalability and ethical considerations are paramount".into());
        },
        "medical" | "pharma" => {
            assumptions.push("Assume strict regulatory compliance (e.g. HIPAA, FDA guidelines) is required".into());
        }
        _ => {}
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

/// Derive dynamic final instruction based on intent
fn derive_dynamic_instruction(profile: &IntentProfile) -> String {
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
fn derive_persona_anchor(domain: &str, role: &str) -> String {
    match domain {
        "business-strategy" => format!("Senior {} with specialization in market scaling, resource optimization, and strategic growth.", role),
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
