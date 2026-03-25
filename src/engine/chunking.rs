use std::time::Instant;
use super::PrincipleResult;

pub fn run(text: &str, use_case: &str) -> PrincipleResult {
    let start = Instant::now();

    let chunks: Vec<String> = match use_case {
        "legal" | "research" | "code" | "financial" => {
            text.split("\n\n")
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect()
        }
        "ticket" | "transcript" => {
            text.split('\n')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect()
        }
        _ => {
            let sentences: Vec<&str> = text
                .split(|c| c == '.' || c == '!' || c == '?')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            sentences.chunks(3).map(|g| g.join(". ") + ".").collect()
        }
    };

    let count = chunks.len();
    PrincipleResult {
        text:          chunks.join("\n\n"),
        chunks,
        items_removed: 0,
        detail:        format!("Split into {} semantic units (strategy: {})", count, use_case),
        duration_ms:   start.elapsed().as_millis() as u64,
    }
}
