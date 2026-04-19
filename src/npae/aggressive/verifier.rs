use std::collections::HashSet;
use super::builder::CandidateRule;
use super::config::{UnifiedConfig, ConstraintRule};

#[derive(Debug)]
pub struct MatchedRule {
    pub candidate: CandidateRule,
    pub json_rule: ConstraintRule,
    pub match_score: f32,
}

#[derive(Debug)]
pub struct VerificationResult {
    pub confidence: f32,
    pub matched_rules: Vec<MatchedRule>,
    pub unmatched_candidates: Vec<CandidateRule>,
    pub passed: bool,
}

pub fn verify_candidates(
    candidates: &[CandidateRule],
    config: &UnifiedConfig,
    threshold: f32,
) -> VerificationResult {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    for candidate in candidates {
        let best_match = config.constraints.iter()
            .filter_map(|rule| {
                let score = compute_match_score(candidate, rule);
                if score > 0.0 { Some((rule, score)) } else { None }
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        match best_match {
            Some((rule, score)) => matched.push(MatchedRule {
                candidate: candidate.clone(),
                json_rule: rule.clone(),
                match_score: score,
            }),
            None => unmatched.push(candidate.clone()),
        }
    }

    let confidence = if candidates.is_empty() {
        0.0
    } else {
        let weighted: f32 = matched.iter().map(|m| m.match_score).sum();
        weighted / candidates.len() as f32
    };

    VerificationResult {
        confidence,
        matched_rules: matched,
        unmatched_candidates: unmatched,
        passed: confidence >= threshold,
    }
}

fn compute_match_score(candidate: &CandidateRule, rule: &ConstraintRule) -> f32 {
    let candidate_set: HashSet<&str> = candidate
        .trigger_words.iter().map(|s| s.as_str()).collect();

    let best = rule.trigger.iter()
        .map(|group| {
            let total: u32 = group.iter().map(|tw| tw.weight).sum();
            if total == 0 { return 0.0; }
            let matched_weight: u32 = group.iter()
                .filter(|tw| candidate_set.contains(tw.word.as_str()))
                .map(|tw| tw.weight)
                .sum();
            matched_weight as f32 / total as f32
        })
        .fold(0.0f32, f32::max);

    let domain_bonus = if candidate.inferred_domain == rule.domain { 0.1 } else { 0.0 };
    (best + domain_bonus).min(1.0)
}
