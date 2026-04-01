use crate::types::{AlgorithmOutput, OrdinalSequence};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Existing Lexical Compression algorithm.
/// Performs basic text normalization and stopword removal.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LexicalCompression {
    stopwords: HashSet<String>,
}

impl LexicalCompression {
    pub fn new() -> Self {
        let stopwords = [
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with",
            "by", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do",
            "does", "did", "will", "would", "could", "should", "may", "might", "must", "this",
            "that", "these", "those", "i", "you", "he", "she", "it", "we", "they", "me", "him",
            "her", "us", "them", "over",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();

        Self { stopwords }
    }

    /// Compress text by lowercasing, removing punctuation, and filtering stopwords.
    /// Returns compressed tokens and compression ratio.
    /// Also extracts ordinal sequence for pipeline use.
    pub fn compress(&self, text: &str, output: &mut AlgorithmOutput) -> LexicalResult {
        // Basic tokenization: split on whitespace and punctuation
        let tokens: Vec<String> = text
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .filter_map(|s| {
                if s.is_empty() || self.stopwords.contains(s) {
                    None
                } else {
                    Some(s.to_string())
                }
            })
            .collect();

        let original_word_count = text.split_whitespace().count();
        let compressed_word_count = tokens.len();
        let ratio = if original_word_count > 0 {
            compressed_word_count as f32 / original_word_count as f32
        } else {
            0.0
        };

        // Extract ordinal sequence and store in output for pipeline
        let sequence = OrdinalExtractor::extract(text);
        let score = if sequence.is_empty() { 0.0 } else { 1.0 };
        output.ordinal_sequence = Some(OrdinalSequence { sequence, score });

        LexicalResult {
            tokens,
            ratio,
            input_tokens: original_word_count as u32,
            output_tokens: compressed_word_count as u32,
        }
    }
}

lazy_static::lazy_static! {
    static ref ORDINAL_PATTERN: Regex = Regex::new(r"\b(\d+)(st|nd|rd|th)\b").unwrap();
}

/// Extracts ordinal sequences from text as specified in the refinements guide.
#[derive(Debug, Default)]
pub struct OrdinalExtractor;

impl OrdinalExtractor {
    /// Extract ordinal numbers from text and return them as a sequence.
    ///
    /// # Arguments
    /// * `text` - Input text to extract ordinals from
    ///
    /// # Returns
    /// * `Vec<u32>` - Sequence of ordinal numbers found in the text
    pub fn extract(text: &str) -> Vec<u32> {
        ORDINAL_PATTERN
            .captures_iter(text)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().parse::<u32>().ok()))
            .flatten()
            .collect()
    }

    /// Render the ordinal sequence back to text form.
    ///
    /// # Arguments
    /// * `sequence` - Vector of ordinal numbers
    ///
    /// # Returns
    /// * `String` - Ordinal sequence rendered as text (e.g., "1st 2nd 3rd")
    pub fn render(sequence: &[u32]) -> String {
        sequence
            .iter()
            .map(|n| {
                let suffix = match n % 10 {
                    1 if n % 100 != 11 => "st",
                    2 if n % 100 != 12 => "nd",
                    3 if n % 100 != 13 => "rd",
                    _ => "th",
                };
                format!("{}{}", n, suffix)
            })
            .collect::<Vec<String>>()
            .join(" ")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LexicalResult {
    pub tokens: Vec<String>,
    pub ratio: f32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_compression_removes_stopwords() {
        let lc = LexicalCompression::new();
        let result = lc.compress(
            "the quick brown fox jumps over the lazy dog",
            &mut AlgorithmOutput::default(),
        );
        assert!(!result.tokens.contains(&"the".to_string()));
        assert!(!result.tokens.contains(&"over".to_string()));
        assert!(result.tokens.contains(&"quick".to_string()));
        assert!(result.tokens.contains(&"brown".to_string()));
        assert!(result.tokens.contains(&"fox".to_string()));
        assert!(result.tokens.contains(&"jumps".to_string()));
        assert!(result.tokens.contains(&"lazy".to_string()));
        assert!(result.tokens.contains(&"dog".to_string()));
    }

    #[test]
    fn lexical_compression_lowercases() {
        let lc = LexicalCompression::new();
        let result = lc.compress("THE QUICK BROWN FOX", &mut AlgorithmOutput::default());
        assert_eq!(result.tokens, vec!["quick", "brown", "fox"]);
    }

    #[test]
    fn lexical_compression_ratio() {
        let lc = LexicalCompression::new();
        let result = lc.compress("the quick brown fox", &mut AlgorithmOutput::default());
        // 4 original words, 1 stopword removed = 3 tokens, ratio = 0.75
        assert!((result.ratio - 0.75).abs() < 0.01);
    }

    #[test]
    fn ordinal_extractor_basic() {
        let text = "The 1st and 2nd items are processed before the 3rd one.";
        let sequence = OrdinalExtractor::extract(text);
        assert_eq!(sequence, vec![1, 2, 3]);
    }

    #[test]
    fn ordinal_extractor_render() {
        let sequence = vec![1, 2, 3];
        let rendered = OrdinalExtractor::render(&sequence);
        assert_eq!(rendered, "1st 2nd 3rd");
    }

    #[test]
    fn ordinal_extractor_no_matches() {
        let text = "No ordinal numbers here.";
        let sequence = OrdinalExtractor::extract(text);
        assert!(sequence.is_empty());
    }

    #[test]
    fn test_user_prompt() {
        let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

        println!("Testing user prompt: {}", prompt);
        println!("Word count: {}", prompt.split_whitespace().count());

        let lc = LexicalCompression::new();
        let mut output = AlgorithmOutput::default();

        let result = lc.compress(prompt, &mut output);

        println!("Compressed tokens: {:?}", result.tokens);
        println!("Compression ratio: {:.3}", result.ratio);
        println!("Input tokens: {}", result.input_tokens);
        println!("Output tokens: {}", result.output_tokens);

        if let Some(seq) = &output.ordinal_sequence {
            println!("Ordinal sequence found: {:?}", seq.sequence);
            println!("Ordinal score: {}", seq.score);
        } else {
            println!("No ordinal sequence found");
        }

        // Assertions
        assert!(result.ratio < 1.0, "Compression should reduce token count");
        assert!(
            !result.tokens.is_empty(),
            "Should have some tokens after compression"
        );
        assert!(
            result.tokens.len() < prompt.split_whitespace().count(),
            "Compressed tokens should be fewer than original words"
        );

        // Check that stopwords are removed
        assert!(
            !result.tokens.contains(&"you".to_string()),
            "Should remove 'you'"
        );
        assert!(
            !result.tokens.contains(&"are".to_string()),
            "Should remove 'are'"
        );
        assert!(
            !result.tokens.contains(&"with".to_string()),
            "Should remove 'with'"
        );
        assert!(
            !result.tokens.contains(&"in".to_string()),
            "Should remove 'in'"
        );
        assert!(
            !result.tokens.contains(&"on".to_string()),
            "Should remove 'on'"
        );

        // Check that important words are kept
        assert!(
            result.tokens.contains(&"senior".to_string()),
            "Should keep 'senior'"
        );
        assert!(
            result.tokens.contains(&"cybersecurity".to_string()),
            "Should keep 'cybersecurity'"
        );
        assert!(
            result.tokens.contains(&"expert".to_string()),
            "Should keep 'expert'"
        );
        assert!(
            result.tokens.contains(&"experience".to_string()),
            "Should keep 'experience'"
        );
        assert!(
            result.tokens.contains(&"ransomware".to_string()),
            "Should keep 'ransomware'"
        );
        assert!(
            result.tokens.contains(&"forensics".to_string()),
            "Should keep 'forensics'"
        );
        assert!(
            result.tokens.contains(&"akira".to_string()),
            "Should keep 'akira'"
        );
        assert!(
            result.tokens.contains(&"group".to_string()),
            "Should keep 'group'"
        );
        assert!(
            result.tokens.contains(&"technical".to_string()),
            "Should keep 'technical'"
        );
        assert!(
            result.tokens.contains(&"jargon".to_string()),
            "Should keep 'jargon'"
        );
        assert!(
            result.tokens.contains(&"focusing".to_string()),
            "Should keep 'focusing'"
        );
        assert!(
            result.tokens.contains(&"network".to_string()),
            "Should keep 'network'"
        );
        assert!(
            result.tokens.contains(&"logs".to_string()),
            "Should keep 'logs'"
        );
    }
}
