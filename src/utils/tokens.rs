//! Canonical token estimator for prompt budgeting and compression metrics.
//!
//! // ARCH: ARCHITECTURE.md §1.2, §6 — Single canonical BPE-calibrated token estimator
//! // shared across pipeline stages, RAG context budgeting, and scoring.

/// Canonical BPE-calibrated token estimator.
#[derive(Debug, Clone, Copy, Default)]
pub struct OryxTokenEstimator;

impl OryxTokenEstimator {
    /// Estimates BPE token count using a calibrated `words * 1.3` prose heuristic plus
    /// subword and punctuation expansion for code, JSON, URLs, and long identifiers.
    ///
    /// # Arguments
    /// * `text` - Input text slice to estimate token count for.
    ///
    /// # Panics
    /// Never.
    ///
    /// # Invariants
    /// Returns `0` for empty or whitespace-only inputs; saturates at `u32::MAX` for
    /// astronomically large inputs instead of wrapping or panicking.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn estimate_bpe_tokens(text: &str) -> u32 {
        // Explicit `allow` above: BPE prose calibration multiplies word count by `1.3` and rounds
        // up via `ceil()`, bounded via `u32::try_from` / `saturating_add` so no overflow or wrap occurs.
        let mut words = 0usize;
        let mut extra_subwords = 0usize;
        for tok in text.split_whitespace() {
            words += 1;
            let len = tok.len();
            if len > 12 {
                extra_subwords += (len - 12) / 6;
            }
            let code_punct = tok
                .bytes()
                .filter(|b| {
                    matches!(
                        b,
                        b'{' | b'}' | b'[' | b']' | b'(' | b')' | b'=' | b';' | b':' | b'/'
                    )
                })
                .count();
            if code_punct >= 2 {
                extra_subwords += code_punct / 2;
            }
        }
        let base = (words as f64 * 1.3).ceil() as usize;
        u32::try_from(base.saturating_add(extra_subwords)).unwrap_or(u32::MAX)
    }
}

/// Free-function alias for [`OryxTokenEstimator::estimate_bpe_tokens`].
///
/// # Panics
/// Never.
#[inline]
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    OryxTokenEstimator::estimate_bpe_tokens(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   \n\t  "), 0);
        assert_eq!(OryxTokenEstimator::estimate_bpe_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_basic() {
        assert_eq!(estimate_tokens("hello"), 2);
        assert_eq!(estimate_tokens("hello world"), 3);
        assert_eq!(
            estimate_tokens("one two three four five six seven eight nine ten"),
            13
        );
        assert_eq!(
            OryxTokenEstimator::estimate_bpe_tokens("hello world"),
            estimate_tokens("hello world")
        );
    }

    #[test]
    fn test_estimate_tokens_code_and_long_identifiers() {
        // 1 word ("super_long_identifier_name" = 26 bytes -> extra (26-12)/6 = 2) + ceil(1.3)=2 => 4
        assert_eq!(
            OryxTokenEstimator::estimate_bpe_tokens("super_long_identifier_name"),
            4
        );
        // JSON/code punctuation adds extra subwords: 1 word, len=15 ((15-12)/6=0), punct `{`, `:`, `}` = 3 (+1) => 2 + 1 = 3
        assert_eq!(
            OryxTokenEstimator::estimate_bpe_tokens("{\"key\":\"value\"}"),
            3
        );
        // 1 word, len=20 ((20-12)/6=1), punct `{`, `:`, `}` = 3 (+1) => 2 + 1 + 1 = 4
        assert_eq!(
            OryxTokenEstimator::estimate_bpe_tokens("{\"key\":\"long_value\"}"),
            4
        );
    }
}
