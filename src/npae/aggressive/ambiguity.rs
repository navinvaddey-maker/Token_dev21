use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::AmbiguityAnalysis;

pub fn score(repr: &CompressedRepr, _raw: &str) -> Result<AmbiguityAnalysis, String> {
    let mut gap_zones = Vec::new();
    let mut score = 0.0;

    if repr.compression_ratio < 0.8 {
        score += 0.4;
        gap_zones.push("output_format_ambiguous".into());
    }
    if repr.token_ids.len() < 50 {
        score += 0.35;
        gap_zones.push("error_handling_unspecified".into());
    }
    if repr.attention_weights.len() > 10 {
        score += 0.1;
        gap_zones.push("domain_context_missing".into());
    }

    Ok(AmbiguityAnalysis {
        score,
        threshold: 0.65, // Will be overridden by engine config
        triggered: score > 0.65,
        gap_zones,
    })
}
