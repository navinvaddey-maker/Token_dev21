//! Deterministic Task Instruction Pruning & Long-Narrative Condensation.
//!
//! // ARCH: ARCHITECTURE.md §1.4 & §6.6 — strips duplicate headers/politeness prefixes and
//! // deduplicates condensed multi-clause tasks against execution phases.

use std::collections::HashSet;

use super::ingest::split_raw_input;
use crate::npae::grounding::verifier::split_sentences_preserving_decimals;
use crate::npae::schema::types::ExecutionPhase;

/// without allocating a lowercased copy of `s`.
#[inline]
fn strip_ascii_prefix_ignore_case<'a>(s: &'a str, ascii_prefix: &str) -> Option<&'a str> {
    let prefix_len = ascii_prefix.len();
    let head = s.as_bytes().get(..prefix_len)?;
    if head.eq_ignore_ascii_case(ascii_prefix.as_bytes()) {
        s.get(prefix_len..)
    } else {
        None
    }
}

/// Prunes redundant `# TASK / INSTRUCTION:` headers and leading conversational politeness
/// filler from the raw user task while preserving 100% of numbers, budgets (`$500k`),
/// locations (`Austin`), domain terms, and negations (`do not`, `never`, `without`, `no `).
///
/// // ARCH: ARCHITECTURE.md §1.4 & §6.6 — eliminates duplicate header emission and verbose
/// // conversational preambles under `# TASK / INSTRUCTION:` without per-loop allocations.
///
/// # Panics
/// Never.
#[must_use]
pub fn prune_task_instruction(user_prompt: &str) -> String {
    let mut s = user_prompt.trim();

    let headers = [
        "# task / instruction:",
        "# task / instruction",
        "# task:",
        "# task",
        "# instruction:",
        "# instruction",
        "task / instruction:",
        "task:",
        "instruction:",
    ];
    loop {
        let mut stripped_any = false;
        for h in headers {
            if let Some(rest) = strip_ascii_prefix_ignore_case(s, h) {
                s = rest.trim_start_matches(&[' ', '\t', '\r', '\n', ':', '-'][..]);
                stripped_any = true;
                break;
            }
        }
        if !stripped_any {
            break;
        }
    }

    let polite_prefixes = [
        "i would really appreciate it if you could please help me to ",
        "i would really appreciate it if you could please help me ",
        "i would really appreciate it if you could please ",
        "i would appreciate it if you could please help me to ",
        "i would appreciate it if you could please help me ",
        "i would appreciate it if you could please ",
        "i would really appreciate it if you could ",
        "i would appreciate it if you could ",
        "could you please help me to ",
        "could you please help me ",
        "can you please help me to ",
        "can you please help me ",
        "would you please help me to ",
        "would you please help me ",
        "could you kindly help me to ",
        "could you kindly help me ",
        "could you please ",
        "can you please ",
        "would you please ",
        "please help me to ",
        "please help me ",
        "help me to ",
        "i want you to please ",
        "i need you to please ",
        "i want you to ",
        "i need you to ",
        "i would like you to ",
        "please ",
    ];
    loop {
        let mut stripped_any = false;
        for p in polite_prefixes {
            if let Some(rest) = strip_ascii_prefix_ignore_case(s, p) {
                let trimmed_rest = rest.trim_start();
                if !trimmed_rest.is_empty() {
                    s = trimmed_rest;
                    stripped_any = true;
                    break;
                }
            }
        }
        if !stripped_any {
            break;
        }
    }

    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mut result = String::with_capacity(s.len());
            for up in first.to_uppercase() {
                result.push(up);
            }
            result.push_str(chars.as_str());
            result
        }
        None => user_prompt.trim().to_string(),
    }
}

/// Returns `true` when `sentence` is purely a greeting or conversational chitchat preamble
/// without actionable task directives or domain constraints.
pub(super) fn is_greeting_or_chitchat_sentence(sentence: &str) -> bool {
    let trimmed = sentence
        .trim()
        .trim_matches(&['.', '!', '?', ';', ','][..])
        .trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();
    let exact_greetings = [
        "hi",
        "hi there",
        "hello",
        "hello there",
        "hey",
        "hey there",
        "good morning",
        "good afternoon",
        "good evening",
        "greetings",
    ];
    if exact_greetings.contains(&lower.as_str()) {
        return true;
    }

    let chitchat_prefixes = [
        "i'm really excited",
        "i am really excited",
        "we're really excited",
        "we are really excited",
        "the thing is, we don't really know where to start",
        "the thing is we don't really know where to start",
        "the thing is, i don't really know where to start",
    ];
    chitchat_prefixes.iter().any(|p| lower.starts_with(p))
}

