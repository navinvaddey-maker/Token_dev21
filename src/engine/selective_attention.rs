use std::collections::HashSet;
use std::time::Instant;
use super::PrincipleResult;

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(String::from)
        .collect()
}

fn score(chunk: &str, task_words: &HashSet<String>) -> f64 {
    let words: Vec<String> = chunk
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(String::from)
        .collect();
    if words.is_empty() { return 0.0; }
    words.iter().filter(|w| task_words.contains(w.as_str())).count() as f64 / words.len() as f64
}

pub fn run(chunks: &[String], task: &str, mode: &str, protected: &[String]) -> PrincipleResult {
    let start     = Instant::now();
    let threshold = match mode { "gentle" => 0.03, "aggressive" => 0.20, _ => 0.10 };
    let task_words = tokenize(task);

    let mut kept: Vec<String> = chunks.iter()
        .filter(|c| {
            // always keep chunks containing a protected entity
            let is_protected = protected.iter().any(|e| c.to_lowercase().contains(&e.to_lowercase()));
            is_protected || score(c, &task_words) >= threshold
        })
        .cloned()
        .collect();

    // E004 fallback — never return empty
    if kept.is_empty() {
        kept = chunks.iter().take(5).cloned().collect();
    }

    let dropped = chunks.len().saturating_sub(kept.len());

    PrincipleResult {
        text:          kept.join("\n\n"),
        chunks:        kept.clone(),
        items_removed: dropped,
        detail:        format!("Kept {}/{} chunks (threshold: {:.2}, protected: {})",
                               kept.len(), chunks.len(), threshold, protected.len()),
        duration_ms:   start.elapsed().as_millis() as u64,
    }
}
