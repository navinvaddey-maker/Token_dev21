use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::{AmbiguityAnalysis, ClarifyingQuestion, Priority};

pub fn generate(
    repr: &CompressedRepr,
    amb: &AmbiguityAnalysis,
    max_q: u8,
) -> Result<Vec<ClarifyingQuestion>, String> {
    let mut candidates: Vec<ClarifyingQuestion> = amb.gap_zones
        .iter()
        .enumerate()
        .map(|(i, gap)| score_gap(gap, repr, i as u8 + 1))
        .collect();

    candidates.sort_by(|a, b| b.information_gain.partial_cmp(&a.information_gain).unwrap());

    Ok(candidates.into_iter().take(max_q as usize).collect())
}

fn score_gap(gap: &str, _repr: &CompressedRepr, id: u8) -> ClarifyingQuestion {
    let (question, information_gain) = match gap {
        "output_format_ambiguous" => ("Should this endpoint stream tokens or return full JSON?".into(), 0.88),
        "error_handling_unspecified" => ("What happens when all 3 hallucination layers disagree?".into(), 0.73),
        "domain_context_missing" => ("What domain should this expert role operate in?".into(), 0.65),
        "constraint_incomplete" => ("Are there additional dietary or output constraints to apply?".into(), 0.55),
        _ => (format!("Could you clarify the gap concerning {}?", gap), 0.40),
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
