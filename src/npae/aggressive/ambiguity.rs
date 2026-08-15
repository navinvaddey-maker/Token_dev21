use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::AmbiguityAnalysis;

pub fn score(repr: &CompressedRepr, raw: &str) -> Result<AmbiguityAnalysis, String> {
    let mut gap_zones = Vec::new();
    let mut score = 0.0;

    let lower_raw = raw.to_lowercase();
    let vague_words = ["stuff", "things", "somehow", "maybe", "whatever", "something", "anyway"];
    let vague_count = vague_words.iter().filter(|&&w| lower_raw.contains(w)).count();

    if vague_count > 0 {
        score += 0.25 + (0.05 * vague_count as f32);
        gap_zones.push("vague_language_detected".into());
    }

    let word_count = raw.split_whitespace().count();
    if word_count < 10 {
        score += 0.2;
        gap_zones.push("prompt_too_short".into());
    } else if word_count > 40 {
        score -= 0.15;
    }

    if repr.compression_ratio < 0.8 {
        score += 0.2;
        gap_zones.push("output_format_ambiguous".into());
    }

    if repr.intent_vec.is_empty() {
        score += 0.3;
        gap_zones.push("domain_context_missing".into());
    } else {
        let max_intent = repr.intent_vec.iter().fold(0.0f32, |a, &b| a.max(b));
        let min_intent = repr.intent_vec.iter().fold(1.0f32, |a, &b| a.min(b));
        if max_intent - min_intent < 0.3 {
            score += 0.3;
            gap_zones.push("intent_unclear".into());
        }
    }

    score = score.max(0.0);

    Ok(AmbiguityAnalysis {
        score,
        threshold: 0.65, // Will be overridden by engine config
        triggered: score > 0.65,
        gap_zones,
    })
}
