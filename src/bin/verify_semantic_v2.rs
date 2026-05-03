use token_compress_engine::{
    pipeline::orchestrator::PipelineOrchestrator,
    session::SessionHistory,
    types::OrchestratorResponse,
    npae::aggressive::config::{UnifiedConfig, ConfigHandle},
};
use std::sync::Arc;
use dashmap::DashMap;

fn main() {
    let schema_priors = Arc::new(DashMap::new());
    let npae_config_data = token_compress_engine::npae::aggressive::config::ConfigLoader::load("/Users/navinvaddey/token_compress_engine/config/unified.json").unwrap();
    let npae_config = Arc::new(ConfigHandle::new(npae_config_data));
    let orchestrator = PipelineOrchestrator::build(schema_priors, npae_config);
    let mut session = SessionHistory::new(10);

    let prompts = vec![
        "Write some rust code for a backend api",
        "Create a go-to-market strategy for a SaaS product",
        "Draft a legal notice for contract breach",
        "Translate this into Spanish",
    ];

    for prompt in prompts {
        println!("\n--- Prompt: {} ---", prompt);
        if let Ok(OrchestratorResponse::Aggressive(resp)) = orchestrator.process(prompt, &mut session, Some("aggressive")) {
            println!("{}", resp.optimized_prompt);
        }
    }
}
