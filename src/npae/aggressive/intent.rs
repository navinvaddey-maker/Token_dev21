use crate::npae::compression::types::CompressedRepr;

#[derive(Debug, Clone, PartialEq)]
pub enum IntentClass { Build, Explain, Debug, Analyze, Transform }

#[derive(Debug, Clone, PartialEq)]
pub enum KnowledgeLevel { Novice, Intermediate, Expert }

#[derive(Debug, Clone)]
pub struct IntentProfile {
    pub primary_intent:    IntentClass,
    pub domain:            String,
    pub user_knowledge:    KnowledgeLevel,
    pub temporal_scope:    String,
    pub output_preference: String,
    pub confidence:        f32,
}

pub fn extract(repr: &CompressedRepr, raw: &str) -> Result<IntentProfile, String> {
    if repr.intent_vec.is_empty() {
        return Err("intent_vec is empty".into());
    }

    // Heuristic domain detection
    let domain = detect_domain(raw);

    // simplistic mapping: locate max in intent_vec
    let (max_idx, max_val) = repr.intent_vec.iter().enumerate()
        .fold((0, 0.0f32), |(max_i, max_v), (i, &v)| {
            if v > max_v { (i, v) } else { (max_i, max_v) }
        });

    let primary_intent = match max_idx % 5 {
        0 => IntentClass::Build,
        1 => IntentClass::Explain,
        2 => IntentClass::Debug,
        3 => IntentClass::Analyze,
        _ => IntentClass::Transform,
    };

    let user_knowledge = if max_val > 0.8 {
        KnowledgeLevel::Expert
    } else if max_val > 0.5 {
        KnowledgeLevel::Intermediate
    } else {
        KnowledgeLevel::Novice
    };

    Ok(IntentProfile {
        primary_intent,
        domain,
        user_knowledge,
        temporal_scope: "immediate".into(),
        output_preference: "json".into(),
        confidence: max_val,
    })
}

fn detect_domain(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("nutritionist") || lower.contains("runner") || lower.contains("athletes") || lower.contains("meal") {
        "sports-nutrition".into()
    } else if lower.contains("code") || lower.contains("rust") || lower.contains("bug") || lower.contains("api") {
        "software-engineering".into()
    } else if lower.contains("article") || lower.contains("blog") || lower.contains("writing") {
        "technical-writing".into()
    } else {
        "general".into()
    }
}
