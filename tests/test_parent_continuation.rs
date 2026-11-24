//! Integration tests for Stateful Parent -> Continuation (Phases 1-3).
//!
//! Covers Path A (`DeltaFollowUp`), Path B (`EditContinuation`), `SL-2` domain hysteresis,
//! `SL-3` monotonic `[F#]` preservation, `RepairContext` repair synthesis, Balanced mode
//! continuation, stateless `/api/demo/compress`, and cross-tenant isolation.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::uninlined_format_args
)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use dashmap::DashMap;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tower::util::ServiceExt;

use learning_engine::TenantLearningPool;
use token_compress_engine::npae::aggressive::config::{ConfigHandle, ConfigLoader};
use token_compress_engine::npae::aggressive::engine::{AggressiveEngine, ContinuationInput};
use token_compress_engine::npae::aggressive::structurer::HttpStructurer;
use token_compress_engine::npae::compression::pipeline::run_parallel_pipeline;
use token_compress_engine::npae::continuation::{
    ContinuationKind, ContinuationPath, PromptStateLedger,
};
use token_compress_engine::npae::ory::OryEngine;
use token_compress_engine::npae::schema::types::{
    NpaeConfig, RenderStyle, RepairContext, StructuredPromptResponse,
};
use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
use token_compress_engine::types::{
    ChunkMetadata, EnrichmentContext, RagChunk, ReconstructedInput,
};
use token_compress_engine::AppState;

async fn setup_test_router() -> (axum::Router, AppState, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite memory");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let npae_config = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let schema_priors = Arc::new(DashMap::new());
    let pipeline = Arc::new(PipelineOrchestrator::build_with_pool(
        schema_priors,
        npae_config.clone(),
        ory_engine.clone(),
        Some(pool.clone()),
    ));
    let embedding_engine = Arc::new(token_compress_engine::rag::embeddings::EmbeddingEngine::new());
    let domain_registry = Arc::new(
        token_compress_engine::scenario::ScenarioDomainRegistry::load_from_file(
            "config/scenario/domains.json",
        )
        .expect("domains.json"),
    );
    let style_registry = Arc::new(
        token_compress_engine::scenario::ScenarioStyleRegistry::load_from_file(
            "config/scenario/styles.json",
            "prompts/styles",
        )
        .expect("styles.json"),
    );
    let egress_guard = Arc::new(
        token_compress_engine::scenario::ScenarioEgressGuard::load_from_file(
            "config/scenario/egress.json",
        )
        .expect("egress.json"),
    );
    let scenario_router = Arc::new(token_compress_engine::scenario::ScenarioModeRouter::new(
        domain_registry,
        style_registry,
        egress_guard,
        "prompts/styles".to_string(),
    ));

    std::env::set_var(
        "JWT_SECRET",
        "test_parent_continuation_secret_key_32_bytes_min",
    );

    let state = AppState {
        pool: pool.clone(),
        learning_pool: Arc::new(TenantLearningPool::new(pool.clone(), 64)),
        pipeline,
        npae_config,
        ory_engine,
        rag_store: Arc::new(token_compress_engine::rag::store::RagStore::new(
            pool.clone(),
        )),
        sessions: Arc::new(DashMap::new()),
        prompt_ledgers: Arc::new(DashMap::new()),
        embedding_engine,
        scenario_router,
        pdf_ingest_sem: Arc::new(Semaphore::new(4)),
        aggressive_sem: Arc::new(Semaphore::new(16)),
        compress_sem: Arc::new(Semaphore::new(16)),
    };

    let router = token_compress_engine::api::router(state.clone());
    (router, state, pool)
}

async fn seed_user_jwt(pool: &sqlx::SqlitePool, username: &str, tenant_id: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let pw_hash = bcrypt::hash("ContTestPw123!", bcrypt::DEFAULT_COST).expect("hash");
    token_compress_engine::data::Repository::create_user_scoped(
        pool,
        &user_id,
        username,
        &pw_hash,
        &format!("{username}@example.com"),
        "Developer",
        "user",
        tenant_id,
    )
    .await
    .expect("seed user");
    token_compress_engine::domain::create_jwt_for_user(&user_id, tenant_id, "user")
        .expect("issue jwt")
}

