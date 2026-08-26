use token_compress_engine::npae::aggressive::config::ConfigLoader;
use token_compress_engine::npae::aggressive::intent::{extract_with_config, IntentProfile, IntentClass, KnowledgeLevel};
use token_compress_engine::npae::schema::types::DeliverableType;
use token_compress_engine::npae::aggressive::role::generate_role;
use token_compress_engine::npae::aggressive::structurer::render_crisp_prompt;
use token_compress_engine::npae::aggressive::resolver::ResolvedPrompt;
use token_compress_engine::npae::aggressive::structurer;
use token_compress_engine::npae::compression::types::{CompressedRepr, StageMetrics};
use dashmap::DashMap;

fn mock_compressed_repr(intent_vec: Vec<f32>) -> CompressedRepr {
    CompressedRepr {
        token_ids: vec![],
        attention_weights: vec![],
        concept_graph: DashMap::new(),
        intent_vec,
        compression_ratio: 1.0,
        stage_metrics: StageMetrics {
            stage1_ms: 0,
            stage2_ms: 0,
            stage3_ms: 0,
        },
    }
}

#[test]
fn test_finance_income_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "I want earn more money";

    let profile = IntentProfile {
        primary_intent: IntentClass::Build,
        deliverable_type: DeliverableType::Strategy,
        domain: "finance".to_string(),
        confidence: 0.95,
        user_knowledge: KnowledgeLevel::Intermediate,
        output_preference: "structured_markdown".to_string(),
        temporal_scope: "unspecified".to_string(),
        has_phases: true,
        has_timeline: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: None,
        detected_team: None,
        dynamic_subject: Some("Money".to_string()),
    };

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Wealth & Income Strategist"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    assert!(rendered.contains("WEALTH & INCOME STRATEGIST"));
    assert!(rendered.contains("Financial & Asset Audit"));
    assert!(rendered.contains("Strategy & Vehicle Selection"));
    assert!(rendered.contains("Execution & Reinvestment"));
}

#[test]
fn test_finance_earn_millions_e2e_intent_and_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "I want earn millions";

    let repr = mock_compressed_repr(vec![0.9, 0.1, 0.1, 0.1, 0.1]);

    let profile = extract_with_config(&repr, raw_input, Some(&config)).expect("Failed to extract intent");
    assert_eq!(profile.domain, "finance");
    assert_eq!(profile.dynamic_subject, Some("Millions".to_string()));

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Wealth & Income Strategist"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    assert!(rendered.contains("WEALTH & INCOME STRATEGIST"));
    assert!(rendered.contains("Financial & Asset Audit"));
    assert!(rendered.contains("Strategy & Vehicle Selection"));
    assert!(rendered.contains("Execution & Reinvestment"));
    assert!(!rendered.contains("Registered Dietician"));
    assert!(!rendered.contains("Software Architect"));
}

#[test]
fn test_finance_wealth_passive_income_e2e() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "How to build wealth and generate passive income";

    let repr = mock_compressed_repr(vec![0.9, 0.1, 0.1, 0.1, 0.1]);

    let profile = extract_with_config(&repr, raw_input, Some(&config)).expect("Failed to extract intent");
    assert_eq!(profile.domain, "finance");

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Wealth & Income Strategist"));
}

#[test]
fn test_computers_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "Design a high throughput computer operating system kernel";

    let profile = IntentProfile {
        primary_intent: IntentClass::Build,
        deliverable_type: DeliverableType::Artifact,
        domain: "computers".to_string(),
        confidence: 0.95,
        user_knowledge: KnowledgeLevel::Expert,
        output_preference: "code".to_string(),
        temporal_scope: "unspecified".to_string(),
        has_phases: true,
        has_timeline: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: None,
        detected_team: None,
        dynamic_subject: Some("Operating System Kernel".to_string()),
    };

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Computer Systems Architect"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    assert!(rendered.contains("COMPUTER SYSTEMS ARCHITECT"));
    assert!(rendered.contains("System Architecture & Specs"));
    assert!(rendered.contains("Core System Implementation"));
}

#[test]
fn test_science_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "Design a scientific experiment to test quantum coherence hypothesis";

    let profile = IntentProfile {
        primary_intent: IntentClass::Build,
        deliverable_type: DeliverableType::Analysis,
        domain: "science".to_string(),
        confidence: 0.95,
        user_knowledge: KnowledgeLevel::Expert,
        output_preference: "markdown".to_string(),
        temporal_scope: "unspecified".to_string(),
        has_phases: true,
        has_timeline: false,
        has_validation: false,
        detected_geography: None,
        detected_risk: None,
        detected_biz_type: None,
        detected_team: None,
        dynamic_subject: Some("Quantum Coherence Hypothesis".to_string()),
    };

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Lead Research Scientist"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    assert!(rendered.contains("LEAD RESEARCH SCIENTIST"));
    assert!(rendered.contains("Hypothesis & Experimental Design"));
    assert!(rendered.contains("Data Acquisition & Execution"));
}

#[test]
fn test_health_structuring_e2e() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input = "Design a clinical health protocol for patient vitality";

    let repr = mock_compressed_repr(vec![0.9, 0.1, 0.1, 0.1, 0.1]);

    let profile = extract_with_config(&repr, raw_input, Some(&config)).expect("Failed to extract intent");
    assert_eq!(profile.domain, "health");

    let role = generate_role(&profile, raw_input, &config.domain_taxonomy, &config.roles);
    assert!(role.contains("Clinical Health Specialist"));

    let resolved = ResolvedPrompt {
        role: role.clone(),
        inclusions: vec![],
        forbidden: vec![],
        confidence: 0.95,
        status: "RESOLVED".to_string(),
    };

    let structured = structurer::build(&profile, raw_input, &resolved, Some(&config)).expect("Failed to build structured prompt");
    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    assert!(rendered.contains("CLINICAL HEALTH SPECIALIST"));
    assert!(rendered.contains("Health Assessment & Baseline Audit"));
    assert!(rendered.contains("Intervention & Protocol Design"));
    assert!(rendered.contains("Monitoring & Long-Term Adaptation"));
}
