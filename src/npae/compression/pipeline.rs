use crate::npae::compression::merge::{merge_stage1, merge_stage2, merge_stage3};
use crate::npae::compression::semantic;
use crate::npae::compression::types::{CompressedRepr, StageMetrics};
use rayon;
use std::time::Instant;

pub fn run_parallel_pipeline(raw: &str) -> Result<CompressedRepr, String> {
    let tokens: Vec<String> = raw.split_whitespace().map(String::from).collect();

    let lower_raw = raw.to_lowercase();
    let lower_raw_clone = lower_raw.clone();

    let t0 = Instant::now();
    let (s1_lex, s1_spr) = rayon::join(
        || {
            // Real lexical compression (stop words removal)
            let stop_words = [
                "the", "a", "an", "this", "that", "it", "i", "you", "he", "she", "we", "they",
                "is", "are", "was", "were", "will", "would", "can", "could", "to", "and", "or",
                "of", "in", "for", "with", "on", "at", "by",
            ];
            tokens
                .iter()
                .filter(|t| !stop_words.contains(&t.to_lowercase().as_str()))
                .cloned()
                .collect::<Vec<String>>()
        },
        || {
            // Sparse prune mask
            vec![true; tokens.len()]
        },
    );
    let s1 = merge_stage1(s1_lex, s1_spr)?;
    let stage1_ms = t0.elapsed().as_millis() as u64;

    let t1 = Instant::now();
    let (s2_sem, s2_prd) = rayon::join(
        || semantic::compress(&s1),
        || {
            // FIX: Step 1.2 & 1.4 — Match on whole words (not substrings like "prefix" -> "fix"
            // or "anyhow" -> "why") and normalize `intent_vec` so it is a proper probability
            // distribution summing to 1.0 as documented in `CompressedRepr`.
            let lower = lower_raw_clone;
            let words: std::collections::HashSet<&str> = lower
                .split(|c: char| !c.is_alphanumeric() && c != '-')
                .filter(|s| !s.is_empty())
                .collect();
            let has_word = |w: &str| -> bool {
                if w.contains(' ') {
                    lower.contains(w)
                } else {
                    words.contains(w)
                }
            };

            let mut dist = vec![0.1_f32; 5];

            if [
                "create", "build", "make", "design", "develop", "plan", "draft",
            ]
            .iter()
            .any(|w| has_word(w))
            {
                dist[0] += 0.5; // Build
            }
            if [
                "explain", "how to", "what is", "describe", "clarify", "teach",
            ]
            .iter()
            .any(|w| has_word(w))
            {
                dist[1] += 0.6; // Explain
            }
            if ["fix", "bug", "error", "debug", "troubleshoot", "patch"]
                .iter()
                .any(|w| has_word(w))
            {
                dist[2] += 0.7; // Debug
            }
            if [
                "analyze", "why", "compare", "evaluate", "review", "assess", "audit",
            ]
            .iter()
            .any(|w| has_word(w))
            {
                dist[3] += 0.6; // Analyze (on par with Explain; slightly above incidental "design"/"build" nouns)
            }
            if ["refactor", "rewrite", "transform", "convert", "migrate"]
                .iter()
                .any(|w| has_word(w))
            {
                dist[4] += 0.6; // Transform
            }

            // Ensure at least one intent is dominant if no verb matched
            let no_match = dist.iter().all(|&x| (x - 0.1).abs() < f32::EPSILON);
            if no_match {
                dist[0] = 0.6; // Default to Build
            }

            // Normalize to a probability distribution summing to 1.0
            let sum: f32 = dist.iter().sum();
            if sum > 0.0 {
                for v in &mut dist {
                    *v /= sum;
                }
            }

            Ok::<Vec<f32>, String>(dist)
        },
    );
    let s2 = merge_stage2(s2_sem?, s2_prd?)?;
    let stage2_ms = t1.elapsed().as_millis() as u64;

    let t2 = Instant::now();
    let (s3_heb, s3_cmp) = rayon::join(
        || {
            // Basic Hebbian association based on input
            let mut concepts = Vec::new();
            if lower_raw.contains("business") {
                concepts.push("strategy".to_string());
            }
            if lower_raw.contains("code") || lower_raw.contains("software") {
                concepts.push("architecture".to_string());
            }
            if lower_raw.contains("nutrition") {
                concepts.push("health".to_string());
            }
            if concepts.is_empty() {
                concepts.push("general_execution".to_string());
            }
            Ok::<Vec<String>, String>(concepts)
        },
        || {
            // Competitive node activation scores
            Ok::<Vec<i32>, String>(vec![1, 2, 3])
        },
    );
    let stage3_ms = t2.elapsed().as_millis() as u64;

    merge_stage3(
        &s2,
        s3_heb?,
        s3_cmp?,
        StageMetrics {
            stage1_ms,
            stage2_ms,
            stage3_ms,
        },
    )
}
