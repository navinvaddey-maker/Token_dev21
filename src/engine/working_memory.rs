use std::collections::HashSet;
use std::time::Instant;
use super::PrincipleResult;

pub fn run(text: &str, max_tokens: usize) -> PrincipleResult {
    let start = Instant::now();
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<&str>    = Vec::new();
    let mut duplicates            = 0usize;

    for line in text.lines() {
        let normalized = line.trim().to_lowercase();
        if normalized.is_empty() {
            deduped.push(line);
        } else if seen.contains(&normalized) {
            duplicates += 1;
        } else {
            seen.insert(normalized);
            deduped.push(line);
        }
    }

    let mut result = deduped.join("\n");

    // whitespace normalization
    result = result.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");

    // token budget
    let word_limit = (max_tokens as f64 * 0.75) as usize;
    let words: Vec<&str> = result.split_whitespace().collect();
    let mut truncated = false;
    if words.len() > word_limit {
        result    = words[..word_limit].join(" ");
        truncated = true;
    }

    let mut detail = format!("Deduped {} lines · whitespace normalized", duplicates);
    if truncated { detail.push_str(&format!(" · truncated to {} words", word_limit)); }

    PrincipleResult {
        text:          result.clone(),
        chunks:        vec![result],
        items_removed: duplicates,
        detail,
        duration_ms:   start.elapsed().as_millis() as u64,
    }
}
