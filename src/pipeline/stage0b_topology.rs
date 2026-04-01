use crate::types::{AlgorithmOutput, PromptTopology};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

lazy_static! {
    /// Hierarchical indicators: markdown headers, indentation patterns, numbering schemes
    static ref HIERARCHICAL_PATTERNS: Vec<Regex> = vec![
        Regex::new(r#"(?m)^\s*#{1,6}\s+"#).unwrap(), // Markdown headers
        Regex::new(r#"(?m)^\s*\d+\.\s+"#).unwrap(),   // Numbered lists
        Regex::new(r#"(?m)^\s*[a-zA-Z]\.\s+"#).unwrap(), // Lettered lists
        Regex::new(r#"(?m)^\s*[-*+]\s+"#).unwrap(),    // Bullet points
        Regex::new(r#"(?m)^\s*>{1,}\s+"#).unwrap(),    // Blockquotes
        Regex::new(r#"(?m)^\s*```.*?```"#).unwrap(),   // Code blocks
    ];

    /// Network indicators: connective words, relationship terms, graph-like language
    static ref NETWORK_PATTERNS: Vec<Regex> = vec![
        Regex::new(r#"(?i)\b(?:connected|related|linked|associated|correlated|interconnected)\b"#).unwrap(),
        Regex::new(r#"(?i)\b(?:node|edge|vertex|graph|network|web|mesh|topology)\b"#).unwrap(),
        Regex::new(r#"(?i)\b(?:depends on|leads to|results in|causes|triggers|influences)\b"#).unwrap(),
        Regex::new(r#"(?i)\b(?:and|or|but|however|although|while|whereas)\b"#).unwrap(),
    ];

    /// Linear indicators: sequential markers, temporal language, procedural language
    static ref LINEAR_PATTERNS: Vec<Regex> = vec![
        Regex::new(r#"(?i)\b(?:first|second|third|fourth|fifth|next|then|finally|last|lastly)\b"#).unwrap(),
        Regex::new(r#"\b(?:step|stage|phase|part|section)\s+\d+"#).unwrap(),
        Regex::new(r#"(?i)\b(?:begin|start|commence|initiate|proceed|continue|advance|move|go)\b"#).unwrap(),
        Regex::new(r#"(?i)\b(?:end|finish|complete|conclude|terminate|stop|halt)\b"#).unwrap(),
        Regex::new(r#"(?i)\b(?:before|after|during|while|until|since|when|once)\b"#).unwrap(),
    ];
}

/// Classifies the topological structure of a prompt
pub struct TopologyClassifier;

impl TopologyClassifier {
    /// Classify the topology of a prompt and update the AlgorithmOutput
    /// Returns the classified topology for chaining if needed
    pub fn classify(&self, prompt: &str, out: &mut AlgorithmOutput) -> PromptTopology {
        let topology = Self::detect_topology(prompt);
        out.topology = Some(topology.clone());
        topology
    }

    /// Detect topology from prompt text using pattern matching
    fn detect_topology(prompt: &str) -> PromptTopology {
        let mut scores = HashMap::new();
        scores.insert(
            PromptTopology::Hierarchical,
            Self::score_patterns(prompt, &HIERARCHICAL_PATTERNS),
        );
        scores.insert(
            PromptTopology::Network,
            Self::score_patterns(prompt, &NETWORK_PATTERNS),
        );
        scores.insert(
            PromptTopology::Linear,
            Self::score_patterns(prompt, &LINEAR_PATTERNS),
        );

        // Flat is default - score based on lack of other patterns
        let max_other_score = scores.values().cloned().fold(0.0, f32::max);
        scores.insert(PromptTopology::Flat, 1.0 - max_other_score.min(1.0));

        // Find topology with highest score
        scores
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k)
            .unwrap_or(PromptTopology::Flat)
    }

    /// Score how well a prompt matches a set of patterns
    fn score_patterns(prompt: &str, patterns: &[Regex]) -> f32 {
        let mut matches = 0.0f32;
        let total_chars = prompt.chars().count() as f32;

        if total_chars == 0.0 {
            return 0.0;
        }

        for pattern in patterns {
            matches += pattern.find_iter(prompt).count() as f32;
        }

        // Normalize by text length and number of patterns
        let raw_score = matches / (total_chars * patterns.len() as f32);
        (raw_score * 10.0).min(1.0) // Cap at 1.0, scale up for sensitivity
    }

    /// Create a new TopologyClassifier instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TopologyClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Get prior probability for processing mode based on prompt topology
/// Returns a tuple (gentle_prior, aggressive_prior) where values sum to 1.0
pub fn topology_mode_prior(topology: PromptTopology) -> (f32, f32) {
    match topology {
        PromptTopology::Linear => (0.6, 0.4), // Linear flows suit gentle processing but allow aggressive for technical content
        PromptTopology::Hierarchical => (0.4, 0.6), // Hierarchical needs moderate aggressive
        PromptTopology::Network => (0.2, 0.8), // Network structures benefit from aggressive
        PromptTopology::Flat => (0.8, 0.2),   // Flat text is usually simple, gentle preferred
    }
}
