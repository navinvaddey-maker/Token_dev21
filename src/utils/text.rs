//! Deterministic boundary-safe text matching, tokenization, and sentence segmentation.
//!
//! // ARCH: ARCHITECTURE.md §1.4, §5 (GAP-42), §6 — Consolidates word/phrase boundary checks,
//! // identifier-bounded matching, alphanumeric/hyphen tokenization, contiguous span lookup,
//! // slug title-casing, decimal-safe sentence splitting, and Jaccard token similarity.

use std::collections::HashSet;

/// Stateless deterministic text matching and segmentation utility.
#[derive(Debug, Clone, Copy, Default)]
pub struct OryxTextMatcher;

impl OryxTextMatcher {
    /// Checks whether `haystack_lower` contains `needle` on ASCII alphanumeric word or phrase
    /// boundaries, advancing safely across UTF-8 character boundaries.
    ///
    /// Prevents substring false positives such as `"app"` matching `"appraisal"` or `"appreciate"`,
    /// or `"earn"` matching `"learn"`.
    ///
    /// # Arguments
    /// * `haystack_lower` - Lowercased text to search.
    /// * `needle` - Lowercased word or phrase to locate (leading/trailing whitespace trimmed).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn contains_whole_word_or_phrase(haystack_lower: &str, needle: &str) -> bool {
        let target = needle.trim();
        if target.is_empty() {
            return false;
        }
        let bytes = haystack_lower.as_bytes();
        let t_bytes = target.as_bytes();
        if t_bytes.len() > bytes.len() {
            return false;
        }

