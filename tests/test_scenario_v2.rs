use std::sync::Arc;
use token_compress_engine::scenario::{
    ScenarioDomainRegistry, ScenarioEgressGuard, ScenarioModeRouter, ScenarioNamespaceGuard,
    ScenarioStyleRegistry,
};

#[test]
fn test_domain_registry_and_style_registry_startup() {
    let domain_reg = ScenarioDomainRegistry::load_from_file("config/scenario/domains.json")
        .expect("Failed to load domains.json");
    assert!(domain_reg.domains.len() >= 3);
    assert!(domain_reg.get_domain("legal").is_some());
    assert!(domain_reg.get_domain("business").is_some());
    assert!(domain_reg.get_domain("finance").is_some());

    let style_reg = ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
        .expect("Failed to load styles.json and validate prompt files");
    assert!(style_reg.styles.len() >= 4);
    assert!(style_reg.get_style("balanced").is_some());
    assert!(style_reg.get_style("concise").is_some());
    assert!(style_reg.get_style("detailed").is_some());
    assert!(style_reg.get_style("aggressive").is_some());
}

#[test]
fn test_style_registry_fails_on_missing_prompt_file() {
    let res = ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "non_existent_prompts_dir");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Startup validation failed"));
}

#[test]
fn test_egress_guard_allowlist() {
    let guard = ScenarioEgressGuard::load_from_file("config/scenario/egress.json")
        .expect("Failed to load egress config");

    assert!(guard.is_allowed("127.0.0.1"));
    assert!(guard.is_allowed("localhost"));
    assert!(guard.is_allowed("internal-vector-store.local"));
    assert!(guard.is_allowed("private-llm-endpoint.local"));

    assert!(!guard.is_allowed("google.com"));
    assert!(!guard.is_allowed("api.openai.com"));

    assert!(guard.validate_egress("127.0.0.1").is_ok());
    assert!(guard.validate_egress("api.openai.com").is_err());
}

#[test]
fn test_namespace_guard_isolation_and_entitlements() {
    // Scenario mode strictly locks to domain namespace
    let scenario_ns = ScenarioNamespaceGuard::get_permitted_namespaces("scenario", Some("legal_docs"), "legal");
    assert_eq!(scenario_ns, vec!["legal_docs"]);

    // Regular mode for standard legal user
    let legal_user_ns = ScenarioNamespaceGuard::get_permitted_namespaces("regular", None, "legal");
    assert!(legal_user_ns.contains(&"general".to_string()));
    assert!(legal_user_ns.contains(&"legal_docs".to_string()));
    assert!(!legal_user_ns.contains(&"financial_records".to_string()));

    // Admin user gets all namespaces
    let admin_ns = ScenarioNamespaceGuard::get_permitted_namespaces("regular", None, "admin");
    assert!(admin_ns.contains(&"all".to_string()));

    // Chunk permission checks
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(Some("legal_docs"), None, &legal_user_ns));
    assert!(!ScenarioNamespaceGuard::is_chunk_permitted(Some("financial_records"), None, &legal_user_ns));
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(Some("financial_records"), None, &admin_ns));
    
    // Test Option B prefix matching with filename geography check
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(Some("legal"), Some("legal_docs_india.pdf"), &vec!["legal_docs_india".to_string()]));
    assert!(!ScenarioNamespaceGuard::is_chunk_permitted(Some("legal"), Some("legal_docs_uk.pdf"), &vec!["legal_docs_india".to_string()]));
}

#[tokio::test]
async fn test_router_mode_validation() {
    let domain_reg = Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap());
    let style_reg = Arc::new(ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles").unwrap());
    let egress_guard = Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap());

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect(&db_url).await.unwrap();
    let store = token_compress_engine::rag::store::RagStore::new(pool);
    let retriever = token_compress_engine::rag::retriever::DocumentRetriever::new(store);

    // Scenario mode requires domain
    let err1 = router.route_and_execute("Scenario", None, None, None, "Question", "user1", "legal", None, &retriever).await;
    assert!(err1.is_err());
    assert!(err1.unwrap_err().contains("Domain selection is mandatory"));

    // Scenario mode cannot have style
    let err2 = router.route_and_execute("Scenario", Some("legal"), Some("balanced"), None, "Question", "user1", "legal", None, &retriever).await;
    assert!(err2.is_err());
    assert!(err2.unwrap_err().contains("Style selection is invalid"));

    // Regular mode requires style
    let err3 = router.route_and_execute("Regular", None, None, None, "Question", "user1", "legal", None, &retriever).await;
    assert!(err3.is_err());
    assert!(err3.unwrap_err().contains("Style selection is mandatory"));

    // Invalid mode
    let err4 = router.route_and_execute("UnknownMode", None, None, None, "Question", "user1", "legal", None, &retriever).await;
    assert!(err4.is_err());
    assert!(err4.unwrap_err().contains("Invalid Mode"));
}

#[tokio::test]
async fn test_empty_knowledge_not_found_response() {
    let domain_reg = Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap());
    let style_reg = Arc::new(ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles").unwrap());
    let egress_guard = Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap());

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect(&db_url).await.unwrap();
    let store = token_compress_engine::rag::store::RagStore::new(pool);
    let retriever = token_compress_engine::rag::retriever::DocumentRetriever::new(store);

    let resp = router.route_and_execute(
        "Regular",
        None,
        Some("balanced"),
        None,
        "Please search the web for latest stocks",
        "user1",
        "legal",
        None,
        &retriever,
    ).await.unwrap();

    assert!(resp.is_empty_knowledge);
    assert!(resp.output_text.contains("Not found in the internal knowledge base"));
    assert!(resp.warnings.iter().any(|w| w.contains("External web search request refused")));
}