#[tokio::test]
async fn test_path_a_delta_followup_inherits_python_locks_software_domain_and_phase_1() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    // Turn 1: software backend build prompt with Python inclusion & multi-phase plan
    let turn1_raw =
        "Build a step-by-step backend API architecture in Python with FastAPI, must use Python, and include JWT authentication.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let recon1 = ReconstructedInput::default();
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &recon1,
        None,
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");

    let cont1 = resp1.continuation.as_ref().expect("turn 1 continuation");
    assert_eq!(cont1.path, ContinuationPath::Initial);
    assert_eq!(cont1.turn_number, 1);
    assert_eq!(resp1.structured_prompt.context.domain, "software");

    // Turn 2 (Path A): short delta mentioning "$100/mo" (must NOT hijack domain to finance),
    // "no Redis", and "phase 1".
    let turn2_raw = "no Redis, under $100/mo, phase 1";
    let repr2 = run_parallel_pipeline(turn2_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_raw.to_string(),
    };
    let recon2 = ReconstructedInput::default();
    let cont_input = ContinuationInput {
        parent_ledger: None,
        parent_raw_prompt: Some(turn1_raw.to_string()),
        explicit_parent_id: Some(cont1.ledger_id.clone()),
        revoked_constraints: Vec::new(),
        active_phase: None,
        resolved_slots: BTreeMap::new(),
        repair_context: None,
    };

    let resp2 = AggressiveEngine::run_with_continuation(
        turn2_raw,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &recon2,
        None,
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .expect("turn 2");

    let cont2 = resp2.continuation.as_ref().expect("turn 2 continuation");
    assert_eq!(cont2.path, ContinuationPath::DeltaFollowUp);
    assert_eq!(cont2.turn_number, 2);
    assert_eq!(cont2.active_phase, Some(1));

    // SL-2 hysteresis: "$100/mo" must NOT flip locked "software" domain into "finance"
    assert_eq!(resp2.structured_prompt.context.domain, "software");

    // Inherited Python + new constraints ("no Redis", "under $100/mo")
    let prompt_low = resp2.optimized_prompt.to_lowercase();
    assert!(
        prompt_low.contains("python"),
        "Expected inherited Python in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        prompt_low.contains("redis") && prompt_low.contains("[avoid]"),
        "Expected Redis exclusion in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        !resp2.optimized_prompt.contains("- [STRICT] Exclude Redis"),
        "Exclusions must never appear under [STRICT]:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        prompt_low.contains("$100/mo"),
        "Expected under $100/mo bound in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );

    // Phase 1 active and deferred phases collapsed in execution_phases
    assert!(
        resp2.structured_prompt.execution_phases.len() <= 1,
        "Expected deferred phases collapsed when active_phase=1, got {:?}",
        resp2.structured_prompt.execution_phases
    );

    // Delta prompt populated
    let delta_prompt = cont2.delta_prompt.as_deref().expect("delta_prompt present");
    assert!(delta_prompt.contains("# TURN DELTA"));
    assert!(delta_prompt.contains("[CONSTRAINTS_PATCH]"));
    assert!(delta_prompt.contains("[TASK]"));
}

#[tokio::test]
async fn test_path_b_edit_continuation_supersedes_and_scrubs_mongodb_for_postgresql() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let turn1_raw =
        "Build a REST API in Rust using MongoDB. Include JWT authentication. Deploy with Docker.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");

    let cont1 = resp1.continuation.as_ref().expect("turn 1 continuation");

    // Turn 2 (Path B): retyped full prompt replacing MongoDB with PostgreSQL
    let turn2_raw = "Build a REST API in Rust using PostgreSQL instead of MongoDB. Include JWT authentication. Deploy with Docker.";
    let repr2 = run_parallel_pipeline(turn2_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_raw.to_string(),
    };
    let cont_input = ContinuationInput {
        parent_ledger: None,
        parent_raw_prompt: Some(turn1_raw.to_string()),
        explicit_parent_id: Some(cont1.ledger_id.clone()),
        ..ContinuationInput::default()
    };

    let resp2 = AggressiveEngine::run_with_continuation(
        turn2_raw,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .expect("turn 2");

    let cont2 = resp2.continuation.as_ref().expect("turn 2 continuation");
    assert_eq!(cont2.path, ContinuationPath::EditContinuation);
    assert!(
        cont2
            .superseded_constraints
            .iter()
            .any(|s| s.eq_ignore_ascii_case("MongoDB") || s.to_lowercase().contains("mongodb")),
        "Expected MongoDB in superseded_constraints, got: {:?}",
        cont2.superseded_constraints
    );
    assert!(
        !resp2.optimized_prompt.to_lowercase().contains("mongodb"),
        "MongoDB must be scrubbed from optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        resp2.optimized_prompt.contains("- [STRICT] PostgreSQL"),
        "Expected PostgreSQL as [STRICT] constraint in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        resp2
            .hallucination_report
            .as_ref()
            .is_some_and(|r| r.passed),
        "Expected hallucination_report.passed == true, got: {:?}",
        resp2.hallucination_report
    );
}

