use super::PrincipleResult;
use std::collections::HashSet;
use std::time::Instant;

pub fn run(text: &str, max_tokens: usize) -> PrincipleResult {
    let start = Instant::now();
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<&str> = Vec::new();
    let mut duplicates = 0usize;

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
    result = result
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");

    // token budget (keep consistent with pipeline's 1.3 tokens/word heuristic)
    let word_limit = (max_tokens as f64 / 1.3) as usize;
    let mut truncated = false;
    // CRISP stages prepend a role/constraints "header" before the `---` separator.
    // To avoid header overhead consuming the user's requested budget, we only
    // apply truncation to the content body after the first `---` line.
    if let Some(sep_idx) = result.lines().position(|l| l.trim() == "---") {
        let lines: Vec<&str> = result.lines().collect();
        if sep_idx + 1 < lines.len() {
            let prefix = lines[..=sep_idx].join("\n");
            let body = lines[sep_idx + 1..].join("\n");
            let body_words: Vec<&str> = body.split_whitespace().collect();
            if body_words.len() > word_limit {
                truncated = true;
                let truncated_body = body_words[..word_limit].join(" ");
                result = if prefix.trim().is_empty() {
                    truncated_body
                } else if truncated_body.trim().is_empty() {
                    prefix
                } else {
                    format!("{}\n{}", prefix, truncated_body)
                };
            }
        }
    } else {
        let words: Vec<&str> = result.split_whitespace().collect();
        if words.len() > word_limit {
            result = words[..word_limit].join(" ");
            truncated = true;
        }
    }

    let mut detail = format!("Deduped {} lines · whitespace normalized", duplicates);
    if truncated {
        if result.lines().any(|l| l.trim() == "---") {
            detail.push_str(&format!(
                " · truncated body to {} words (header not budgeted)",
                word_limit
            ));
        } else {
            detail.push_str(&format!(" · truncated to {} words", word_limit));
        }
    }

    PrincipleResult {
        text: result.clone(),
        chunks: vec![result],
        items_removed: duplicates,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
