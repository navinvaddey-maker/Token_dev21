use serde::Serialize;
use std::collections::HashSet;
use tracing::info;

/// GAP-17: Formalize implicit signals for Hebbian learning
#[derive(Debug, Serialize)]
pub enum ImplicitSignal {
    RecompressionWithinSession { gap_secs: u32 },
    OutputEditDistance { normalized: f32 },
    SessionAbandonment { stage_reached: u8 },
    DownstreamModelSuccess { task_score: f32 },
}

pub struct DetectionContext {
    pub user_id: String,
    pub history_id: String,
    pub prev_prompt: Option<String>,
    pub curr_prompt: String,
    pub response_time_ms: u64,
    pub engagement_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct DetectedSignal {
    pub signal_type: String,
    pub signal_layer: i64,
    pub value: f64,
    pub meta: serde_json::Value,
}

/// Jaccard similarity between two token sets
fn jaccard(a: &str, b: &str) -> f64 {
    let sa: HashSet<&str> = a.split_whitespace().collect();
    let sb: HashSet<&str> = b.split_whitespace().collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Layer 3 signal detection — runs automatically after every compression
pub fn detect(ctx: &DetectionContext) -> Vec<DetectedSignal> {
    let mut signals: Vec<DetectedSignal> = Vec::new();

    // ── Repetition (Jaccard ≥ 0.75 between prev and curr prompt)
    if let Some(prev) = &ctx.prev_prompt {
        let similarity = jaccard(prev, &ctx.curr_prompt);
        if similarity >= 0.75 {
            info!(signal = "repetition", similarity = similarity);
            signals.push(DetectedSignal {
                signal_type: "repetition".into(),
                signal_layer: 3,
                value: similarity,
                meta: serde_json::json!({ "jaccard": similarity }),
            });
        }

        // ── Forgot / re-explanation
        let forgot_phrases = [
            "as i said",
            "you forgot",
            "i already mentioned",
            "as mentioned",
            "remember",
        ];
        if forgot_phrases
            .iter()
            .any(|p| ctx.curr_prompt.to_lowercase().contains(p))
        {
            info!(signal = "forgot");
            signals.push(DetectedSignal {
                signal_type: "forgot".into(),
                signal_layer: 3,
                value: 1.0,
                meta: serde_json::json!({}),
            });
        }
    }

    // ── Fast reprompt (< 15 seconds)
    if ctx.response_time_ms < 15_000 {
        info!(signal = "fast_reprompt", ms = ctx.response_time_ms);
        signals.push(DetectedSignal {
            signal_type: "fast_reprompt".into(),
            signal_layer: 3,
            value: 1.0,
            meta: serde_json::json!({ "response_time_ms": ctx.response_time_ms }),
        });
    }

    // ── Long engagement (> 5 minutes — positive signal)
    if ctx.engagement_ms > 300_000 {
        info!(signal = "long_engagement", ms = ctx.engagement_ms);
        signals.push(DetectedSignal {
            signal_type: "long_engagement".into(),
            signal_layer: 3,
            value: 1.0,
            meta: serde_json::json!({ "engagement_ms": ctx.engagement_ms }),
        });
    }

    signals
}
