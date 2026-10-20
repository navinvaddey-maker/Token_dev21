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

    let style_reg =
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .expect("Failed to load styles.json and validate prompt files");
    assert!(style_reg.styles.len() >= 4);
    assert!(style_reg.get_style("balanced").is_some());
    assert!(style_reg.get_style("concise").is_some());
    assert!(style_reg.get_style("detailed").is_some());
    assert!(style_reg.get_style("aggressive").is_some());
}

#[test]
fn test_style_registry_fails_on_missing_prompt_file() {
    let res = ScenarioStyleRegistry::load_from_file(
        "config/scenario/styles.json",
        "non_existent_prompts_dir",
    );
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
    let scenario_ns =
        ScenarioNamespaceGuard::get_permitted_namespaces("scenario", Some("legal_docs"), "legal");
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
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(
        Some("legal_docs"),
        None,
        &legal_user_ns
    ));
    assert!(!ScenarioNamespaceGuard::is_chunk_permitted(
        Some("financial_records"),
        None,
        &legal_user_ns
    ));
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(
        Some("financial_records"),
        None,
        &admin_ns
    ));

    // Test Option B prefix matching with filename geography check
    assert!(ScenarioNamespaceGuard::is_chunk_permitted(
        Some("legal"),
        Some("legal_docs_india.pdf"),
        &vec!["legal_docs_india".to_string()]
    ));
    assert!(!ScenarioNamespaceGuard::is_chunk_permitted(
        Some("legal"),
        Some("legal_docs_uk.pdf"),
        &vec!["legal_docs_india".to_string()]
    ));
}

#[tokio::test]
async fn test_router_mode_validation() {
    let domain_reg =
        Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap());
    let style_reg = Arc::new(
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .unwrap(),
    );
    let egress_guard =
        Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap());

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .unwrap();
    let store = token_compress_engine::rag::store::RagStore::new(pool);
    let retriever = token_compress_engine::rag::retriever::DocumentRetriever::new(store);

    // Scenario mode requires domain
    let err1 = router
        .route_and_execute(
            "Scenario", None, None, None, "Question", "user1", "legal", None, &retriever, None,
        )
        .await;
    assert!(err1.is_err());
    assert!(err1.unwrap_err().contains("Domain selection is mandatory"));

    // Scenario mode cannot have style
    let err2 = router
        .route_and_execute(
            "Scenario",
            Some("legal"),
            Some("balanced"),
            None,
            "Question",
            "user1",
            "legal",
            None,
            &retriever,
            None,
        )
        .await;
    assert!(err2.is_err());
    assert!(err2.unwrap_err().contains("Style selection is invalid"));

    // Regular mode requires style
    let err3 = router
        .route_and_execute(
            "Regular", None, None, None, "Question", "user1", "legal", None, &retriever, None,
        )
        .await;
    assert!(err3.is_err());
    assert!(err3.unwrap_err().contains("Style selection is mandatory"));

    // Invalid mode
    let err4 = router
        .route_and_execute(
            "UnknownMode",
            None,
            None,
            None,
            "Question",
            "user1",
            "legal",
            None,
            &retriever,
            None,
        )
        .await;
    assert!(err4.is_err());
    assert!(err4.unwrap_err().contains("Invalid Mode"));
}

#[tokio::test]
async fn test_empty_knowledge_not_found_response() {
    let domain_reg =
        Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap());
    let style_reg = Arc::new(
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .unwrap(),
    );
    let egress_guard =
        Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap());

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = token_compress_engine::rag::store::RagStore::new(pool);
    let retriever = token_compress_engine::rag::retriever::DocumentRetriever::new(store);

    let resp = router
        .route_and_execute(
            "Regular",
            None,
            Some("balanced"),
            None,
            "Please search the web for latest stocks",
            "user1",
            "legal",
            None,
            &retriever,
            None,
        )
        .await
        .unwrap();

    assert!(resp.is_empty_knowledge);
    assert!(resp
        .output_text
        .contains("Not found in the internal knowledge base"));
    assert!(resp
        .warnings
        .iter()
        .any(|w| w.contains("External web search request refused")));
}

