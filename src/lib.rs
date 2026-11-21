pub mod algorithms;
pub mod api;
pub mod behavior;
pub mod classifier;
pub mod correction;
pub mod data;
pub mod db;
pub mod domain;
pub mod engine;
pub mod errors;
pub mod models;
pub mod npae;
pub mod pipeline;
pub mod rag;
pub mod scoring;
pub mod session;
pub mod types;
pub mod utils;

pub mod scenario;

use db::DbPool;
use learning_engine::TenantLearningPool;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

use crate::npae::aggressive::config::ConfigHandle;
use crate::pipeline::orchestrator::PipelineOrchestrator;

/// Shared application state injected into Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub learning_pool: Arc<TenantLearningPool>,
    pub pipeline: Arc<PipelineOrchestrator>,
    pub npae_config: Arc<ConfigHandle>,
    pub ory_engine: Arc<Mutex<crate::npae::ory::OryEngine>>,
    pub rag_store: Arc<crate::rag::store::RagStore>,
    pub sessions: Arc<dashmap::DashMap<String, crate::session::SessionHistory>>,
    pub embedding_engine: Arc<crate::rag::embeddings::EmbeddingEngine>,
    pub scenario_router: Arc<crate::scenario::ScenarioModeRouter>,
    pub pdf_ingest_sem: Arc<Semaphore>,
    pub aggressive_sem: Arc<Semaphore>,
    pub compress_sem: Arc<Semaphore>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("sessions_count", &self.sessions.len())
            .field(
                "pdf_ingest_permits",
                &self.pdf_ingest_sem.available_permits(),
            )
            .field(
                "aggressive_permits",
                &self.aggressive_sem.available_permits(),
            )
            .field("compress_permits", &self.compress_sem.available_permits())
            .finish_non_exhaustive()
    }
}
