/// Canonical token estimator — BPE approximation (words * 1.3)
pub fn estimate_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count();
    (words as f64 * 1.3).ceil() as u32
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
        assert_eq!(estimate_tokens("one two three four five six seven eight nine ten"), 13);
    }
}