#[test]
fn test_generic_topical_filter_rejects_off_topic_chunks() {
    let question = "X (original owner) died intestate leaving 15 cents of land without registration/transfer to any son.";
    let criminal_chunk = "Offence IPC (old) BNS (new) Punishment (summary) Outraging modesty of a woman 354 74 1 to 5 years and fine Stalking 354D 78 Voluntarily causing hurt 323 Theft 379 Defamation 499";

    // Criminal table has no topical overlap with intestate land succession
    let is_relevant = ScenarioNamespaceGuard::is_chunk_content_relevant(question, criminal_chunk);
    assert!(
        !is_relevant,
        "Criminal offences chunk must be rejected for civil property succession query"
    );

    // Relevant chunk should pass
    let property_chunk = "Intestate succession: Where a male Hindu dies intestate, his property devolves according to Section 8 of the Hindu Succession Act upon Class I heirs.";
    let is_relevant2 = ScenarioNamespaceGuard::is_chunk_content_relevant(question, property_chunk);
    assert!(
        is_relevant2,
        "Property succession chunk must pass topical relevance filter"
    );
}

#[test]
fn test_domain_config_has_configurable_min_similarity() {
    let domain_reg = ScenarioDomainRegistry::load_from_file("config/scenario/domains.json")
        .expect("Failed to load domains.json");
    let legal = domain_reg
        .get_domain("legal")
        .expect("legal domain not found");
    assert_eq!(legal.min_similarity, Some(0.45));
}

#[test]
fn test_scenario_pipeline_components_generic() {
    use token_compress_engine::scenario::{ScenarioGapCheck, ScenarioParser, ScenarioToolRouter};

    let domain_reg =
        ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap();
    let legal = domain_reg.get_domain("legal").unwrap();

    let question = "X (original owner) died intestate leaving 15 cents of land without registration/transfer to any son.";

    // Dynamic parser
    let parsed = ScenarioParser::parse(question, legal);
    assert!(parsed.contains("focus tokens:"));
    assert!(parsed.contains("tokens"));

    // Dynamic gap check
    let (_ok, gaps) = ScenarioGapCheck::evaluate(question, legal);
    // jurisdiction was not stated in the question
    assert!(gaps.iter().any(|g| g.contains("jurisdiction")));

    // Structured tool dispatch
    let tools = ScenarioToolRouter::execute_tools(legal, question);
    assert_eq!(tools.len(), 2);
    assert!(tools[0].contains("target_terms:"));
    assert!(tools[0].contains("context_bytes:"));
}

