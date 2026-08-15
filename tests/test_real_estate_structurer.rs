use token_compress_engine::npae::aggressive::config::ConfigLoader;
use token_compress_engine::npae::aggressive::intent::{IntentProfile, IntentClass, KnowledgeLevel};
use token_compress_engine::npae::schema::types::DeliverableType;
use token_compress_engine::npae::aggressive::role::generate_role;
use token_compress_engine::npae::aggressive::structurer::render_crisp_prompt;
use token_compress_engine::npae::aggressive::resolver::ResolvedPrompt;
use token_compress_engine::npae::aggressive::structurer;

#[test]
fn test_real_estate_prompt_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "I want to become real estate agent. need to build good company tell me the steps";

    let profile = IntentProfile {
        primary_intent: IntentClass::Build,
        deliverable_type: DeliverableType::Strategy,
        domain: "real-estate".to_string(),
        confidence: 0.95,
        user_knowledge: KnowledgeLevel::Intermediate,
        output_preference: "markdown".to_string(),
        temporal_scope: "unspecified".to_string(),
        has_phases: true,
        has_timeline: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: Some("Real Estate Agency".to_string()),
        detected_team: None,
        dynamic_subject: Some("Real Estate Company".to_string()),
    };

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.starts_with("Real Estate Agent"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    println!("Rendered Output:\n{}", rendered);

    assert!(rendered.contains("# ROLE: REAL ESTATE AGENT"));
    assert!(rendered.contains("Real Estate Agent"));
    assert!(!rendered.contains("FINANCE FULL STACK DEVELOPER"));
    assert!(rendered.contains("Licensing & Pre-Registration"));
    assert!(rendered.contains("Client Acquisition & Expansion"));
}