#[tokio::test]
async fn test_sl3_monotonic_fact_ids_preserved_across_rag_turns() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let turn1_raw = "Summarize our production service SLA and maximum latency requirements.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let enrich1 = EnrichmentContext {
        rag_chunks: Some(vec![RagChunk {
            domain_tag: "software".to_string(),
            content: "Production service uptime SLA is 99.95% and maximum latency is 120ms."
                .to_string(),
            metadata: Some(ChunkMetadata {
                source_file: "sla.pdf".to_string(),
                page_number: Some(1),
            }),
        }]),
        ..Default::default()
    };
    let cfg = NpaeConfig {
        render_style: Some(RenderStyle::Harness),
        ..Default::default()
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &cfg,
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        Some(&enrich1),
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");

    assert_eq!(resp1.grounding_facts.len(), 1);
    let f1_id = resp1.grounding_facts[0].id.clone();
    let f1_text = resp1.grounding_facts[0].text.clone();
    assert_eq!(f1_id, "F1");

    let cont1 = resp1.continuation.as_ref().expect("cont1");

    // Turn 2: repeat Turn 1 fact (with extra whitespace) alongside a brand-new RAG fact.
    let turn2_raw = "Also include the PostgreSQL database version requirement.";
    let repr2 = run_parallel_pipeline(turn2_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_raw.to_string(),
    };
    let enrich2 = EnrichmentContext {
        rag_chunks: Some(vec![
            RagChunk {
                domain_tag: "software".to_string(),
                content: "Database target engine is PostgreSQL 16.".to_string(),
                metadata: Some(ChunkMetadata {
                    source_file: "db_spec.pdf".to_string(),
                    page_number: Some(2),
                }),
            },
            RagChunk {
                domain_tag: "software".to_string(),
                content: "Production service uptime SLA   is 99.95% and maximum latency is 120ms."
                    .to_string(),
                metadata: Some(ChunkMetadata {
                    source_file: "sla.pdf".to_string(),
                    page_number: Some(1),
                }),
            },
        ]),
        ..Default::default()
    };

    let cont_input = ContinuationInput {
        parent_ledger: None,
        parent_raw_prompt: Some(turn1_raw.to_string()),
        explicit_parent_id: Some(cont1.ledger_id.clone()),
        ..ContinuationInput::default()
    };

    let resp2 = AggressiveEngine::run_with_continuation(
        turn2_raw,
        &repr2,
        &cfg,
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        Some(&enrich2),
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .expect("turn 2");

    assert_eq!(resp2.grounding_facts.len(), 2);
    assert_eq!(resp2.grounding_facts[0].id, "F1");
    assert_eq!(resp2.grounding_facts[0].text, f1_text);
    assert_eq!(resp2.grounding_facts[1].id, "F2");
    assert!(resp2.grounding_facts[1].text.contains("PostgreSQL 16"));

    let delta_prompt = resp2
        .continuation
        .as_ref()
        .and_then(|c| c.delta_prompt.as_deref())
        .expect("delta_prompt");
    assert!(
        delta_prompt.contains("[NEW_FACTS]") && delta_prompt.contains("[F2]"),
        "Expected only newly minted [F2] in [NEW_FACTS]:\n{delta_prompt}"
    );
    assert!(
        !delta_prompt.contains("[F1]"),
        "Duplicate Turn 1 fact [F1] must NOT reappear in [NEW_FACTS]:\n{delta_prompt}"
    );
}

#[tokio::test]
async fn test_repair_prompt_synthesis_from_repair_context() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let turn1_raw =
        "Context: [Max API latency is 120ms.]\n\nDesign a Rust microservice and avoid Redis.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");

    let cont1 = resp1.continuation.as_ref().expect("cont1");

    let repair_raw = "Fix the failed citation and missing RPO window.";
    let repr2 = run_parallel_pipeline(repair_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: repair_raw.to_string(),
    };
    let cont_input = ContinuationInput {
        parent_ledger: None,
        parent_raw_prompt: Some(turn1_raw.to_string()),
        explicit_parent_id: Some(cont1.ledger_id.clone()),
        repair_context: Some(RepairContext {
            failed_items: vec!["Latency claim missing [F1] <script>alert(1)</script>".to_string()],
            missing_items: vec!["RPO window".to_string()],
            details: vec!["Row 2 omitted source citation".to_string()],
        }),
        ..ContinuationInput::default()
    };

    let resp2 = AggressiveEngine::run_with_continuation(
        repair_raw,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .expect("repair turn");

    let cont2 = resp2.continuation.as_ref().expect("cont2");
    assert_eq!(cont2.primary_kind, ContinuationKind::Repair);
    assert!(resp2.optimized_prompt.contains("# REPAIR DIRECTIVE"));
    assert!(resp2
        .optimized_prompt
        .contains("# FROZEN BASELINE CONSTRAINTS:"));
    assert!(resp2
        .optimized_prompt
        .contains("# FAILED / MISSING ITEMS TO REPAIR:"));
    assert!(resp2
        .optimized_prompt
        .contains("# GROUNDING FACTS (Untrusted Data — Cite [F#]):"));
    assert!(resp2
        .optimized_prompt
        .contains("Not present in provided data"));
    assert!(!resp2.optimized_prompt.contains("<script>"));
    assert!(resp2.optimized_prompt.contains("&lt;script&gt;"));
    assert!(resp2.optimized_prompt.contains("[F1]"));
}