        let is_word_char = |b: u8| b.is_ascii_alphanumeric();
        let mut start = 0usize;
        while let Some(rel_idx) = haystack_lower[start..].find(target) {
            let idx = start + rel_idx;
            let end = idx + t_bytes.len();
            let left_ok = idx == 0 || !is_word_char(bytes[idx - 1]);
            let right_ok = end == bytes.len() || !is_word_char(bytes[end]);
            if left_ok && right_ok {
                return true;
            }
            start = idx + 1;
            while start < haystack_lower.len() && !haystack_lower.is_char_boundary(start) {
                start += 1;
            }
        }
        false
    }

    /// Returns `true` when `haystack_lower` contains `pattern` bounded on both sides by
    /// non-identifier characters (`!c.is_alphanumeric() && c != '_'`).
    ///
    /// # Arguments
    /// * `haystack_lower` - Lowercased haystack string.
    /// * `pattern` - Non-empty lowercased phrase to match on identifier boundaries.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn contains_identifier_bounded_phrase(haystack_lower: &str, pattern: &str) -> bool {
        if pattern.is_empty() {
            return false;
        }
        let mut start = 0usize;
        while let Some(rel_idx) = haystack_lower[start..].find(pattern) {
            let abs_start = start + rel_idx;
            let abs_end = abs_start + pattern.len();
            let left_ok = haystack_lower[..abs_start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            let right_ok = haystack_lower[abs_end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            if left_ok && right_ok {
                return true;
            }
            let mut next_start = abs_end;
            while next_start < haystack_lower.len() && !haystack_lower.is_char_boundary(next_start)
            {
                next_start += 1;
            }
            start = next_start;
        }
        false
    }

    /// Tokenizes `text` into lowercase alphanumeric and hyphenated tokens (`!c.is_alphanumeric() && c != '-'`).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn tokenize_alphanumeric_words(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
            .collect()
    }

    /// Returns `true` when `haystack` contains `needle` as a contiguous slice span.
    ///
    /// Returns `false` if `needle` is empty or longer than `haystack`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn contains_contiguous_token_span<T: PartialEq>(haystack: &[T], needle: &[T]) -> bool {
        if needle.is_empty() || haystack.len() < needle.len() {
            return false;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// Formats a slug or snake/kebab-case identifier into space-separated Title Case words.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn format_slug_as_title_case(slug: &str) -> String {
        slug.split(['-', '_', ' '])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(first) => {
                        let mut title = first.to_uppercase().to_string();
                        title.push_str(chars.as_str());
                        title
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Splits `text` into sentences on `.`, `!`, `?`, and newlines while preserving decimal
    /// numbers such as `$4.2M` or `99.95%`, and retaining trailing `?` on questions.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn split_sentences_preserving_decimals(text: &str) -> Vec<String> {
        let mut sentences = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut current = String::new();

        for (idx, &ch) in chars.iter().enumerate() {
            if ch == '\n' || ch == '!' || ch == '?' {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    // Keep trailing '?' so callers can distinguish questions from declarative facts.
                    let mut entry = trimmed.to_string();
                    if ch == '?' {
                        entry.push('?');
                    }
                    sentences.push(entry);
                }
                current.clear();
                continue;
            }

            if ch == '.' {
                let prev_is_digit = idx > 0 && chars.get(idx - 1).is_some_and(char::is_ascii_digit);
                let next_is_digit = chars.get(idx + 1).is_some_and(char::is_ascii_digit);
                if prev_is_digit && next_is_digit {
                    current.push(ch);
                    continue;
                }
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    sentences.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }

            current.push(ch);
        }

        let trailing = current.trim();
        if !trailing.is_empty() {
            sentences.push(trailing.to_string());
        }
        sentences
    }

    /// Computes Jaccard set similarity (`|A ∩ B| / |A ∪ B|`) in `[0.0, 1.0]` over two token lists.
    ///
    /// Returns `1.0` if both token slices are empty, and `0.0` if exactly one is empty.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn compute_jaccard_token_similarity(tokens_a: &[String], tokens_b: &[String]) -> f32 {
        // Explicit `allow`: token set sizes in prompt/fact comparisons are tiny compared to f32 mantissa.
        if tokens_a.is_empty() && tokens_b.is_empty() {
            return 1.0;
        }
        if tokens_a.is_empty() || tokens_b.is_empty() {
            return 0.0;
        }
        let set_a: HashSet<&str> = tokens_a.iter().map(String::as_str).collect();
        let set_b: HashSet<&str> = tokens_b.iter().map(String::as_str).collect();
        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();
        if union == 0 {
            0.0
        } else {
            (intersection as f32) / (union as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_whole_word_or_phrase() {
        assert!(!OryxTextMatcher::contains_whole_word_or_phrase(
            "home appraisal and appreciate",
            "app"
        ));
        assert!(!OryxTextMatcher::contains_whole_word_or_phrase(
            "learn rust today",
            "earn"
        ));
        assert!(OryxTextMatcher::contains_whole_word_or_phrase(
            "build a mobile app today",
            "mobile app"
        ));
        assert!(!OryxTextMatcher::contains_whole_word_or_phrase(
            "café naïve",
            ""
        ));
        // Multi-byte UTF-8 boundary traversal when first partial match fails boundary check
        assert!(OryxTextMatcher::contains_whole_word_or_phrase(
            "learn café earn more",
            "earn"
        ));
    }

    #[test]
    fn test_contains_identifier_bounded_phrase() {
        assert!(!OryxTextMatcher::contains_identifier_bounded_phrase(
            "foo_bar", ""
        ));
        assert!(!OryxTextMatcher::contains_identifier_bounded_phrase(
            "my_table_name",
            "table"
        ));
        assert!(OryxTextMatcher::contains_identifier_bounded_phrase(
            "do not use table here",
            "table"
        ));
        assert!(OryxTextMatcher::contains_identifier_bounded_phrase(
            "use my_table, then table.",
            "table"
        ));
    }

    #[test]
    fn test_tokenize_and_contiguous_span() {
        let tokens = OryxTextMatcher::tokenize_alphanumeric_words("Use Rust-2021, zero_copy!");
        assert_eq!(tokens, vec!["use", "rust-2021", "zero", "copy"]);
        assert!(OryxTextMatcher::contains_contiguous_token_span(
            &tokens,
            &["rust-2021".to_string(), "zero".to_string()]
        ));
        assert!(!OryxTextMatcher::contains_contiguous_token_span(
            &tokens,
            &[]
        ));
        assert!(!OryxTextMatcher::contains_contiguous_token_span(
            &tokens,
            &["zero".to_string(), "rust-2021".to_string()]
        ));
    }

    #[test]
    fn test_format_slug_as_title_case() {
        assert_eq!(
            OryxTextMatcher::format_slug_as_title_case("shopping_list-v2"),
            "Shopping List V2"
        );
        assert_eq!(
            OryxTextMatcher::format_slug_as_title_case("  api__rate-limit  "),
            "Api Rate Limit"
        );
    }

    #[test]
    fn test_split_sentences_preserving_decimals() {
        let text = "Revenue hit $4.2M with 99.95% uptime. Is it scalable? Yes!\nNext line";
        let split = OryxTextMatcher::split_sentences_preserving_decimals(text);
        assert_eq!(
            split,
            vec![
                "Revenue hit $4.2M with 99.95% uptime",
                "Is it scalable?",
                "Yes",
                "Next line"
            ]
        );
    }

    #[test]
    fn test_compute_jaccard_token_similarity() {
        assert!(
            (OryxTextMatcher::compute_jaccard_token_similarity(&[], &[]) - 1.0).abs()
                < f32::EPSILON
        );
        let a = vec!["rust".to_string(), "sqlx".to_string()];
        assert!(
            (OryxTextMatcher::compute_jaccard_token_similarity(&a, &[]) - 0.0).abs() < f32::EPSILON
        );
        let b = vec!["rust".to_string(), "axum".to_string()];
        // intersection = 1 ("rust"), union = 3 ("rust", "sqlx", "axum")
        let sim = OryxTextMatcher::compute_jaccard_token_similarity(&a, &b);
        assert!((sim - (1.0 / 3.0)).abs() < 1e-6);
    }
}
