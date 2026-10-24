use dashmap::DashMap;
use std::sync::Arc;
use token_compress_engine::api::CompressRequest;
use token_compress_engine::npae::aggressive::config::{ConfigHandle, ConfigLoader};
use token_compress_engine::npae::ory::OryEngine;
use token_compress_engine::npae::schema::types::NpaeConfig;
use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
use token_compress_engine::session::SessionHistory;
use token_compress_engine::types::OrchestratorResponse;

#[tokio::test]
async fn test_npae_config_override_threading() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let npae_config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(tokio::sync::Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());

    let orchestrator = PipelineOrchestrator::build(schema_priors, npae_config_handle, ory_engine);
    let mut session = SessionHistory::new(10);

    let ambiguous_input = "Tell me how to fix the server issue";

    // 1. Force aggressive mode with high ambiguity threshold (e.g. 0.99) -> should suppress clarifying questions
    let override_high_threshold = NpaeConfig {
        ambiguity_threshold: Some(0.99),
        max_questions: Some(1),
        confidence_threshold: Some(0.8),
        skip_stage: None,
    };

    let resp_high = orchestrator
        .process(
            ambiguous_input,
            &mut session,
            None,
            Some("aggressive"),
            Some(&override_high_threshold),
            None,
        )
        .await
        .expect("Process should succeed");

    if let OrchestratorResponse::Aggressive(aggressive_resp) = resp_high {
        assert!(
            aggressive_resp.clarifying_questions.is_empty(),
            "Expected 0 clarifying questions when threshold is set to 0.99"
        );
    } else {
        panic!("Expected OrchestratorResponse::Aggressive");
    }

    // 2. Force aggressive mode with low threshold (0.01) and max_questions = 1
    let override_low_threshold = NpaeConfig {
        ambiguity_threshold: Some(0.01),
        max_questions: Some(1),
        confidence_threshold: Some(0.8),
        skip_stage: None,
    };

    let resp_low = orchestrator
        .process(
            ambiguous_input,
            &mut session,
            None,
            Some("aggressive"),
            Some(&override_low_threshold),
            None,
        )
        .await
        .expect("Process should succeed");

    if let OrchestratorResponse::Aggressive(aggressive_resp) = resp_low {
        assert_eq!(
            aggressive_resp.clarifying_questions.len(),
            1,
            "Expected exactly 1 clarifying question when max_questions is set to 1"
        );
    } else {
        panic!("Expected OrchestratorResponse::Aggressive");
    }
}

#[test]
fn test_compress_request_deserialization_with_npae_fields() {
    let json_data = r#"{
        "raw_text": "Optimize my query",
        "npae_ambiguity_threshold": 0.45,
        "npae_max_questions": 2,
        "npae_confidence_threshold": 0.85
    }"#;

    let req: CompressRequest =
        serde_json::from_str(json_data).expect("Failed to deserialize CompressRequest");
    assert_eq!(req.raw_text, "Optimize my query");
    assert_eq!(req.npae_ambiguity_threshold, Some(0.45));
    assert_eq!(req.npae_max_questions, Some(2));
    assert_eq!(req.npae_confidence_threshold, Some(0.85));
}

#[tokio::test]
async fn test_itinerary_travel_domain_and_role_accuracy() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let npae_config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(tokio::sync::Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());

    let orchestrator = PipelineOrchestrator::build(schema_priors, npae_config_handle, ory_engine);
    let mut session = SessionHistory::new(10);

    let prompt = "Plan a 3-day itinerary in Goa for a family of 4 under 30000 INR, vegetarian food only, then optimize it for cost";

    let resp = orchestrator
        .process(prompt, &mut session, None, Some("aggressive"), None, None)
        .await
        .expect("Process should succeed");

    if let OrchestratorResponse::Aggressive(aggressive_resp) = resp {
        let sp = &aggressive_resp.structured_prompt;
        println!("ROLE: {}", sp.role.primary);
        println!("DOMAIN: {}", sp.context.domain);
        println!("PERSONA ANCHOR: {}", sp.role.persona_anchor);
        println!(
            "PHASES: {:?}",
            sp.execution_phases
                .iter()
                .map(|p| &p.name)
                .collect::<Vec<_>>()
        );

        assert_eq!(
            sp.context.domain, "travel",
            "Domain must be travel, not nutrition"
        );
        assert!(
            sp.role.primary.contains("Travel") || sp.role.primary.contains("Itinerary"),
            "Role must be a Travel/Itinerary specialist, got: {}",
            sp.role.primary
        );
        assert!(
            !sp.role.primary.to_lowercase().contains("dietician"),
            "Role must not hallucinate Dietician"
        );
        assert!(
            !sp.role
                .persona_anchor
                .to_lowercase()
                .contains("dietary programming"),
            "Persona must not hallucinate clinical nutrition"
        );
        assert!(
            sp.execution_phases
                .iter()
                .any(|p| p.name.to_lowercase().contains("logistics")
                    || p.name.to_lowercase().contains("activity")),
            "Execution phases must be travel/itinerary logistics"
        );
    } else {
        panic!("Expected OrchestratorResponse::Aggressive");
    }
}