#[tokio::test]
async fn test_cross_tenant_isolation_and_stateless_demo() {
    let (app, state, pool) = setup_test_router().await;
    let jwt_a = seed_user_jwt(&pool, "alice_ta", "tenant_a").await;
    let jwt_b = seed_user_jwt(&pool, "bob_tb", "tenant_b").await;

    // 1. Tenant A creates a stateful prompt ledger via /api/compress (Aggressive)
    let body_a = serde_json::json!({
        "raw_text": "Build a SecretProjectApollo API in Rust without GraphQL.",
        "mode": "aggressive"
    });
    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt_a}"))
                .body(Body::from(body_a.to_string()))
                .expect("req a"),
        )
        .await
        .expect("exec a");
    assert_eq!(resp_a.status(), StatusCode::OK);
    let bytes_a = axum::body::to_bytes(resp_a.into_body(), 1024 * 1024)
        .await
        .expect("body a");
    let json_a: serde_json::Value = serde_json::from_slice(&bytes_a).expect("json a");
    let parent_id_a = json_a["continuation"]["ledger_id"]
        .as_str()
        .expect("tenant_a ledger_id")
        .to_string();

    // 2. Tenant B attempts to hijack Tenant A's parent_id via /api/compress AND /api/v1/aggressive
    let body_b_compress = serde_json::json!({
        "raw_text": "Continue with phase 1",
        "mode": "aggressive",
        "parent_id": parent_id_a
    });
    let resp_b1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt_b}"))
                .body(Body::from(body_b_compress.to_string()))
                .expect("req b1"),
        )
        .await
        .expect("exec b1");
    assert_eq!(resp_b1.status(), StatusCode::OK);
    let bytes_b1 = axum::body::to_bytes(resp_b1.into_body(), 1024 * 1024)
        .await
        .expect("body b1");
    let json_b1: serde_json::Value = serde_json::from_slice(&bytes_b1).expect("json b1");
    assert_eq!(json_b1["continuation"]["turn_number"], 1);
    assert_eq!(json_b1["continuation"]["path"], "initial");
    assert_ne!(json_b1["continuation"]["ledger_id"], parent_id_a);
    let opt_b1 = json_b1["optimized_prompt"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    assert!(!opt_b1.contains("secretprojectapollo"));
    assert!(!opt_b1.contains("graphql"));

    let body_b_agg = serde_json::json!({
        "prompt": "Continue with phase 1",
        "parent_id": parent_id_a
    });
    let resp_b2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/aggressive")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt_b}"))
                .body(Body::from(body_b_agg.to_string()))
                .expect("req b2"),
        )
        .await
        .expect("exec b2");
    assert_eq!(resp_b2.status(), StatusCode::OK);
    let agg_bytes_b2 = axum::body::to_bytes(resp_b2.into_body(), 1024 * 1024)
        .await
        .expect("body b2");
    let resp_struct_b2: StructuredPromptResponse =
        serde_json::from_slice(&agg_bytes_b2).expect("parse StructuredPromptResponse");
    let cont_b2 = resp_struct_b2.continuation.expect("cont_b2");
    assert_eq!(cont_b2.turn_number, 1);
    assert_eq!(cont_b2.path, ContinuationPath::Initial);
    assert_ne!(cont_b2.ledger_id, parent_id_a);
    assert!(!resp_struct_b2
        .optimized_prompt
        .to_lowercase()
        .contains("secretprojectapollo"));

    // 3. Balanced mode Turn 1 -> Turn 2 continuation via /api/compress
    let bal_t1 = serde_json::json!({
        "raw_text": "Build a REST API in Rust using MongoDB and include JWT authentication.",
        "mode": "balanced"
    });
    let resp_bal1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt_a}"))
                .body(Body::from(bal_t1.to_string()))
                .expect("bal1"),
        )
        .await
        .expect("exec bal1");
    assert_eq!(resp_bal1.status(), StatusCode::OK);
    let bal1_bytes = axum::body::to_bytes(resp_bal1.into_body(), 1024 * 1024)
        .await
        .expect("bal1 body");
    let bal1_json: serde_json::Value = serde_json::from_slice(&bal1_bytes).expect("bal1 json");
    let bal_pid = bal1_json["continuation"]["ledger_id"]
        .as_str()
        .expect("balanced ledger_id")
        .to_string();

    let bal_t2 = serde_json::json!({
        "raw_text": "Use PostgreSQL instead of MongoDB.",
        "mode": "balanced",
        "parent_id": bal_pid
    });
    let resp_bal2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt_a}"))
                .body(Body::from(bal_t2.to_string()))
                .expect("bal2"),
        )
        .await
        .expect("exec bal2");
    assert_eq!(resp_bal2.status(), StatusCode::OK);
    let bal2_bytes = axum::body::to_bytes(resp_bal2.into_body(), 1024 * 1024)
        .await
        .expect("bal2 body");
    let bal2_json: serde_json::Value = serde_json::from_slice(&bal2_bytes).expect("bal2 json");
    assert_eq!(bal2_json["continuation"]["turn_number"], 2);
    assert_eq!(bal2_json["continuation"]["path"], "delta_follow_up");
    assert!(bal2_json["continuation"]["delta_prompt"].is_string());

    // 4. /api/demo/compress must remain stateless (never write to state.prompt_ledgers)
    let count_before_demo = state.prompt_ledgers.len();
    let demo_body = serde_json::json!({
        "raw_text": "Build a quick Rust CLI tool without Redis.",
        "mode": "aggressive",
        "parent_id": parent_id_a
    });
    let demo_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/demo/compress")
                .header("content-type", "application/json")
                .body(Body::from(demo_body.to_string()))
                .expect("demo req"),
        )
        .await
        .expect("exec demo");
    assert_eq!(demo_resp.status(), StatusCode::OK);
    assert_eq!(
        state.prompt_ledgers.len(),
        count_before_demo,
        "/api/demo/compress must never mutate state.prompt_ledgers"
    );
}

