//! Stage 0B: Prompt Topology Classification (`ARCHITECTURE.md` §1.2, §1.4).
//!
//! Classifies the structural topology (`Linear`, `Hierarchical`, `Network`, `Flat`)
//! of an incoming prompt using `ScenarioSignal` fast-path rules across canonical
//! domains and task types, or lexical pattern scoring when `ScenarioSignal` is absent.

use crate::types::{AlgorithmOutput, PromptTopology};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

fn compile_regexes(patterns: &[&str]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

lazy_static! {
    /// Hierarchical indicators: markdown headers, indentation patterns, numbering schemes.
    static ref HIERARCHICAL_PATTERNS: Vec<Regex> = compile_regexes(&[
        r"(?m)^\s*#{1,6}\s+",
        r"(?m)^\s*\d+\.\s+",
        r"(?m)^\s*[a-zA-Z]\.\s+",
        r"(?m)^\s*[-*+]\s+",
        r"(?m)^\s*>{1,}\s+",
        r"(?m)^\s*```.*?```",
    ]);

    /// Network indicators: connective words, relationship terms, graph-like language.
    static ref NETWORK_PATTERNS: Vec<Regex> = compile_regexes(&[
        r"(?i)\b(?:connected|related|linked|associated|correlated|interconnected)\b",
        r"(?i)\b(?:node|edge|vertex|graph|network|web|mesh|topology)\b",
        r"(?i)\b(?:depends on|leads to|results in|causes|triggers|influences)\b",
        r"(?i)\b(?:and|or|but|however|although|while|whereas)\b",
    ]);

    /// Linear indicators: sequential markers, temporal language, procedural language.
    static ref LINEAR_PATTERNS: Vec<Regex> = compile_regexes(&[
        r"(?i)\b(?:first|second|third|fourth|fifth|next|then|finally|last|lastly)\b",
        r"\b(?:step|stage|phase|part|section)\s+\d+",
        r"(?i)\b(?:begin|start|commence|initiate|proceed|continue|advance|move|go)\b",
        r"(?i)\b(?:end|finish|complete|conclude|terminate|stop|halt)\b",
        r"(?i)\b(?:before|after|during|while|until|since|when|once)\b",
    ]);
}

/// Classifies the topological structure of a prompt.
#[derive(Debug)]
pub struct TopologyClassifier;

impl TopologyClassifier {
    /// Classify the topology of a prompt and update the `AlgorithmOutput`.
    ///
    /// Returns the classified topology for chaining if needed.
    ///
    /// # Panics
    /// Never.
    pub fn classify(&self, prompt: &str, out: &mut AlgorithmOutput) -> PromptTopology {
        let topology = if let Some(ref signal) = out.scenario {
            if signal.confidence >= 0.75 {
                self.derive_topology_from_domain_and_task(&signal.domain, &signal.task_type)
            } else {
                Self::detect_topology(prompt)
            }
        } else {
            Self::detect_topology(prompt)
        };
        out.topology = Some(topology.clone());
        topology
    }

    /// Maps canonical `domain` + `task_type` combinations to their expected topology.
    ///
    /// Used as a fast-path when scenario confidence >= 0.75 (`ARCHITECTURE.md` §1.4).
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn derive_topology_from_domain_and_task(
        &self,
        domain: &str,
        task_type: &str,
    ) -> PromptTopology {
        let dom = domain.to_lowercase();
        let task = task_type.to_lowercase();

        match (dom.as_str(), task.as_str()) {
            // Troubleshooting / debugging across any domain follows a sequential diagnostic chain
            (_, "troubleshooting") => PromptTopology::Linear,

            // Legal & Cybersecurity analysis / compliance map relational obligations & threat vectors
            ("legal" | "cybersecurity", "analysis" | "creation" | "planning") => {
                PromptTopology::Network
            }
            ("ai-ml" | "data-science" | "science" | "scientific-research", "analysis") => {
                PromptTopology::Network
            }

            // Structured creation, architecture, and multi-phase planning across canonical domains
            (
                "software"
                | "software-engineering"
                | "tech"
                | "devops"
                | "devops-infra"
                | "computers"
                | "ai-ml"
                | "data-science"
                | "real-estate"
                | "finance"
                | "business"
                | "business-strategy"
                | "marketing"
                | "ecommerce"
                | "education"
                | "medical"
                | "health"
                | "pharma",
                "creation" | "planning" | "analysis" | "optimization",
            ) => PromptTopology::Hierarchical,

            // Sequential schedules, itineraries, meal plans, and step-by-step explanations
            ("nutrition" | "sports-nutrition" | "travel", "creation" | "planning") => {
                PromptTopology::Linear
            }
            (_, "explanation") => PromptTopology::Linear,
            (_, "planning") => PromptTopology::Hierarchical,

            _ => PromptTopology::Flat,
        }
    }

    /// Detect topology from prompt text using pattern matching.
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
        let max_other_score = scores.values().copied().fold(0.0, f32::max);
        scores.insert(PromptTopology::Flat, 1.0 - max_other_score.min(1.0));

        // Find topology with highest score
        scores
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k)
            .unwrap_or(PromptTopology::Flat)
    }

    /// Score how well a prompt matches a set of patterns.
    fn score_patterns(prompt: &str, patterns: &[Regex]) -> f32 {
        let mut matches = 0.0f32;
        let total_chars = prompt.chars().count() as f32;

        if total_chars == 0.0 || patterns.is_empty() {
            return 0.0;
        }

        for pattern in patterns {
            matches += pattern.find_iter(prompt).count() as f32;
        }

        // Normalize by text length and number of patterns
        let raw_score = matches / (total_chars * patterns.len() as f32);
        (raw_score * 10.0).min(1.0)
    }

    /// Create a new `TopologyClassifier` instance.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for TopologyClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Get prior probability for processing mode based on prompt topology.
///
/// Returns a tuple `(gentle_prior, aggressive_prior)` where values sum to `1.0`.
///
/// # Panics
/// Never.
#[must_use]
pub fn topology_mode_prior(topology: PromptTopology) -> (f32, f32) {
    match topology {
        PromptTopology::Linear => (0.6, 0.4),
        PromptTopology::Hierarchical => (0.4, 0.6),
        PromptTopology::Network => (0.2, 0.8),
        PromptTopology::Flat => (0.8, 0.2),
    }
}
