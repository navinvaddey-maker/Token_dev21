use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{AmbiguityAnalysis, ClarifyingQuestion, Priority};

pub fn generate(
    repr: &CompressedRepr,
    amb: &AmbiguityAnalysis,
    max_q: u8,
) -> Result<Vec<ClarifyingQuestion>, String> {
    generate_with_domain(repr, amb, max_q, "general")
}

/// Domain-aware question generation — produces relevant questions based on domain context
pub fn generate_with_domain(
    repr: &CompressedRepr,
    amb: &AmbiguityAnalysis,
    max_q: u8,
    domain: &str,
) -> Result<Vec<ClarifyingQuestion>, String> {
    let mut candidates: Vec<ClarifyingQuestion> = amb.gap_zones
        .iter()
        .enumerate()
        .map(|(i, gap)| score_gap(gap, repr, i as u8 + 1, domain))
        .collect();

    candidates.sort_by(|a, b| b.information_gain.partial_cmp(&a.information_gain).unwrap());

    Ok(candidates.into_iter().take(max_q as usize).collect())
}

fn score_gap(gap: &str, _repr: &CompressedRepr, id: u8, domain: &str) -> ClarifyingQuestion {
    let (question, information_gain) = match (gap, domain) {
        // --- Domain-specific: Business ---
        ("domain_context_missing", "business-strategy") => 
            ("What specific industry or market vertical is this business targeting?".into(), 0.88),
        ("intent_unclear", "business-strategy") => 
            ("Is this a new venture from scratch or scaling an existing business?".into(), 0.85),
        ("output_format_ambiguous", "business-strategy") => 
            ("Do you need a strategic roadmap, financial projections, or an operational plan?".into(), 0.82),
        ("constraint_incomplete", "business-strategy") => 
            ("What is your budget range and desired timeline to launch?".into(), 0.75),
        ("vague_language_detected", "business-strategy") => 
            ("What is your primary revenue model: subscription, one-time, marketplace, or ad-supported?".into(), 0.78),

        // --- Domain-specific: Software Engineering ---
        ("output_format_ambiguous", "software-engineering" | "devops-infra") => 
            ("Should this be a working implementation, architecture design, or code review?".into(), 0.88),
        ("domain_context_missing", "software-engineering" | "devops-infra") => 
            ("What is the target tech stack and deployment environment?".into(), 0.82),
        ("intent_unclear", "software-engineering" | "devops-infra") => 
            ("Are you building from scratch, extending existing code, or debugging?".into(), 0.80),
        ("constraint_incomplete", "software-engineering" | "devops-infra") => 
            ("What are the performance requirements (latency, throughput, scale)?".into(), 0.73),

        // --- Domain-specific: Nutrition/Fitness ---
        ("domain_context_missing", "sports-nutrition" | "health-fitness") => 
            ("What is the specific training goal (performance, weight management, recovery)?".into(), 0.85),
        ("constraint_incomplete", "sports-nutrition" | "health-fitness") => 
            ("Are there any allergies, dietary restrictions, or medical conditions to consider?".into(), 0.82),
        ("intent_unclear", "sports-nutrition" | "health-fitness") => 
            ("Is this for competition preparation, general fitness, or rehabilitation?".into(), 0.78),
        ("output_format_ambiguous", "sports-nutrition" | "health-fitness") => 
            ("Do you need a daily meal plan, weekly overview, or macro guidelines?".into(), 0.75),

        // --- Domain-specific: Marketing ---
        ("domain_context_missing", "marketing") => 
            ("What is the target audience demographic and psychographic profile?".into(), 0.86),
        ("intent_unclear", "marketing") => 
            ("Is this for brand awareness, lead generation, or conversion optimization?".into(), 0.82),
        ("constraint_incomplete", "marketing") => 
            ("What channels are in scope (social, email, paid ads, content)?".into(), 0.75),

        // --- Domain-specific: Data Science ---
        ("domain_context_missing", "data-science") => 
            ("What is the dataset size, format, and current state of data quality?".into(), 0.85),
        ("intent_unclear", "data-science") => 
            ("Is the goal prediction, classification, clustering, or exploratory analysis?".into(), 0.82),
        ("output_format_ambiguous", "data-science") => 
            ("Do you need a model, a report, or an automated pipeline?".into(), 0.78),

        // --- Domain-specific: Education ---
        ("domain_context_missing", "education") => 
            ("What is the learner level and subject area?".into(), 0.83),
        ("intent_unclear", "education") => 
            ("Are you creating a curriculum, a single lesson, or assessment materials?".into(), 0.80),

        // --- Domain-specific: Finance ---
        ("domain_context_missing", "finance") => 
            ("What is the investment horizon and risk appetite?".into(), 0.85),
        ("intent_unclear", "finance") => 
            ("Is this for personal finance, corporate finance, or investment analysis?".into(), 0.82),

        // --- Generic fallbacks (domain-agnostic) ---
        ("output_format_ambiguous", _) => 
            ("What format would be most useful: structured plan, detailed analysis, or actionable checklist?".into(), 0.70),
        ("domain_context_missing", _) => 
            ("What is the primary domain or industry context for this request?".into(), 0.65),
        ("intent_unclear", _) => 
            ("What is the primary goal: build something new, analyze existing, or fix a problem?".into(), 0.60),
        ("constraint_incomplete", _) => 
            ("Are there specific constraints on budget, timeline, or resources?".into(), 0.55),
        ("vague_language_detected", _) => 
            ("Could you provide more specific details about the expected outcome?".into(), 0.50),
        _ => 
            (format!("Could you clarify the gap concerning {}?", gap), 0.40),
    };

    let priority = if information_gain > 0.8 {
        Priority::High
    } else if information_gain > 0.6 {
        Priority::Medium
    } else {
        Priority::Low
    };

    ClarifyingQuestion {
        id,
        question,
        information_gain,
        gap_addressed: gap.to_string(),
        priority,
    }
}
