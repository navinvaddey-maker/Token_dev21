use crate::{
    algorithms::predictive_coding::PredictiveCoding, session::SessionHistory,
    types::AlgorithmOutput,
};

/// Handles Stage 2: Boundary Detection (Predictive Coding → mode decision)
pub struct Stage2 {
    predictive: PredictiveCoding,
}

impl Stage2 {
    /// Creates a new Stage2 instance
    pub fn new(predictive: PredictiveCoding) -> Self {
        Self { predictive }
    }

    /// Processes input through Stage 2: Boundary Detection
    /// Always runs, both modes. Output: out.mode is set here.
    pub fn run(&self, session: &SessionHistory, out: &mut AlgorithmOutput) {
        let result = self.predictive.compute_error(
            &out.sparse_tokens,
            &session.turns,
            out.topology.clone().unwrap_or_default(),
            out,
        );
        out.error_score = result.error_score;
        out.delta_tokens = result.delta_tokens;
        out.mode = Some(result.mode);
        out.is_ambiguous = result.is_ambiguous;
    }

    /// Exposes schema updates directly for the orchestrator to call post-completion.
    pub fn update_schema(&self, delta_tokens: &[String]) {
        self.predictive.update_schema(delta_tokens);
    }

    /// Exposes feedback application directly for the orchestrator.
    pub fn apply_feedback(&self, tokens: &[String], weight: f32) {
        self.predictive.apply_feedback(tokens, weight);
    }
}
