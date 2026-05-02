//! Flow Registry v2 — Deep multi-level auditing

use crate::npae::ory::types::{
    FlowAudit, LearnedIntent, AuditRecommendation,
    DomainAuditResult, DomainMatchMethod, TemplateAuditResult,
    ConstraintAuditResult, AuditGapDetail, GapType, GapSeverity,
};
use crate::npae::ory::domain_mapper;
use crate::npae::aggressive::config::UnifiedConfig;
use anyhow::Result;

pub struct FlowRegistry;

impl FlowRegistry {
    pub fn audit(intent: &LearnedIntent, config: &UnifiedConfig) -> Result<FlowAudit> {
        let mut gaps = Vec::new();
        let mut gap_details = Vec::new();

        // Level 1: Domain Audit
        let dm = domain_mapper::map_domain(&intent.inferred_domain, config);
        let domain_audit = DomainAuditResult {
            matched_domain: dm.matched_domain.clone(),
            match_method: match dm.method {
                domain_mapper::MatchMethod::ExactMatch => DomainMatchMethod::ExactMatch,
                domain_mapper::MatchMethod::SynonymMatch => DomainMatchMethod::SynonymMatch,
                domain_mapper::MatchMethod::HierarchyMatch => DomainMatchMethod::HierarchyMatch,
                domain_mapper::MatchMethod::FuzzyMatch | domain_mapper::MatchMethod::KeywordOverlap
                    => DomainMatchMethod::FuzzyMatch,
                domain_mapper::MatchMethod::NoMatch => DomainMatchMethod::NoMatch,
            },
            match_confidence: dm.confidence,
        };
        if dm.matched_domain.is_none() {
            gaps.push(format!("No flow for domain: {}", intent.inferred_domain));
            gap_details.push(AuditGapDetail {
                gap_type: GapType::DomainMissing,
                severity: GapSeverity::Critical,
                description: format!("Domain '{}' not in taxonomy", intent.inferred_domain),
                suggested_action: "Add domain or map via synonym".into(),
            });
        }

        // Level 2: Template Coverage
        let eff_domain = dm.matched_domain.clone()
            .unwrap_or_else(|| intent.inferred_domain.clone());
        let tc = domain_mapper::has_structurer_templates(&eff_domain);
        let template_audit = TemplateAuditResult {
            has_execution_phases: tc.has_execution_phases,
            has_validation_steps: tc.has_validation_steps,
            has_success_criteria: tc.has_success_criteria,
            has_role_template: tc.has_role_template,
            has_constraint_rules: tc.has_constraint_rules,
            template_coverage: tc.score(),
        };
        if !tc.has_execution_phases {
            gap_details.push(AuditGapDetail {
                gap_type: GapType::TemplateMissing, severity: GapSeverity::Major,
                description: format!("No execution phases for '{}'", eff_domain),
                suggested_action: "Blueprint should generate phases".into(),
            });
        }

        // Level 3: Constraint Coverage
        let total_c = intent.constraint_phrases.len() as u32;
        let mut covered = 0u32;
        let mut uncovered = Vec::new();
        for cp in &intent.constraint_phrases {
            let cl = cp.to_lowercase();
            let hit = config.constraints.iter().any(|r| {
                r.trigger.iter().any(|tg| tg.iter().any(|w| cl.contains(&w.word.to_lowercase())))
            });
            if hit { covered += 1; } else { uncovered.push(cp.clone()); }
        }
        let cc = if total_c > 0 { covered as f32 / total_c as f32 } else { 1.0 };
        let constraint_audit = ConstraintAuditResult {
            total_constraints_detected: total_c, constraints_covered: covered,
            uncovered_constraints: uncovered.clone(), constraint_coverage: cc,
        };
        for uc in &uncovered {
            gap_details.push(AuditGapDetail {
                gap_type: GapType::ConstraintUnsupported, severity: GapSeverity::Major,
                description: format!("Constraint '{}' unmatched", uc),
                suggested_action: "Handle in blueprint".into(),
            });
        }

        // Level 4: Novel signals
        for sig in &intent.novel_signals {
            gap_details.push(AuditGapDetail {
                gap_type: GapType::NovelSignalUnhandled, severity: GapSeverity::Major,
                description: format!("Novel signal: {}", sig),
                suggested_action: "Blueprint should address".into(),
            });
        }

        // Composite score
        let novel_pen = if intent.novel_signals.is_empty() { 1.0 }
            else { (1.0 - intent.novel_signals.len() as f32 * 0.15).max(0.1) };
        let composite = dm.confidence * 0.3 + tc.score() * 0.3 + cc * 0.2 + novel_pen * 0.2;

        let recommendation = if composite >= 0.8 && intent.novel_signals.is_empty() {
            AuditRecommendation::UseExistingFlow
        } else if composite >= 0.5 {
            if !intent.novel_signals.is_empty() {
                gaps.push(format!("Novel signals: {:?}", intent.novel_signals));
            }
            AuditRecommendation::AugmentExistingFlow
        } else {
            AuditRecommendation::BuildDynamicFlow
        };

        let tmpl_match = if tc.score() >= 0.5 { Some(format!("{}_template", eff_domain)) } else { None };

        Ok(FlowAudit {
            existing_domain_match: dm.matched_domain,
            existing_template_match: tmpl_match,
            coverage_score: composite,
            gaps_identified: gaps,
            recommendation,
            domain_audit,
            template_audit,
            constraint_audit,
            gap_details,
        })
    }
}
