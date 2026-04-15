use std::sync::Arc;
use uuid::Uuid;

use crate::types::{Mode, PromptTopology, NormalizationResult};
use crate::engine::neuro::orchestrator::StageError;
use crate::engine::neuro::correction::CorrectionTier;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityCert {
    pub input_token_count:   usize,
    pub output_token_count:  usize,
    pub compression_ratio:   f32,        // 1 - (output/input)
    pub l2_residual_norm:    f32,        // semantic distance from original
    pub reconstruction_loss: f32,        // 0.0 = lossless, 1.0 = full loss
    pub signal_intact:       bool,       // false if loss > threshold → SignalDegradationError
    pub phrase_survival_rate:f32,        // % of DOMAIN_PHRASES that survived sparse pass
    pub dead_token_ratio:    f32,        // tokens with salience < 0.10 / total
}

impl QualityCert {
    pub const LOSS_THRESHOLD: f32 = 0.35;   // triggers SignalDegradationError if exceeded

    pub fn is_acceptable(&self) -> bool {
        self.reconstruction_loss <= Self::LOSS_THRESHOLD && self.signal_intact
    }
}

#[derive(Debug, Clone)]
pub struct PipelineContext {
    pub request_id:          Uuid,
    pub raw_prompt:          String,
    pub normalized_prompt:   String,
    pub normalization_result:NormalizationResult,
    pub topology:            PromptTopology,
    pub mode:                Mode,
    pub wm_capacity:         usize,
    pub quality_cert:        Option<QualityCert>,
    pub correction_tier:     Option<CorrectionTier>,
    pub fork_depth:          u8,
}

#[derive(Debug)]
pub enum ContextDelta {
    Topology(PromptTopology),
    ModeDecision { mode: Mode, wm_capacity: usize },
    QualityCert(QualityCert),
    CorrectionEntry(CorrectionTier),
    Passthrough,
}

impl ContextDelta {
    pub fn apply(self, ctx: &mut PipelineContext) {
        match self {
            Self::Topology(t)                         => ctx.topology = t,
            Self::ModeDecision { mode, wm_capacity }  => {
                ctx.mode        = mode;
                ctx.wm_capacity = wm_capacity;
            }
            Self::QualityCert(cert)                   => ctx.quality_cert = Some(cert),
            Self::CorrectionEntry(tier)               => ctx.correction_tier = Some(tier),
            Self::Passthrough                         => {}
        }
    }
}

impl PipelineContext {
    pub fn fork(&self, delta: ContextDelta) -> Result<Arc<Self>, StageError> {
        if self.fork_depth >= 3 {
            return Err(StageError::CorrectionLoopExceeded);
        }
        let mut next = self.clone();
        delta.apply(&mut next);
        next.fork_depth += 1;
        Ok(Arc::new(next))
    }
}
