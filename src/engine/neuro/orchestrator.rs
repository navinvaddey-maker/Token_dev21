use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::types::{AlgorithmOutput, CompressionResponse, ScoringResult, PromptTopology, NormalizationResult};
use super::context::PipelineContext;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StageError {
    #[error("Correction loop exceeded max depth")]
    CorrectionLoopExceeded,
    #[error("Degradation: {0}")]
    Degradation(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type StageFuture<'a> = Pin<Box<
    dyn Future<Output = Result<Arc<PipelineContext>, StageError>> + Send + 'a
>>;

pub trait Stage: Send + Sync {
    fn name(&self) -> &'static str;

    fn execute<'a>(
        &'a self,
        ctx:    Arc<PipelineContext>,
        output: &'a mut AlgorithmOutput,
    ) -> StageFuture<'a>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StageIndex {
    Stage0A = 0,
    Stage0B = 1,
    Stage1  = 2,
    Stage2  = 3,
    Stage3  = 4,
    Stage4  = 5,
    Stage4B = 6,
    Stage5  = 7,
}

pub struct PipelineOrchestrator {
    pub stages: Vec<Box<dyn Stage>>,
    pub state_bus: broadcast::Sender<(String, serde_json::Value)>,
}

impl PipelineOrchestrator {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            stages,
            state_bus: tx,
        }
    }

    pub async fn run(&mut self, mut ctx: Arc<PipelineContext>, output: &mut AlgorithmOutput)
        -> Result<Arc<PipelineContext>, StageError>
    {
        for stage in &self.stages {
            ctx = stage.execute(ctx, output).await?;
        }
        Ok(ctx)
    }

    pub async fn run_from_stage(
        &mut self,
        ctx:        Arc<PipelineContext>,
        from:       StageIndex,
        output:     &mut AlgorithmOutput,
    ) -> Result<CompressionResponse, StageError> {
        let mut current_ctx = ctx;
        for stage in self.stages.iter().skip(from as usize) {
            current_ctx = stage.execute(current_ctx, output).await?;
        }
        
        let scoring_result = ScoringResult::default(); 
        output.scoring_result = Some(scoring_result.clone());
        Ok(self.package_response(output, scoring_result))
    }

    pub fn package_response(&self, _output: &AlgorithmOutput, scoring_result: ScoringResult) -> CompressionResponse {
        CompressionResponse {
            mode: "Gentle".into(),
            error_score: 0.0,
            fidelity: 0.0,
            schema: crate::types::CompressionSchema::default(),
            wm_slots_used: 0,
            null_fields: vec![],
            response: "".into(),
            normalization: NormalizationResult::default(),
            field_issues: vec![],
            topology: PromptTopology::default(),
            scope_injections: vec![],
            clusters: None,
            delta_tokens: None,
            reasoning_chain: None,
            scoring_result,
            correction_cycle: crate::types::CorrectionCycle::default(),
        }
    }
}
