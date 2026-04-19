// Removed unused serde imports
use chrono::{DateTime, Utc};
use anyhow::Result;

use crate::npae::schema::types::{
    ClarifyingQuestion, HallucinationGuardConfig, LengthBound, PromptConstraints, PromptContext,
    PromptRole, StructuredPrompt,
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

// --- Legacy Compatibility Layer ---

pub fn render_crisp_prompt(prompt: &StructuredPrompt, questions: &[ClarifyingQuestion]) -> String {
    let mut out = String::new();

    out.push_str(&format!("# ROLE: {}\n", prompt.role.primary.to_uppercase()));
    out.push_str(&format!(
        "Expertise: {}\n\n",
        prompt.role.expertise_domains.join(", ")
    ));

    out.push_str(&format!("# CONTEXT: {}\n", prompt.context.description));
    out.push_str(&format!(
        "User Level: {} | Domain: {} | Intent: {}\n",
        prompt.context.user_knowledge_level,
        prompt.context.domain,
        prompt.context.temporal_scope
    ));
    out.push_str(&format!("Background: {}\n\n", prompt.context.background));

    out.push_str("# CONSTRAINTS:\n");
    for inc in &prompt.constraints.required_inclusions {
        out.push_str(&format!("- [STRICT] {}\n", inc));
    }
    for exc in &prompt.constraints.forbidden_topics {
        out.push_str(&format!("- [AVOID]  {}\n", exc));
    }
    out.push_str(&format!("Output Format: {} | Tone: {}\n\n", 
        prompt.constraints.output_format,
        prompt.constraints.tone));

    if !questions.is_empty() {
        out.push_str("# CLARIFYING QUESTIONS:\n");
        for q in questions {
            out.push_str(&format!("- [{}] {}\n", q.priority.to_string().to_uppercase(), q.question));
        }
        out.push_str("\n");
    }

    out.push_str("# FINAL INSTRUCTION:\n");
    out.push_str("Execute the core task defined in CONTEXT with maximum precision. Align all outputs with the stipulated ROLE and strictly respect all CONSTRAINTS. Prioritize information requested in CLARIFYING QUESTIONS if provided.\n");

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

pub fn build(profile: &IntentProfile, raw: &str) -> std::result::Result<StructuredPrompt, String> {
    // Bridge to new logic by loading config
    let config = super::config::ConfigLoader::load("config/unified.json")
        .map_err(|e| format!("Config load error: {}", e))?;

    let role_primary = super::role::generate_role(profile, raw, &config.domain_taxonomy, &config.roles);
    let (inclusions, forbidden) = super::constraints::extract_constraints(raw, &config.domain_taxonomy, &config.constraints);

    let user_level_str = match profile.user_knowledge {
        super::intent::KnowledgeLevel::Novice => "novice",
        super::intent::KnowledgeLevel::Intermediate => "intermediate",
        super::intent::KnowledgeLevel::Expert => "expert",
    }.to_string();

    let intent_type = match profile.primary_intent {
        super::intent::IntentClass::Build => "development/construction",
        super::intent::IntentClass::Explain => "educational/instructional",
        super::intent::IntentClass::Debug => "troubleshooting/optimization",
        super::intent::IntentClass::Analyze => "analytical/discovery",
        super::intent::IntentClass::Transform => "refactoring/conversion",
    };

    // Extract a meaningful snippet for context
    let raw_clean = raw.lines().next().unwrap_or("").trim();
    let snippet = if raw_clean.len() > 120 {
        format!("{}...", &raw_clean[..117])
    } else {
        raw_clean.to_string()
    };

    Ok(StructuredPrompt {
        role: PromptRole {
            primary: role_primary,
            expertise_domains: vec![profile.domain.clone()],
            persona_constraints: vec![
                "no_external_api_calls".into(),
                "production_grade_only".into(),
            ],
        },
        context: PromptContext {
            domain: profile.domain.clone(),
            description: format!("{} [Focus: {}]", snippet, intent_type),
            background: format!("Original intent detected from input soup: '{}'. Target audience: {}.", 
                snippet, user_level_str),
            user_knowledge_level: user_level_str,
            temporal_scope: profile.temporal_scope.clone(),
            intent_vector: vec![profile.confidence],
        },
        constraints: PromptConstraints {
            output_format: profile.output_preference.clone(),
            length_bound: LengthBound {
                min: 200,
                max: 2000,
            },
            forbidden_topics: forbidden,
            required_inclusions: inclusions,
            tone: "technical-precise-concise".into(),
        },
        hallucination_guard: HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into(), "cite_uncertainty".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
        },
    })
}