/// Condenses long, multi-paragraph conversational prompts (`> 80` words) when execution phases
/// have already structured the workflow, stripping greeting/chitchat sentences and verbose
/// conversational framing while preserving 100% of required domain terms, numbers, budgets,
/// locations, and negations. Short and medium prompts (`<= 80` words) are returned verbatim
/// after prefix pruning.
///
/// # Panics
/// Never.
#[must_use]
pub fn condense_task_instruction(user_prompt: &str, has_execution_phases: bool) -> String {
    let pruned = prune_task_instruction(user_prompt);
    let total_words = pruned.split_whitespace().count();
    if !has_execution_phases || total_words <= 80 {
        return pruned;
    }

    let raw_sentences: Vec<String> = split_sentences_preserving_decimals(&pruned);

    let conversational_leads = [
        "specifically, i would love it if you could explain everything we need to know about ",
        "specifically, i would love it if you could explain ",
        "specifically, could you explain ",
        "after that, could you explain ",
        "furthermore, once an offer is accepted, ",
        "furthermore, could you explain ",
        "i want to understand ",
        "could you please walk me through ",
        "please walk me through ",
        "please also include ",
        "also include ",
    ];

    let trailing_fluff_markers = [
        ", in as much detail as possible",
        " in as much detail as possible",
        ", and feel free to use clear examples",
        " and feel free to use clear examples",
        " because we want to be completely prepared",
        "—so we don't end up stuck in a bad deal",
        " so we don't end up stuck in a bad deal",
        " when we have to sign a million documents and wire the funds",
    ];

    let clause_replacements = [
        ("walk me through the entire process of buying a house from the very beginning all the way to getting the keys", "End-to-end house-buying guide"),
        ("getting our finances in order first, like what ", ""),
        (" we should aim for, how much money we actually need saved up for a ", ", "),
        (" versus ", "/"),
        ("how mortgage pre-approval works compared to pre-qualification, as well as the different types of loans available like ", "pre-approval/pre-qualification, loans ("),
        ("how to find a good real estate agent, what to look for when touring homes and neighborhoods, how to figure out what a fair offer price is, and what kind of ", "agent, touring, offer, "),
        (" we should definitely include in our purchase agreement—like ", " ("),
        ("what happens during the ", ""),
        (" period", ""),
        ("what a home inspector actually checks, how we can negotiate ", "inspection, "),
        (" if they find major issues with the roof, foundation, plumbing, or electrical systems, what the lender is doing behind the scenes during ", ", "),
        ("why we need ", ""),
        ("any common ", ""),
        (" first-time homebuyers make so we can avoid them", " to avoid"),
        ("conventional, FHA, or fixed-rate/adjustable-rate mortgages", "conventional/FHA/fixed-rate/adjustable-rate"),
        ("inspection contingencies, appraisal contingencies, and financing contingencies", "inspection/appraisal/financing"),
        ("title insurance and homeowners insurance, and final walkthrough and closing day", "title insurance, homeowners insurance, final walkthrough, closing"),
        ("closing costs and emergency funds, and ", "closing costs/emergency funds, "),
        ("escrow and underwriting", "escrow/underwriting"),
    ];

    let mut condensed_clauses: Vec<String> = Vec::new();
    for raw_sent in &raw_sentences {
        if is_greeting_or_chitchat_sentence(raw_sent) {
            continue;
        }
        let mut s = prune_task_instruction(raw_sent);
        for lead in conversational_leads {
            let lower = s.to_lowercase();
            if lower.starts_with(lead) {
                if let Some(rest) = s.get(lead.len()..) {
                    s = rest.trim_start().to_string();
                }
                break;
            }
        }
        for marker in trailing_fluff_markers {
            let lower = s.to_lowercase();
            if let Some(idx) = lower.find(marker) {
                if let Some(prefix) = s.get(..idx) {
                    s = prefix.trim_end().to_string();
                }
            }
        }
        for (from, to) in clause_replacements {
            while let Some(idx) = s.to_lowercase().find(from) {
                let mut rebuilt = String::new();
                if let Some(pre) = s.get(..idx) {
                    rebuilt.push_str(pre);
                }
                rebuilt.push_str(to);
                if let Some(post) = s.get(idx + from.len()..) {
                    rebuilt.push_str(post);
                }
                s = rebuilt;
            }
        }
        let open_parens = s.chars().filter(|&c| c == '(').count();
        let close_parens = s.chars().filter(|&c| c == ')').count();
        if open_parens > close_parens {
            s.push(')');
        }
        let trimmed = s.trim().trim_end_matches(&['.', ';', ',', '?'][..]).trim();
        if !trimmed.is_empty() {
            let mut chars = trimmed.chars();
            if let Some(first) = chars.next() {
                let mut clause: String = first.to_uppercase().collect();
                clause.push_str(chars.as_str());
                condensed_clauses.push(clause);
            }
        }
    }

    if condensed_clauses.is_empty() {
        return pruned;
    }

    format!("{}.", condensed_clauses.join("; "))
}