#[tokio::test]
async fn test_scenario_aggressive_mode_strips_raw_evidence_and_preserves_contract() {
    use sqlx::sqlite::SqlitePoolOptions;
    use token_compress_engine::rag::embeddings::EmbeddingEngine;
    use token_compress_engine::rag::retriever::DocumentRetriever;
    use token_compress_engine::rag::store::RagStore;
    use token_compress_engine::rag::types::embedding_to_bytes;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, username, password_hash, email) VALUES ('u1', 'u1', 'x', 'u1@test.local')")
        .execute(&pool)
        .await
        .unwrap();

    let doc_content = "Under Section 43A of the Information Technology Act 2000, body corporate possessing sensitive personal data must maintain reasonable security practices. Failure to maintain security practices results in liability to pay compensation to affected persons.";
    sqlx::query(
        "INSERT INTO rag_documents (id, user_id, filename, domain, content_hash, status)
         VALUES ('d1', 'u1', 'it_act.txt', 'legal', 'h1', 'ready')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let engine = EmbeddingEngine::new();
    let blob = embedding_to_bytes(&engine.embed(doc_content));
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count)
         VALUES ('d1-c0', 'd1', 0, $1, $2, '{\"page_number\":1}', 30)",
    )
    .bind(doc_content)
    .bind(blob)
    .execute(&pool)
    .await
    .unwrap();

    let store = RagStore::new(pool.clone());
    let retriever = DocumentRetriever::new(store);

    let router = ScenarioModeRouter::new(
        Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap()),
        Arc::new(
            ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
                .unwrap(),
        ),
        Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap()),
        "prompts/styles".to_string(),
    );

    // Set up orchestrator with priors
    let priors = Arc::new(dashmap::DashMap::new());
    let config_handle = Arc::new(
        token_compress_engine::npae::aggressive::config::ConfigHandle::new(
            token_compress_engine::npae::aggressive::config::UnifiedConfig {
                domain_taxonomy: vec![],
                roles: vec![],
                constraints: vec![],
            },
        ),
    );
    let ory_engine = Arc::new(tokio::sync::Mutex::new(
        token_compress_engine::npae::ory::OryEngine::new(),
    ));
    let orchestrator =
        token_compress_engine::pipeline::orchestrator::PipelineOrchestrator::build_with_pool(
            priors,
            config_handle,
            ory_engine,
            Some(pool.clone()),
        );

    let resp = router
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            Some("india"),
            "What is Section 43A compensation liability for body corporate sensitive data?",
            "u1",
            "legal",
            Some("aggressive"),
            &retriever,
            Some(&orchestrator),
        )
        .await
        .unwrap();

    println!("\n================ ACTUAL OUTPUT TEXT ================\n{}\n==================================================\n", resp.output_text);

    // Invariants:
    // 1. Must preserve the System Prompt Contract
    assert!(resp.output_text.contains("=== SYSTEM PROMPT ==="));
    assert!(resp
        .output_text
        .contains("# 1. SYSTEM DIRECTIVE & SPECIALIZED ROLE"));
    assert!(resp
        .output_text
        .contains("# 6. TARGET RESPONSE SCHEMA CONTRACT"));

    // 2. Must contain User Prompt with Compressed Fact Matrix
    assert!(resp.output_text.contains("=== USER PROMPT ==="));
    assert!(resp.output_text.contains("# 2. USER SCENARIO OBJECTIVE"));
    assert!(resp
        .output_text
        .contains("# 3. VERIFIED GROUNDING CONTEXT (COMPRESSED FACT MATRIX)"));

    // 3. Raw chunk markup `### Key Evidence [` must be removed from User Prompt
    assert!(!resp.output_text.contains("### Key Evidence ["));

    // 4. Defect 5 Protocol: Executable Verification Mechanism must be explicitly specified in contract
    assert!(resp
        .output_text
        .contains("Executable Verification Mechanism (Defect 5 Protocol)"));
    assert!(resp.output_text.contains("Authoritative Source to Consult"));
    assert!(resp.output_text.contains("Remaining Uncertainty & Action"));
}

