pub mod algorithms;
pub mod api;
pub mod behavior;
pub mod correction;
pub mod data;
pub mod domain;
pub mod engine;
pub mod errors;
pub mod models;
pub mod npae;
pub mod pipeline;
pub mod scoring;
pub mod session;
pub mod types;

use db::DbPool;
use learning_engine::LearningEngine;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::pipeline::orchestrator::PipelineOrchestrator;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub engine: Arc<Mutex<LearningEngine>>,
    pub pipeline: Arc<PipelineOrchestrator>,
}

