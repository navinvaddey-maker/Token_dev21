use crate::pipeline::TopologyClassifier;
use crate::types::AlgorithmOutput;

/// Wrapper around TopologyClassifier that follows the Tee pattern (like TeeNormalizer)
/// This allows topology classification to be inserted into the pipeline as a side effect
pub struct TeeTopologyClassifier;

impl TeeTopologyClassifier {
    /// Classify topology and update AlgorithmOutput, returning nothing (tee pattern)
    pub fn classify(&self, prompt: &str, out: &mut AlgorithmOutput) {
        let classifier = TopologyClassifier::new();
        classifier.classify(prompt, out);
    }

    /// Create a new TeeTopologyClassifier instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for TeeTopologyClassifier {
    fn default() -> Self {
        Self::new()
    }
}