#[tokio::test]
async fn test_c1_id_revocation_and_path_a_non_constraint_instruction_retention() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    // Turn 1: initial prompt with Redis exclusion and MongoDB inclusion
    let turn1_raw = "Build a REST API in Rust without Redis and include MongoDB.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");

    let cont1 = resp1.continuation.as_ref().expect("cont1");
    let c1 = cont1
        .active_constraints
        .iter()
        .find(|c| c.id == "C1")
        .expect("C1 present");
    let c1_text_low = c1.text.to_lowercase();

    // Turn 2: revoke "C1" by ID and send Path A non-constraint additive request
    let turn2_raw = "Add OpenAPI docs and rate limiting";
    let repr2 = run_parallel_pipeline(turn2_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_raw.to_string(),
    };
    let cont_input = ContinuationInput {
        parent_ledger: None,
        parent_raw_prompt: Some(turn1_raw.to_string()),
        explicit_parent_id: Some(cont1.ledger_id.clone()),
        revoked_constraints: vec!["C1".to_string()],
        ..ContinuationInput::default()
    };

    let resp2 = AggressiveEngine::run_with_continuation(
        turn2_raw,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(cont_input),
    )
    .await
    .expect("turn 2");

    let cont2 = resp2.continuation.as_ref().expect("cont2");
    assert_eq!(cont2.path, ContinuationPath::DeltaFollowUp);
    assert!(
        !cont2.active_constraints.iter().any(|c| c.id == "C1"),
        "C1 must no longer be active after ID revocation: {:?}",
        cont2.active_constraints
    );
    if c1_text_low.contains("mongodb") {
        assert!(
            !resp2.optimized_prompt.to_lowercase().contains("mongodb"),
            "Revoked C1 (MongoDB) must be scrubbed:\n{}",
            resp2.optimized_prompt
        );
    }
    let prompt_low = resp2.optimized_prompt.to_lowercase();
    assert!(
        prompt_low.contains("openapi docs") && prompt_low.contains("rate limiting"),
        "Path A non-constraint instruction must be retained in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
}

