use token_compress_engine::npae::aggressive::config::ConfigLoader;
use token_compress_engine::npae::aggressive::intent::{IntentClass, IntentProfile, KnowledgeLevel};
use token_compress_engine::npae::aggressive::resolver::ResolvedPrompt;
use token_compress_engine::npae::aggressive::role::generate_role;
use token_compress_engine::npae::aggressive::structurer;
use token_compress_engine::npae::aggressive::structurer::render_crisp_prompt;
use token_compress_engine::npae::schema::types::DeliverableType;

#[test]
fn test_real_estate_prompt_structuring() {
    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let raw_input =
        "I want to become real estate agent. need to build good company tell me the steps";

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
        baseline_knowledge: None,
        goal_intent: None,
        vertical: None,
        secondary_domains: vec![],
        rule_locked: false,
        legal_as_constraint: false,
        composed_primary_role: None,
        perspective: Default::default(),
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

    let reconstructed = token_compress_engine::types::ReconstructedInput::default();
    let structured = structurer::build(
        &profile,
        raw_input,
        &resolved,
        Some(&config),
        &reconstructed,
        None,
    )
    .expect("Failed to build structured prompt");

    let rendered = render_crisp_prompt(&structured, &[], raw_input);

    println!("Rendered Output:\n{}", rendered);

    assert!(rendered.contains("# ROLE: REAL ESTATE AGENT"));
    assert!(rendered.contains("Real Estate Agent"));
    assert!(!rendered.contains("FINANCE FULL STACK DEVELOPER"));
    assert!(rendered.contains("Licensing & Pre-Registration"));
    assert!(rendered.contains("Client Acquisition & Expansion"));
}

#[tokio::test]
async fn test_buying_a_house_all_modes_clean_aggressive() {
    use dashmap::DashMap;
    use std::sync::Arc;
    use token_compress_engine::npae::aggressive::config::ConfigHandle;
    use token_compress_engine::npae::ory::OryEngine;
    use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
    use token_compress_engine::session::SessionHistory;
    use token_compress_engine::types::OrchestratorResponse;

    let config = ConfigLoader::load("config/unified.json").expect("Failed to load unified.json");
    let npae_config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(tokio::sync::Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());
    let orchestrator = PipelineOrchestrator::build(schema_priors, npae_config_handle, ory_engine);

    let house_prompt = "Hi there! I'm really excited and also honestly pretty nervous because my spouse and I have finally decided to take the plunge and buy our very first house together, and we have been saving up for years and years to get to this point, so it feels like a huge milestone in our lives. The thing is, we don't really know where to start at all and the whole process seems super overwhelming with so many different moving parts, legal jargon, financial requirements, and hidden costs that people warn you about. Could you please walk me through the entire process of buying a house from the very beginning all the way to getting the keys, in as much detail as possible?\n\nSpecifically, I would love it if you could explain everything we need to know about getting our finances in order first, like what credit score we should aim for, how much money we actually need saved up for a down payment versus closing costs and emergency funds, and how mortgage pre-approval works compared to pre-qualification, as well as the different types of loans available like conventional, FHA, or fixed-rate versus adjustable-rate mortgages. After that, could you explain how to find a good real estate agent, what to look for when touring homes and neighborhoods, how to figure out what a fair offer price is, and what kind of contingencies we should definitely include in our purchase agreement—like inspection contingencies, appraisal contingencies, and financing contingencies—so we don't end up stuck in a bad deal?\n\nFurthermore, once an offer is accepted, what happens during the escrow and underwriting period? I want to understand what a home inspector actually checks, how we can negotiate repairs or credits if they find major issues with the roof, foundation, plumbing, or electrical systems, what the lender is doing behind the scenes during underwriting, why we need title insurance and homeowners insurance, and what happens during the final walkthrough and closing day when we have to sign a million documents and wire the funds. Please also include any common rookie mistakes first-time homebuyers make so we can avoid them, and feel free to use clear examples, tips, and thorough explanations for every single step because we want to be completely prepared before we even talk to a lender or an agent!";

    let orig_words = house_prompt.split_whitespace().count();

    for mode in ["gentle", "balanced"] {
        let mut session = SessionHistory::new(10);
        let resp = orchestrator
            .process(house_prompt, &mut session, None, Some(mode), None, None)
            .await
            .unwrap_or_else(|e| panic!("{mode} mode failed: {e}"));
        match resp {
            OrchestratorResponse::Legacy(r) => {
                let final_words = r.response.split_whitespace().count();
                let saved_pct =
                    100.0 * (orig_words.saturating_sub(final_words) as f64) / (orig_words as f64);
                println!(
                    "[{mode}] orig_words={orig_words}, final_words={final_words}, saved_pct={saved_pct:.1}%, tes={:.2}",
                    r.scoring_result.tes
                );
                assert!(
                    final_words < orig_words,
                    "[{mode}] expected token reduction, got orig_words={orig_words} final_words={final_words}"
                );
            }
            OrchestratorResponse::Aggressive(_) => {
                panic!("Expected Legacy response for {mode} mode");
            }
        }
    }

    let mut session = SessionHistory::new(10);
    let aggr_resp = orchestrator
        .process(
            house_prompt,
            &mut session,
            None,
            Some("aggressive"),
            None,
            None,
        )
        .await
        .expect("aggressive mode failed");

    match aggr_resp {
        OrchestratorResponse::Aggressive(r) => {
            println!(
                "[aggressive] orig={}, final={}, saved={}",
                r.token_original, r.token_final, r.token_saved
            );
            println!(
                "=== AGGRESSIVE OUTPUT ===\n{}\n=== END ===",
                r.optimized_prompt
            );
            assert!(
                !r.optimized_prompt.contains("[AVOID]  Exclude them")
                    && !r.optimized_prompt.contains("[AVOID] Exclude them"),
                "Found dangling [AVOID] Exclude them in output"
            );
            assert!(
                !r.optimized_prompt.contains("explanation of Point at"),
                "Found unresolved 'Point' placeholder in # SUCCESS CRITERIA"
            );
            assert!(
                r.optimized_prompt
                    .contains("residential home-buying and mortgage process"),
                "Expected clean subject in # SUCCESS CRITERIA"
            );
        }
        OrchestratorResponse::Legacy(_) => {
            panic!("Expected Aggressive response for aggressive mode");
        }
    }
}
