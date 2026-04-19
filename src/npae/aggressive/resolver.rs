use std::sync::Arc;
use anyhow::Result;

use super::config::ConfigHandle;
use super::parser::parse_prompt;
use super::builder::build_candidate_rules;
use super::verifier::verify_candidates;
use super::role::generate_role;
use super::constraints::extract_constraints;
use super::metrics;
use super::structurer::{Structurer, TrustLevel};
use super::intent::IntentProfile;

pub struct ResolvedPrompt {
    pub role: String,
    pub inclusions: Vec<String>,
    pub forbidden: Vec<String>,
    pub confidence: f32,
    pub status: String,
}

pub struct PromptResolver {
    config_handle: Arc<ConfigHandle>,
    threshold: f32,
}

impl PromptResolver {
    pub fn new(config_handle: Arc<ConfigHandle>) -> Self {
        Self {
            config_handle,
            threshold: 0.95,
        }
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    pub fn resolve(
        &self,
        text: &str,
        profile: &IntentProfile,
        _source_name: &str
    ) -> ResolvedPrompt {
        let config = self.config_handle.read();
        
        // Phase 1: Parse
        let parsed = parse_prompt(text);
        
        // Phase 2: Build Candidates
        let candidates = build_candidate_rules(&parsed, &config);

        // Phase 3: Verify
        let verification = verify_candidates(&candidates, &config, self.threshold);
        
        let status = if verification.passed {
            "resolved"
        } else if candidates.is_empty() {
            "empty"
        } else {
            "low_confidence"
        };

        // Metrics (simplified for no-dependency version)
        match status {
            "resolved" => metrics::RESOLVE_TOTAL_RESOLVED.inc(),
            "low_confidence" => metrics::RESOLVE_TOTAL_LOW_CONFIDENCE.inc(),
            _ => metrics::RESOLVE_TOTAL_EMPTY.inc(),
        }

        // Generate Role and Constraints
        let role = generate_role(profile, text, &config.domain_taxonomy, &config.roles);
        let (inclusions, forbidden) = extract_constraints(text, &config.domain_taxonomy, &config.constraints);

        metrics::CONSTRAINTS_INCLUSION_TOTAL.inc_by(inclusions.len() as u64);
        metrics::CONSTRAINTS_FORBIDDEN_TOTAL.inc_by(forbidden.len() as u64);

        ResolvedPrompt {
            role,
            inclusions,
            forbidden,
            confidence: verification.confidence,
            status: status.to_string(),
        }
    }
}

pub struct StructurerRouter {
    resolver: PromptResolver,
}

impl StructurerRouter {
    pub fn new(resolver: PromptResolver) -> Self {
        Self { resolver }
    }

    pub fn dispatch(
        &mut self,
        structurer: &dyn Structurer,
        profile: &IntentProfile,
    ) -> Result<ResolvedPrompt> {
        let input = structurer.collect()?;
        structurer.pre_validate(&input)?;

        let threshold = match input.trust_level {
            TrustLevel::High      => 0.90,
            TrustLevel::Medium    => 0.95,
            TrustLevel::Low       => 0.95,
            TrustLevel::Untrusted => 0.98,
        };

        self.resolver.set_threshold(threshold);
        let result = self.resolver.resolve(&input.text, profile, structurer.source_name());

        Ok(result)
    }
}
