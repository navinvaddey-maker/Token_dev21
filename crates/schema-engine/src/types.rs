#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PromptSchema {
    pub intent: Intent,
    pub domains: Vec<Domain>,
    pub style: StyleHint,
    pub token_count: usize,
}

/// The ONLY export that crosses into neuro crates.
#[derive(Debug, Clone)]
pub struct TokenActivation {
    pub tokens: Vec<String>,
    pub weight_hint: f32,
    pub blocked: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum Intent {
    CodeGeneration,
    Explanation,
    Debugging,
    Refactoring,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum Domain {
    Rust,
    Async,
    Sqlx,
    Tokio,
    Axum,
    Postgres,
    Serde,
    Other(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum StyleHint {
    Verbose,
    Concise,
    Documented,
    Unknown,
}
