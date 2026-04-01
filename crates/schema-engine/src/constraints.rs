use crate::types::{Domain, PromptSchema};

#[derive(Debug, Clone, Default)]
pub struct ConstraintSet {
    pub blocked_tokens: Vec<String>,
    pub required_domains: Vec<Domain>,
    pub max_tokens: Option<usize>,
    pub min_weight_hint: f32,
}

impl ConstraintSet {
    pub fn default_rust() -> Self {
        Self {
            blocked_tokens: vec!["unsafe".into(), ".unwrap()".into()],
            required_domains: vec![],
            max_tokens: Some(2048),
            min_weight_hint: 0.1,
        }
    }

    pub fn validate(&self, schema: &PromptSchema, tokens: &[String]) -> ConstraintResult {
        if let Some(max) = self.max_tokens {
            if schema.token_count > max {
                return ConstraintResult::Violated(format!(
                    "token count {} exceeds max {}",
                    schema.token_count, max
                ));
            }
        }
        for token in tokens {
            if self
                .blocked_tokens
                .iter()
                .any(|b| token.contains(b.as_str()))
            {
                return ConstraintResult::Violated(format!("blocked token: {token}"));
            }
        }
        if !self.required_domains.is_empty() {
            let matched = self
                .required_domains
                .iter()
                .any(|d| schema.domains.contains(d));
            if !matched {
                return ConstraintResult::Violated("no required domain matched".into());
            }
        }
        ConstraintResult::Passed
    }
}

#[derive(Debug)]
pub enum ConstraintResult {
    Passed,
    Violated(String),
}