#[tokio::test]
async fn test_cheque_bounce_grounding_has_no_divorce_or_corpus_metadata() {
    use sqlx::sqlite::SqlitePoolOptions;
    use token_compress_engine::rag::embeddings::EmbeddingEngine;
    use token_compress_engine::rag::retriever::DocumentRetriever;
    use token_compress_engine::rag::store::RagStore;
    use token_compress_engine::rag::types::embedding_to_bytes;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, username, password_hash, email) VALUES ('u1', 'u1', 'x', 'u1@test.local')")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO rag_documents (id, user_id, filename, domain, content_hash, status)
         VALUES ('d_chq', 'u1', 'legal_docs_india.pdf', 'legal', 'h_chq', 'ready')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let engine = EmbeddingEngine::new();

    // Chunk 0: Corpus Index & Scaffolding
    let c0 = "legal_docs_india | v0.1 | Reference corpus for retrieval. Page 1 legal_docs_india Legal Sections Reference Corpus for RAG - India Version v0.1 (draft corpus) Compiled 4 October 2026 Scenarios Cheque bounce, property and land, civil, criminal, divorce and family, cross-cutting tables Chunks 79 section chunks plus 3 reference tables Purpose Source corpus for the scenario engine.";
    let blob0 = embedding_to_bytes(&engine.embed(c0));
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count)
         VALUES ('d_chq-c0', 'd_chq', 0, $1, $2, '{\"page_number\":1}', 50)",
    )
    .bind(c0)
    .bind(blob0)
    .execute(&pool)
    .await
    .unwrap();

    // Chunk 1: Substantive Cheque bounce details
    let c1 = "Offence arises when a cheque issued for a legally enforceable debt is returned unpaid for insufficient funds. Within 30 days of receiving bank dishonour information, payee must send written demand notice. Drawer has 15 days from receipt of demand notice to pay the amount. If drawer fails to pay, complaint must be filed within 1 month before the Judicial Magistrate.";
    let blob1 = embedding_to_bytes(&engine.embed(c1));
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count)
         VALUES ('d_chq-c1', 'd_chq', 1, $1, $2, '{\"page_number\":3}', 65)",
    )
    .bind(c1)
    .bind(blob1)
    .execute(&pool)
    .await
    .unwrap();

    // Chunk 2: Cross-cutting reference table with Divorce
    let c2 = "Part F - Cross-cutting reference XCT-01 - Master time-limit table. Cheque bounce demand notice period 30 days from dishonour memo. Divorce second motion mutual consent takes 6 to 18 months from first motion. Divorce no petition allowed within 1 year of marriage.";
    let blob2 = embedding_to_bytes(&engine.embed(c2));
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count)
         VALUES ('d_chq-c2', 'd_chq', 2, $1, $2, '{\"page_number\":21}', 55)",
    )
    .bind(c2)
    .bind(blob2)
    .execute(&pool)
    .await
    .unwrap();

    let store = RagStore::new(pool.clone());
    let retriever = DocumentRetriever::new(store);

    let domain_reg =
        Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap());
    let style_reg = Arc::new(
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .unwrap(),
    );
    let egress_guard =
        Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap());

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    let priors = Arc::new(dashmap::DashMap::new());
    let config_handle = Arc::new(
        token_compress_engine::npae::aggressive::config::ConfigHandle::new(
            token_compress_engine::npae::aggressive::config::UnifiedConfig {
                domain_taxonomy: vec![],
                roles: vec![],
                constraints: vec![],
            },
        ),
    );
    let ory_engine = Arc::new(tokio::sync::Mutex::new(
        token_compress_engine::npae::ory::OryEngine::new(),
    ));
    let orchestrator =
        token_compress_engine::pipeline::orchestrator::PipelineOrchestrator::build_with_pool(
            priors,
            config_handle,
            ory_engine,
            Some(pool.clone()),
        );

    let resp = router
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            Some("india"),
            "I need cheque bounce case details",
            "u1",
            "legal",
            Some("aggressive"),
            &retriever,
            Some(&orchestrator),
        )
        .await
        .unwrap();

    println!(
        "\n=== CHEQUE BOUNCE TEST OUTPUT ===\n{}\n=================================\n",
        resp.output_text
    );

    // Assert fact matrix is present
    assert!(resp
        .output_text
        .contains("# 3. VERIFIED GROUNDING CONTEXT (COMPRESSED FACT MATRIX)"));

    // Assert NO divorce leakage into cheque bounce query
    assert!(
        !resp.output_text.to_lowercase().contains("divorce"),
        "Grounding must not leak divorce into cheque bounce"
    );

    // Assert NO draft corpus metadata leakage
    assert!(
        !resp.output_text.to_lowercase().contains("draft corpus"),
        "Grounding must not leak corpus metadata"
    );
    assert!(
        !resp
            .output_text
            .to_lowercase()
            .contains("79 section chunks"),
        "Grounding must not leak index chunks"
    );

    // Assert substantive cheque bounce facts ARE present
    assert!(resp.output_text.to_lowercase().contains("cheque"));
    assert!(
        resp.output_text.to_lowercase().contains("dishonour")
            || resp.output_text.to_lowercase().contains("notice")
    );
}
