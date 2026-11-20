//! Prompt Input Ingestion, Trust Levels & XSS/SQLi Pre-Validation.
//!
//! // ARCH: ARCHITECTURE.md §6.6 — isolated ingestion & compiled `RegexSet` pre-validation.

use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::{RegexSet, RegexSetBuilder};

/// Compiled case-insensitive XSS / `SQLi` pattern set for `HttpStructurer::pre_validate`.
///
/// // ARCH: ARCHITECTURE.md §6.6 — single-pass compiled `RegexSet` avoids allocating
/// // `decoded.to_lowercase()` 11 times per request.
static DANGEROUS_CONTENT_PATTERNS: Lazy<Option<RegexSet>> = Lazy::new(|| {
    RegexSetBuilder::new([
        regex::escape("<script"),
        regex::escape("javascript:"),
        regex::escape("onerror="),
        regex::escape("onload="),
        regex::escape("drop table"),
        regex::escape("delete from"),
        regex::escape("insert into"),
        regex::escape("union select"),
        regex::escape("'; --"),
        regex::escape("1=1"),
        regex::escape("or 1=1"),
    ])
    .case_insensitive(true)
    .build()
    .ok()
});

/// Source channel from which a raw prompt entered the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    Cli,
    Http {
        remote_addr: String,
        route: String,
    },
    File {
        path: String,
        line_number: usize,
    },
    Sdk {
        api_key_prefix: String,
        model: String,
    },
    WebSocket {
        session_id: String,
    },
    Queue {
        topic: String,
        message_id: String,
    },
    Internal {
        caller: String,
    },
    Unknown,
}

/// Trust level assigned to a prompt source channel.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum TrustLevel {
    High,
    Medium,
    Low,
    Untrusted,
}

impl PromptSource {
    /// Returns the default `TrustLevel` for this prompt source.
    ///
    /// # Panics
    /// Never.
    pub fn trust_level(&self) -> TrustLevel {
        match self {
            PromptSource::Internal { .. } => TrustLevel::High,
            PromptSource::Sdk { .. } => TrustLevel::High,
            PromptSource::Http { .. } => TrustLevel::Medium,
            PromptSource::WebSocket { .. } => TrustLevel::Medium,
            PromptSource::Cli => TrustLevel::Low,
            PromptSource::File { .. } => TrustLevel::Low,
            PromptSource::Queue { .. } => TrustLevel::Untrusted,
            PromptSource::Unknown => TrustLevel::Untrusted,
        }
    }
}

/// Raw user input bundled with its source, trust level, and request metadata.
#[derive(Debug, Clone)]
pub struct RawInput {
    pub text: String,
    pub source: PromptSource,
    pub trust_level: TrustLevel,
    pub metadata: InputMetadata,
}

/// Request metadata attached to a `RawInput`.
#[derive(Debug, Clone)]
pub struct InputMetadata {
    pub request_id: String,
    pub received_at: DateTime<Utc>,
    pub language_hint: Option<String>,
    pub max_tokens_hint: Option<usize>,
}

impl InputMetadata {
    /// Creates a new `InputMetadata` with a fresh UUID and current UTC timestamp.
    ///
    /// # Panics
    /// Never.
    pub fn new() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            received_at: Utc::now(),
            language_hint: None,
            max_tokens_hint: None,
        }
    }
}

impl Default for InputMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait implemented by input collectors that validate and wrap raw user prompts.
pub trait Structurer: Send + Sync {
    /// Collects raw input from the underlying source.
    ///
    /// # Errors
    /// Returns `Err` if reading or initial bounds checking fails.
    fn collect(&self) -> Result<RawInput>;
    /// Returns the static identifier of the input source.
    fn source_name(&self) -> &'static str;
    /// Validates the collected `RawInput` for security or structural issues.
    ///
    /// # Errors
    /// Returns `Err` if dangerous patterns (e.g. XSS or SQLi) are detected.
    fn pre_validate(&self, _input: &RawInput) -> Result<()> {
        Ok(())
    }
}

/// CLI stdin input collector.
#[derive(Debug)]
pub struct CliStructurer {
    pub max_length: usize,
}

impl Structurer for CliStructurer {
    fn collect(&self) -> Result<RawInput> {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let text = line.trim().to_string();

        if text.is_empty() {
            return Err(anyhow::anyhow!("Empty prompt"));
        }
        if text.len() > self.max_length {
            return Err(anyhow::anyhow!(
                "Prompt too long: {} > {}",
                text.len(),
                self.max_length
            ));
        }

        Ok(RawInput {
            text,
            source: PromptSource::Cli,
            trust_level: TrustLevel::Low,
            metadata: InputMetadata::new(),
        })
    }

    fn source_name(&self) -> &'static str {
        "cli"
    }
}

/// HTTP request body input collector with XSS/SQLi pre-validation.
#[derive(Debug)]
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

    fn source_name(&self) -> &'static str {
        "http"
    }

    fn pre_validate(&self, input: &RawInput) -> Result<()> {
        if let Some(ref set) = *DANGEROUS_CONTENT_PATTERNS {
            if set.is_match(&input.text) {
                return Err(anyhow::anyhow!("Potentially dangerous content detected"));
            }
            if let Ok(decoded) = urlencoding::decode(&input.text) {
                if set.is_match(&decoded) {
                    return Err(anyhow::anyhow!("Encoded dangerous content detected"));
                }
            }
            return Ok(());
        }

        // Fallback if RegexSet compilation were ever unavailable (should never happen for static literals).
        let dangerous_patterns = [
            "<script",
            "javascript:",
            "onerror=",
            "onload=",
            "drop table",
            "delete from",
            "insert into",
            "union select",
            "'; --",
            "1=1",
            "or 1=1",
        ];
        let lower = input.text.to_lowercase();
        if dangerous_patterns.iter().any(|p| lower.contains(p)) {
            return Err(anyhow::anyhow!("Potentially dangerous content detected"));
        }
        let decoded_lower = urlencoding::decode(&input.text)
            .unwrap_or_default()
            .to_lowercase();
        if dangerous_patterns.iter().any(|p| decoded_lower.contains(p)) {
            return Err(anyhow::anyhow!("Encoded dangerous content detected"));
        }
        Ok(())
    }
}

// ... Additional structurers (File, SDK, Queue) would be implemented similarly

// --- CRISP Prompt Renderer (v2 — token waste eliminated) ---

/// Splits a raw prompt into `(optional_context_block, optional_rag_block, clean_user_prompt)`.
///
/// # Panics
/// Never.
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
            rag = Some(
                user_prompt[start_idx + "--- Retrieved Knowledge ---\n".len()..end_idx]
                    .trim()
                    .to_string(),
            );
            user_prompt = user_prompt[end_idx + "--- End Retrieved Knowledge ---\n".len()..]
                .trim()
                .to_string();
        }
    }

    (context, rag, user_prompt)
}
