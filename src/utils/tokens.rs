/// Canonical token estimator — BPE-calibrated approximation (`words * 1.3` for prose,
/// plus subword/punctuation expansion for code, JSON, URLs, and long identifiers).
pub fn estimate_tokens(text: &str) -> u32 {
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
    ((words as f64 * 1.3).ceil() as usize + extra_subwords) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   \n\t  "), 0);
    }

    #[test]
    fn test_estimate_tokens_basic() {
        assert_eq!(estimate_tokens("hello"), 2);
        assert_eq!(estimate_tokens("hello world"), 3);
        assert_eq!(
            estimate_tokens("one two three four five six seven eight nine ten"),
            13
        );
    }
}
