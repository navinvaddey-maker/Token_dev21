use super::PrincipleResult;
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::Instant;

static HEDGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(maybe|perhaps|possibly|sort of|kind of|i guess|i think|i believe)\b")
        .unwrap()
});
static COURTESY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(please|thank you|thanks in advance|hi|hello|hey|if possible|if you can|if you could)\b").unwrap()
});
static FILLER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(basically|actually|just|really|very|quite|so|well|literally|essentially)\b")
        .unwrap()
});
static META_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(i want you to|i need you to|i was wondering if|could you maybe|could you possibly|could you please|i.?d like you to)\b").unwrap()
});
static PASSIVE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b\w+ (should be|needs to be|must be|has to be) \w+ed( by you)?\b").unwrap()
});
static ARTICLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(the|a|an) (following|above|below|given)\b").unwrap());
static SELF_REF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(see above|as mentioned above|as stated previously|as discussed)\b").unwrap()
});
static DUP_SEP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"---\s*---(\s*---)*").unwrap());
static QUOTE_NOISE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""\."\.""#).unwrap());
static SPACES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());

pub fn run(text: &str, mode: &str) -> PrincipleResult {
    let start = Instant::now();
    let mut result = text.to_string();
    let mut removed: usize = 0;

    let patterns: &[&Lazy<Regex>] = match mode {
        "gentle" => &[&FILLER_RE, &COURTESY_RE, &SELF_REF_RE],
        "aggressive" => &[
            &HEDGE_RE,
            &COURTESY_RE,
            &FILLER_RE,
            &META_RE,
            &PASSIVE_RE,
            &ARTICLE_RE,
            &SELF_REF_RE,
        ],
        _ => &[&HEDGE_RE, &COURTESY_RE, &FILLER_RE, &META_RE, &SELF_REF_RE],
    };

    for pattern in patterns {
        removed += pattern.find_iter(&result).count();
        result = pattern.replace_all(&result, "").to_string();
    }

    // passive-to-active hint: flag passive but only replace in aggressive
    if mode == "aggressive" {
        removed += PASSIVE_RE.find_iter(&result).count();
        result = PASSIVE_RE.replace_all(&result, "").to_string();
    }

    // New rule 6 noise cleanups
    removed += QUOTE_NOISE_RE.find_iter(&result).count();
    result = QUOTE_NOISE_RE.replace_all(&result, ".").to_string();

    removed += DUP_SEP_RE.find_iter(&result).count();
    result = DUP_SEP_RE.replace_all(&result, "---").to_string();

    result = SPACES_RE.replace_all(result.trim(), " ").to_string();

    PrincipleResult {
        text: result.clone(),
        chunks: vec![result],
        items_removed: removed,
        detail: format!("Removed {} zero-signal tokens (mode: {})", removed, mode),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