#[tokio::test]
async fn test_path_b_exclusion_override_no_strict_leak() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    // Path B exclusion override: "no dairy" -> "no gluten" (must emit zero [STRICT] gluten)
    let turn1_diet = "Build a weekly meal plan with no dairy and high protein.";
    let repr1 = run_parallel_pipeline(turn1_diet).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_diet.to_string(),
    };
    let mut diet_ledger: Option<PromptStateLedger> = None;
    let resp_d1 = AggressiveEngine::run_with_continuation(
        turn1_diet,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        None,
        &mut diet_ledger,
        None,
    )
    .await
    .expect("diet turn 1");

    let cont_d1 = resp_d1.continuation.as_ref().expect("diet cont1");
    let turn2_diet = "Build a weekly meal plan with no gluten and high protein.";
    let repr2 = run_parallel_pipeline(turn2_diet).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_diet.to_string(),
    };
    let resp_d2 = AggressiveEngine::run_with_continuation(
        turn2_diet,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut diet_ledger,
        Some(ContinuationInput {
            parent_raw_prompt: Some(turn1_diet.to_string()),
            explicit_parent_id: Some(cont_d1.ledger_id.clone()),
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("diet turn 2");

    assert!(
        !resp_d2.optimized_prompt.contains("- [STRICT] gluten")
            && !resp_d2.optimized_prompt.contains("- [STRICT] no gluten"),
        "Overridden exclusion must NEVER appear under [STRICT]:\n{}",
        resp_d2.optimized_prompt
    );
    assert!(
        resp_d2.optimized_prompt.to_lowercase().contains("[avoid]")
            && resp_d2.optimized_prompt.to_lowercase().contains("gluten"),
        "Expected gluten under [AVOID] in optimized_prompt:\n{}",
        resp_d2.optimized_prompt
    );
}

#[tokio::test]
async fn test_harness_continuation_without_new_rag_keeps_turn1_f1_in_facts() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let h_turn1 = "Summarize our Q3 financial revenue.";
    let h_repr1 = run_parallel_pipeline(h_turn1).expect("h_repr1");
    let h_http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: h_turn1.to_string(),
    };
    let h_enrich1 = EnrichmentContext {
        rag_chunks: Some(vec![RagChunk {
            domain_tag: "finance".to_string(),
            content: "Q3 revenue was $4.2M with 18% YoY growth.".to_string(),
            metadata: Some(ChunkMetadata {
                source_file: "q3_report.pdf".to_string(),
                page_number: Some(1),
            }),
        }]),
        ..Default::default()
    };
    let harness_cfg = NpaeConfig {
        render_style: Some(RenderStyle::Harness),
        ..Default::default()
    };
    let mut h_ledger: Option<PromptStateLedger> = None;
    let h_resp1 = AggressiveEngine::run_with_continuation(
        h_turn1,
        &h_repr1,
        &harness_cfg,
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &h_http1,
        &ReconstructedInput::default(),
        Some(&h_enrich1),
        &mut h_ledger,
        None,
    )
    .await
    .expect("harness turn 1");
    let h_cont1 = h_resp1.continuation.as_ref().expect("h_cont1");

    // Turn 2: no new RAG supplied
    let h_turn2 = "Focus on the YoY growth rate.";
    let h_repr2 = run_parallel_pipeline(h_turn2).expect("h_repr2");
    let h_http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: h_turn2.to_string(),
    };
    let h_resp2 = AggressiveEngine::run_with_continuation(
        h_turn2,
        &h_repr2,
        &harness_cfg,
        config_handle,
        ory_engine,
        &h_http2,
        &ReconstructedInput::default(),
        None,
        &mut h_ledger,
        Some(ContinuationInput {
            parent_raw_prompt: Some(h_turn1.to_string()),
            explicit_parent_id: Some(h_cont1.ledger_id.clone()),
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("harness turn 2");

    assert!(
        h_resp2.optimized_prompt.contains("[F1]")
            && h_resp2.optimized_prompt.contains("Q3 revenue was $4.2M"),
        "Harness continuation without new RAG must keep Turn 1 [F1] in <facts>:\n{}",
        h_resp2.optimized_prompt
    );
    assert_eq!(h_resp2.grounding_facts.len(), 1);
    assert_eq!(h_resp2.grounding_facts[0].id, "F1");
}

#[tokio::test]
async fn test_decimal_version_override_and_word_boundary_safety() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    // Turn 1: includes Python 3.10 and MongoDB
    let turn1_raw = "Build a backend API using Python 3.10 and MongoDB.";
    let repr1 = run_parallel_pipeline(turn1_raw).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn1_raw.to_string(),
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;

    let resp1 = AggressiveEngine::run_with_continuation(
        turn1_raw,
        &repr1,
        &NpaeConfig::default(),
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");
    let cont1 = resp1.continuation.as_ref().expect("cont1");

    // Turn 2: decimal version override ("Python 3.12 instead of Python 3.10") AND revoke "Go"
    // ("Go" must NOT match/scrub "MongoDB"!)
    let turn2_raw = "Use Python 3.12 instead of Python 3.10.";
    let repr2 = run_parallel_pipeline(turn2_raw).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: turn2_raw.to_string(),
    };
    let resp2 = AggressiveEngine::run_with_continuation(
        turn2_raw,
        &repr2,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(ContinuationInput {
            parent_raw_prompt: Some(turn1_raw.to_string()),
            explicit_parent_id: Some(cont1.ledger_id.clone()),
            revoked_constraints: vec!["Go".to_string()],
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("turn 2");

    let cont2 = resp2.continuation.as_ref().expect("cont2");
    assert!(
        cont2
            .active_constraints
            .iter()
            .any(|c| c.text.contains("Python 3.12")),
        "Decimal version Python 3.12 must stay intact: {:?}",
        cont2.active_constraints
    );
    assert!(
        !resp2.optimized_prompt.contains("Python 3.10"),
        "Superseded Python 3.10 must be scrubbed:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        resp2.optimized_prompt.contains("Python 3.12"),
        "Replacement Python 3.12 must appear in optimized_prompt:\n{}",
        resp2.optimized_prompt
    );
    assert!(
        resp2.optimized_prompt.to_lowercase().contains("mongodb"),
        "Revoking \"Go\" must NOT match or drop \"MongoDB\":\n{}",
        resp2.optimized_prompt
    );
}

#[tokio::test]
async fn test_standalone_turn1_repair_renders_repair_directive() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let mut standalone_ledger: Option<PromptStateLedger> = None;
    let rep_raw = "Context: [Max latency is 80ms.]\n\nBuild an order service.";
    let rep_repr = run_parallel_pipeline(rep_raw).expect("rep_repr");
    let rep_http = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: rep_raw.to_string(),
    };
    let rep_resp = AggressiveEngine::run_with_continuation(
        rep_raw,
        &rep_repr,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &rep_http,
        &ReconstructedInput::default(),
        None,
        &mut standalone_ledger,
        Some(ContinuationInput {
            repair_context: Some(RepairContext {
                failed_items: vec!["Latency missing [F1]".to_string()],
                missing_items: vec!["SLA target".to_string()],
                details: Vec::new(),
            }),
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("standalone turn 1 repair");

    assert!(rep_resp.optimized_prompt.contains("# REPAIR DIRECTIVE"));
    assert!(rep_resp.optimized_prompt.contains("[F1]"));
}

#[tokio::test]
async fn test_repair_directive_with_grounding_score_percentage_no_false_uncertain() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let mut ledger_opt: Option<PromptStateLedger> = None;
    let rep_raw = "Context: [Q3 revenue was $4.2M.]\n\nSummarize Q3 revenue.";
    let rep_repr = run_parallel_pipeline(rep_raw).expect("rep_repr");
    let rep_http = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: rep_raw.to_string(),
    };
    let rep_resp = AggressiveEngine::run_with_continuation(
        rep_raw,
        &rep_repr,
        &NpaeConfig::default(),
        config_handle,
        ory_engine,
        &rep_http,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(ContinuationInput {
            repair_context: Some(RepairContext {
                failed_items: vec!["Revenue claim missing [F1]".to_string()],
                missing_items: vec!["Operating margin".to_string()],
                details: vec!["Grounding score: 50% (Unsupported: 1)".to_string()],
            }),
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("repair with percentage details");

    assert!(
        rep_resp
            .hallucination_report
            .as_ref()
            .is_some_and(|r| r.passed),
        "Diagnostic 50% in RepairContext must not fail HallucinationGuard: {:?}",
        rep_resp.hallucination_report
    );
    assert!(
        !rep_resp.optimized_prompt.starts_with("[UNCERTAIN]"),
        "Repair directive must not start with [UNCERTAIN]:\n{}",
        rep_resp.optimized_prompt
    );
    assert!(rep_resp.optimized_prompt.starts_with("# REPAIR DIRECTIVE"));
}

#[tokio::test]
async fn test_resolved_slots_turn2_never_collides_with_f1_rag_fact() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));

    let t1 = "Summarize our production SLA requirements.";
    let repr1 = run_parallel_pipeline(t1).expect("repr1");
    let http1 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: t1.to_string(),
    };
    let enrich1 = EnrichmentContext {
        rag_chunks: Some(vec![RagChunk {
            domain_tag: "software".to_string(),
            content: "Production SLA uptime target is 99.95%.".to_string(),
            metadata: Some(ChunkMetadata {
                source_file: "sla.pdf".to_string(),
                page_number: Some(1),
            }),
        }]),
        ..Default::default()
    };
    let cfg = NpaeConfig {
        render_style: Some(RenderStyle::Harness),
        ..Default::default()
    };
    let mut ledger_opt: Option<PromptStateLedger> = None;
    let resp1 = AggressiveEngine::run_with_continuation(
        t1,
        &repr1,
        &cfg,
        Arc::clone(&config_handle),
        Arc::clone(&ory_engine),
        &http1,
        &ReconstructedInput::default(),
        Some(&enrich1),
        &mut ledger_opt,
        None,
    )
    .await
    .expect("turn 1");
    let cont1 = resp1.continuation.as_ref().expect("cont1");

    let mut slots = BTreeMap::new();
    slots.insert(
        "deployment target".to_string(),
        "AWS ECS Fargate".to_string(),
    );
    let t2 = "Focus on deployment checklist.";
    let repr2 = run_parallel_pipeline(t2).expect("repr2");
    let http2 = HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: t2.to_string(),
    };
    let resp2 = AggressiveEngine::run_with_continuation(
        t2,
        &repr2,
        &cfg,
        config_handle,
        ory_engine,
        &http2,
        &ReconstructedInput::default(),
        None,
        &mut ledger_opt,
        Some(ContinuationInput {
            parent_raw_prompt: Some(t1.to_string()),
            explicit_parent_id: Some(cont1.ledger_id.clone()),
            resolved_slots: slots,
            ..ContinuationInput::default()
        }),
    )
    .await
    .expect("turn 2");

    let ids: Vec<&str> = resp2
        .grounding_facts
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    let unique_ids: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique_ids.len(), "Duplicate [F#] IDs: {:?}", ids);
    assert_eq!(resp2.grounding_facts[0].id, "F1");
    assert!(resp2.grounding_facts[0].text.contains("99.95%"));
}