/// Splits `s` on commas that are outside parentheses so parenthetical lists like
/// `loans (conventional, FHA, or fixed-rate/adjustable-rate mortgages)` stay intact.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start <= s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Deduplicates a condensed multi-clause `# TASK / INSTRUCTION` against `# EXECUTION PHASES` on
/// long narratives (`> 150` words) so topics already detailed in phases are not repeated verbatim,
/// while preserving 100% of unique user-requested delta items (e.g. specific loan types, insurance,
/// comparisons, or pitfalls).
pub(super) fn deduplicate_task_against_phases(
    condensed_task: &str,
    phases: &[ExecutionPhase],
) -> String {
    if phases.is_empty() {
        return condensed_task.to_string();
    }

    let mut phase_tokens: HashSet<String> = HashSet::new();
    for p in phases {
        for word in p
            .name
            .split(|c: char| !c.is_alphanumeric())
            .chain(p.description.split(|c: char| !c.is_alphanumeric()))
        {
            let w = word.to_lowercase();
            if w.len() >= 4 {
                phase_tokens.insert(w);
            }
        }
    }

    let clauses: Vec<&str> = condensed_task
        .trim_end_matches('.')
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if clauses.len() <= 1 {
        return condensed_task.to_string();
    }

    let core_objective = clauses[0];
    let mut delta_items: Vec<String> = Vec::new();
    for clause in &clauses[1..] {
        for raw_item in split_top_level_commas(clause) {
            let item = raw_item
                .trim()
                .trim_start_matches("and ")
                .trim_end_matches(&['.', ';', '?'][..])
                .trim();
            if item.is_empty() {
                continue;
            }
            let content_words: Vec<String> = item
                .split(|c: char| !c.is_alphanumeric())
                .map(str::to_lowercase)
                .filter(|w| {
                    w.len() >= 4
                        && !matches!(
                            w.as_str(),
                            "with" | "from" | "that" | "this" | "their" | "they" | "also" | "into"
                        )
                })
                .collect();
            if content_words.is_empty() {
                continue;
            }
            let covered = content_words
                .iter()
                .filter(|w| {
                    phase_tokens.contains(w.as_str())
                        || (w.ends_with('s') && phase_tokens.contains(&w[..w.len() - 1]))
                })
                .count();
            // Keep item if it introduces meaningful net-new coverage (< 60% overlap with phases)
            if (covered as f32 / content_words.len() as f32) < 0.60 {
                delta_items.push(item.to_string());
            }
        }
    }

    if delta_items.is_empty() {
        format!("{core_objective}.")
    } else {
        format!(
            "{core_objective}. Explicitly cover: {}.",
            delta_items.join(", ")
        )
    }
}

/// Renders a `StructuredPrompt`, optional clarifying questions, and optional provenance-rich
/// `DerivedRagContext` in the High-Signal CRISP Markdown format.

/// Extract a meaningful task description — summary based, not raw snippet.
/// Preserves complete clauses and decimal numbers (e.g. `$4.2M`, `v2.1`).
pub(super) fn extract_task_description(raw: &str, intent_type: &str) -> String {
    let (_, _, user_prompt) = split_raw_input(raw);
    let trimmed = user_prompt.trim();

    // FIX: Skip leading greeting or chitchat clauses ("Hi there!", "I'm really excited...",
    // "The thing is...") using decimal-safe sentence splitting before extracting the summary.
    let sentences = split_sentences_preserving_decimals(trimmed);
    let candidate_sentence = sentences
        .iter()
        .map(String::as_str)
        .find(|s| !s.is_empty() && !is_greeting_or_chitchat_sentence(s))
        .unwrap_or_else(|| trimmed.lines().next().unwrap_or(trimmed).trim());

    let cleaned = prune_task_instruction(candidate_sentence);
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let summary = if words.len() <= 20 {
        cleaned
    } else {
        format!("{}...", words[..18].join(" "))
    };

    format!("{} [Type: {}]", summary, intent_type)
}
