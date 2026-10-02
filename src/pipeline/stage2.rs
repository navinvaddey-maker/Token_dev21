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
        if let Some(ref signal) = out.scenario {
            if signal.is_high_confidence() {
                // High confidence (>=0.85): Skip PredictiveCoding, derive mode from intent
                out.mode = Some(self.derive_mode_from_scenario_intent(signal));
                return;
            } else if signal.is_usable() {
                // Moderate confidence (0.60-0.84): Run PredictiveCoding, use signal as nudge
                let result = self.predictive.compute_error(
                    &out.sparse_tokens,
                    &session.turns,
                    out.topology.clone().unwrap_or_default(),
                    out,
                );
                out.error_score = result.error_score;
                out.delta_tokens = result.delta_tokens;
                out.mode = Some(self.reconcile_predicted_mode_with_scenario(result.mode, signal));
                out.is_ambiguous = result.is_ambiguous;
                return;
            }
        }

        // No usable scenario: run PredictiveCoding fully unchanged
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

    fn derive_mode_from_scenario_intent(&self, signal: &crate::classifier::signal::ScenarioSignal) -> crate::types::Mode {
        match signal.intent_class.as_str() {
            "wealth-building" | "brand-building" => crate::types::Mode::Balanced,
            "compliance" | "risk"                => crate::types::Mode::Gentle,
            "aggressive-growth"                  => crate::types::Mode::Aggressive,
            _                                    => crate::types::Mode::Balanced,
        }
    }

    fn reconcile_predicted_mode_with_scenario(
        &self,
        predicted: crate::types::Mode,
        signal: &crate::classifier::signal::ScenarioSignal,
    ) -> crate::types::Mode {
        let scenario_suggested = self.derive_mode_from_scenario_intent(signal);
        if predicted == scenario_suggested {
            predicted
        } else {
            predicted // Existing PredictiveCoding wins on conflict
        }
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
