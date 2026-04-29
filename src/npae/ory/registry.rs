use crate::npae::ory::types::{FlowAudit, LearnedIntent, AuditRecommendation};
use crate::npae::aggressive::config::UnifiedConfig;
use anyhow::Result;

pub struct FlowRegistry;

impl FlowRegistry {
    pub fn audit(intent: &LearnedIntent, config: &UnifiedConfig) -> Result<FlowAudit> {
        let mut gaps = Vec::new();
        let mut existing_domain_match = None;
        
        let lower_inferred = intent.inferred_domain.to_lowercase();
        
        // Audit against domain taxonomy in unified.json
        for domain_tax in &config.domain_taxonomy {
            let domain_name = domain_tax.domain.to_lowercase();
            if lower_inferred.contains(&domain_name) || domain_name.contains(&lower_inferred) {
                existing_domain_match = Some(domain_tax.domain.clone());
                break;
            }
        }
        
        let recommendation = if let Some(ref domain) = existing_domain_match {
            if !intent.novel_signals.is_empty() {
                gaps.push(format!("Novel signals detected in supported domain {}: {:?}", domain, intent.novel_signals));
                AuditRecommendation::AugmentExistingFlow
            } else {
                AuditRecommendation::UseExistingFlow
            }
        } else {
            gaps.push(format!("No supported flow found for domain: {}", intent.inferred_domain));
            AuditRecommendation::BuildDynamicFlow
        };
        
        let coverage_score = if existing_domain_match.is_some() { 0.85 } else { 0.1 };
        
        Ok(FlowAudit {
            existing_domain_match,
            existing_template_match: None, // Logic for checking structurer templates could be added here
            coverage_score,
            gaps_identified: gaps,
            recommendation,
        })
    }
}
