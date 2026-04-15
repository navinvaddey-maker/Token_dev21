use std::sync::Arc;
use crate::types::{AlgorithmOutput, CompressionResponse, PromptTopology, NormalizationResult};
use super::context::{PipelineContext, ContextDelta, QualityCert};
use super::orchestrator::{PipelineOrchestrator, StageError, StageIndex};

#[derive(Debug, Clone, PartialEq)]
pub enum CorrectionTier {
    Surface,     // → Stage 6A (format/length fixes only)
    Semantic,    // → Stage 4  (schema refill)
    Structural,  // → Stage 0B (topology reclassify) + Stage 2 (mode redecide)
}

pub struct TopologyClassifier;
impl TopologyClassifier {
    pub fn classify(_text: &str, _norm: &NormalizationResult) -> PromptTopology {
        PromptTopology::Linear // Mock logic
    }
}

pub struct CorrectionRouter;

impl Default for CorrectionRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrectionRouter {
    pub fn new() -> Self {
        Self
    }

    pub fn route(scoring_result: &crate::types::ScoringResult, cert: &QualityCert) -> Option<CorrectionTier> {
        // Structural: intent collision or signal degradation — deepest rollback
        if scoring_result.sfs < 4.0 || !cert.signal_intact {
            return Some(CorrectionTier::Structural);
        }
        // Semantic: field issues or low SFS — re-fill schema
        if scoring_result.sfs < 6.0 {
            return Some(CorrectionTier::Semantic);
        }
        // Surface: TES only — format/token pruning sufficient
        if scoring_result.tes < 6.0 {
            return Some(CorrectionTier::Surface);
        }
        None  // scores acceptable — no correction needed
    }

    /// Structural correction:
    ///   1. Use ctx.normalized_prompt (not raw_prompt) — Stage 0A output preserved
    ///   2. Re-classify topology on normalized text
    ///   3. Fork context with fresh topology
    ///   4. Re-enter pipeline from Stage 0B (not Stage 2) — Stage 0B is cheap, safe
    pub async fn execute_structural(
        ctx:          Arc<PipelineContext>,
        orchestrator: &mut PipelineOrchestrator,
        output:       &mut AlgorithmOutput,
    ) -> Result<CompressionResponse, StageError> {

        // Use normalized_prompt preserved in context — not raw_prompt
        // NormalizationResult is also in context — not default()
        let new_topology = TopologyClassifier::classify(
            &ctx.normalized_prompt,        // ← normalized, not raw
            &ctx.normalization_result,     // ← actual Stage 0A output
        );

        let forked = ctx.fork(ContextDelta::Topology(new_topology))?;
        let forked = forked.fork(ContextDelta::CorrectionEntry(CorrectionTier::Structural))?;

        // Re-enter from Stage 0B — cheap reclassify, then Stage 2 redecides mode
        orchestrator.run_from_stage(forked, StageIndex::Stage0B, output).await
    }
}
