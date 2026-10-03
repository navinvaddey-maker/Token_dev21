use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::AmbiguityAnalysis;

use super::rag_context::DerivedRagContext;

pub fn score(
    repr: &CompressedRepr,
    raw: &str,
    reconstructed: &crate::types::ReconstructedInput,
    derived_rag: Option<&DerivedRagContext>,
) -> Result<AmbiguityAnalysis, String> {
    let mut gap_zones = Vec::new();
    let mut score = 0.0;

    // 1. Structural Ambiguity (from Reconstruction Stage -1)
    if !reconstructed.ambiguity_register.is_empty() {
        let reg_score = (reconstructed.ambiguity_register.len() as f32 * 0.15).min(0.5);
        score += reg_score;
        for flag in &reconstructed.ambiguity_register {
            let gap_str = format!("structural_ambiguity: {}", flag.reason);
            // Skip gap zone if RAG facts cover this specific gap
            if let Some(rag) = derived_rag {
                if rag.covered_gaps.contains(&gap_str) || rag.covered_gaps.contains(&flag.reason) {
                    score -= (reg_score * 0.5).min(score);
                    continue;
                }
            }
            gap_zones.push(gap_str);
        }
    }

    let lower_raw = raw.to_lowercase();
    let vague_words = ["stuff", "things", "somehow", "maybe", "whatever", "something", "anyway"];
    let vague_count = vague_words.iter().filter(|&&w| lower_raw.contains(w)).count();

    if vague_count > 0 {
        score += 0.25 + (0.05 * vague_count as f32);
        gap_zones.push("vague_language_detected".into());
    }

    let word_count = raw.split_whitespace().count();
    if word_count < 10 {
        if let Some(rag) = derived_rag {
            if !rag.facts.is_empty() {
                // RAG facts supply sufficient context, reduce short prompt penalty
                score += 0.05;
            } else {
                score += 0.2;
                gap_zones.push("prompt_too_short".into());
            }
        } else {
            score += 0.2;
            gap_zones.push("prompt_too_short".into());
        }
    } else if word_count > 40 {
        score -= 0.15;
    }

    if repr.compression_ratio < 0.8 {
        score += 0.2;
        gap_zones.push("output_format_ambiguous".into());
    }

    if repr.intent_vec.is_empty() {
        if let Some(rag) = derived_rag {
            if rag.covered_gaps.contains("domain_context_missing") || !rag.facts.is_empty() {
                score += 0.05;
            } else {
                score += 0.3;
                gap_zones.push("domain_context_missing".into());
            }
        } else {
            score += 0.3;
            gap_zones.push("domain_context_missing".into());
        }
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