#[tokio::test]
async fn test_balanced_mode_with_repair_context_routes_to_repair_directive() {
    let (app, _state, pool) = setup_test_router().await;
    let jwt = seed_user_jwt(&pool, "repair_bal_user", "tenant_rep").await;

    let t1_body = serde_json::json!({
        "raw_text": "Design a Rust REST API without Redis.",
        "mode": "balanced"
    });
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt}"))
                .body(Body::from(t1_body.to_string()))
                .expect("req1"),
        )
        .await
        .expect("exec1");
    assert_eq!(resp1.status(), StatusCode::OK);
    let b1 = axum::body::to_bytes(resp1.into_body(), 1024 * 1024)
        .await
        .expect("b1");
    let j1: serde_json::Value = serde_json::from_slice(&b1).expect("j1");
    let pid = j1["continuation"]["ledger_id"]
        .as_str()
        .expect("ledger_id")
        .to_string();

    let t2_body = serde_json::json!({
        "raw_text": "Fix the failed citation.",
        "mode": "balanced",
        "parent_id": pid,
        "repair_context": {
            "failed_items": ["Missing [F1]"],
            "missing_items": ["SLA target"],
            "details": ["Grounding score: 50% (Unsupported: 1)"]
        }
    });
    let resp2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compress")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {jwt}"))
                .body(Body::from(t2_body.to_string()))
                .expect("req2"),
        )
        .await
        .expect("exec2");
    assert_eq!(resp2.status(), StatusCode::OK);
    let b2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("b2");
    let j2: serde_json::Value = serde_json::from_slice(&b2).expect("j2");
    let opt2 = j2["optimized_prompt"].as_str().unwrap_or_default();
    assert!(
        opt2.starts_with("# REPAIR DIRECTIVE"),
        "Balanced mode + repair_context must render # REPAIR DIRECTIVE:\n{opt2}"
    );
}
