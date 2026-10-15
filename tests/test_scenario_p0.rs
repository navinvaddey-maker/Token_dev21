//! P0 regression tests for the scenario pipeline (bugs D1–D3 from
//! `scenario_functionality_analysis.md`).
//!
//! Each test was written to FAIL against the pre-fix code and pass after the fix.
//! All SQL-touching tests run against a real, migrated in-memory SQLite (rules §7: no mocks).

use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use token_compress_engine::rag::embeddings::EmbeddingEngine;
use token_compress_engine::rag::retriever::DocumentRetriever;
use token_compress_engine::rag::store::RagStore;
use token_compress_engine::rag::types::embedding_to_bytes;
use token_compress_engine::scenario::{
    ScenarioDomainRegistry, ScenarioEgressGuard, ScenarioModeRouter, ScenarioNamespaceGuard,
    ScenarioStyleRegistry,
};

const USER: &str = "user-p0";

/// Opens a single-connection in-memory pool.
///
/// `max_connections(1)` is required: every SQLite `:memory:` connection is a separate
/// database, so a multi-connection pool would see empty, unmigrated databases.
async fn memory_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite must open")
}

/// Memory pool with all repo migrations applied plus one test user (FK target for rag_documents).
async fn migrated_pool() -> SqlitePool {
    let pool = memory_pool().await;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations must apply to a fresh db");
    sqlx::query("INSERT INTO users (id, username, password_hash, email) VALUES ($1, $1, 'x', $2)")
        .bind(USER)
        .bind(format!("{USER}@test.local"))
        .execute(&pool)
        .await
        .expect("insert test user");
    pool
}

/// Inserts one ready document with a single embedded chunk.
async fn insert_doc_with_chunk(
    pool: &SqlitePool,
    engine: &EmbeddingEngine,
    doc_id: &str,
    filename: &str,
    domain: &str,
    content: &str,
) {
    sqlx::query(
        "INSERT INTO rag_documents (id, user_id, filename, domain, content_hash, status)
         VALUES ($1, $2, $3, $4, $1, 'ready')",
    )
    .bind(doc_id)
    .bind(USER)
    .bind(filename)
    .bind(domain)
    .execute(pool)
    .await
    .expect("insert document");

    let blob = embedding_to_bytes(&engine.embed(content));
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count)
         VALUES ($1, $2, 0, $3, $4, '{\"page_number\":1}', 10)",
    )
    .bind(format!("{doc_id}-c0"))
    .bind(doc_id)
    .bind(content)
    .bind(blob)
    .execute(pool)
    .await
    .expect("insert chunk");
}

fn router() -> ScenarioModeRouter {
    ScenarioModeRouter::new(
        Arc::new(ScenarioDomainRegistry::load_from_file("config/scenario/domains.json").unwrap()),
        Arc::new(
            ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
                .unwrap(),
        ),
        Arc::new(ScenarioEgressGuard::load_from_file("config/scenario/egress.json").unwrap()),
        "prompts/styles".to_string(),
    )
}

// ── D1 ────────────────────────────────────────────────────────────────────

/// D1: the relevance filter stemmed terms with a byte slice (`&term[..len-1]`), which panics
/// when the last char is multi-byte (e.g. Devanagari). Must not panic and must still match.
#[test]
fn d1_relevance_filter_handles_non_ascii_terms_without_panic() {
    let query = "संपत्ति बंटवारा";
    let chunk = "पिता की संपत्ति का बंटवारा हिंदू उत्तराधिकार अधिनियम के अनुसार होता है";
    let unrelated = "Stalking 354D Theft 379 Defamation 499";

    assert!(ScenarioNamespaceGuard::is_chunk_content_relevant(
        query, chunk
    ));
    assert!(!ScenarioNamespaceGuard::is_chunk_content_relevant(
        query, unrelated
    ));
}

// ── D2 ────────────────────────────────────────────────────────────────────

/// D2: namespace filtering ran AFTER top-k truncation, so a relevant in-namespace chunk was
/// crowded out by higher-ranked chunks from another domain → false "Not found".
#[tokio::test]
async fn d2_in_namespace_chunk_not_crowded_out_by_other_domains() {
    let pool = migrated_pool().await;
    let engine = EmbeddingEngine::new();
    let question = "Tenant eviction notice period under the rental agreement termination clause";

    // 12 out-of-namespace chunks that match the question exactly (outrank everything,
    // and exceed the internal candidate window of top_k * 2 = 10).
    for i in 0..12 {
        insert_doc_with_chunk(
            &pool,
            &engine,
            &format!("biz-{i}"),
            &format!("strategy_{i}.pdf"),
            "business_strategy",
            question,
        )
        .await;
    }
    // One in-namespace chunk: relevant, but ranked below the 12 above.
    insert_doc_with_chunk(
        &pool,
        &engine,
        "legal-1",
        "lease_act.pdf",
        "legal_docs",
        "Tenant eviction notice period under the rental agreement termination clause. \
         Section 106 of the Transfer of Property Act requires fifteen days notice.",
    )
    .await;

    let retriever = DocumentRetriever::new(RagStore::new(pool));
    let resp = router()
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            None,
            question,
            USER,
            "legal",
            None,
            &retriever,
            None,
        )
        .await
        .expect("scenario request must succeed");

    assert!(
        !resp.is_empty_knowledge,
        "in-namespace chunk was dropped: {}",
        resp.output_text
    );
    assert!(resp.citations.iter().any(|c| c.contains("lease_act.pdf")));
    assert!(
        resp.citations.iter().all(|c| !c.contains("strategy_")),
        "out-of-namespace chunk leaked: {:?}",
        resp.citations
    );
}

// ── D3 ────────────────────────────────────────────────────────────────────

/// D3: a retrieval/DB failure was swallowed with `unwrap_or_default()` and reported to the user
/// as "Not found in the internal knowledge base". It must surface as an error.
#[tokio::test]
async fn d3_retrieval_failure_is_an_error_not_empty_knowledge() {
    // Unmigrated db → rag tables do not exist → the store query fails.
    let retriever = DocumentRetriever::new(RagStore::new(memory_pool().await));
    let r = router();

    let scenario = r
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            None,
            "Tenant eviction notice period in Telangana",
            USER,
            "legal",
            None,
            &retriever,
            None,
        )
        .await;
    let err = scenario.expect_err("scenario mode must not hide a DB failure as 'not found'");
    assert!(err.contains("Retrieval Error"), "unexpected error: {err}");

    let regular = r
        .route_and_execute(
            "Regular",
            None,
            Some("balanced"),
            None,
            "Tenant eviction notice period in Telangana",
            USER,
            "legal",
            None,
            &retriever,
            None,
        )
        .await;
    let err = regular.expect_err("regular mode must not hide a DB failure as 'not found'");
    assert!(err.contains("Retrieval Error"), "unexpected error: {err}");
}

/// Guard against over-correction of D3: a healthy but empty knowledge base is still a
/// legitimate "Not found" (rules §9: no context → "I don't know"), not an error.
#[tokio::test]
async fn d3_empty_but_healthy_kb_still_returns_not_found() {
    let retriever = DocumentRetriever::new(RagStore::new(migrated_pool().await));
    let resp = router()
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            None,
            "Tenant eviction notice period in Telangana",
            USER,
            "legal",
            None,
            &retriever,
            None,
        )
        .await
        .expect("empty kb is not an error");
    assert!(resp.is_empty_knowledge);
    assert!(resp
        .output_text
        .contains("Not found in the internal knowledge base"));
}

// ── D5 ────────────────────────────────────────────────────────────────────

/// D5: retrieved knowledge in scenario mode must be rendered as clean markdown without
/// XML <evidence_chunk> wrappers as requested by user.
#[tokio::test]
async fn d5_retrieved_evidence_clean_markdown_no_xml_tags() {
    let pool = migrated_pool().await;
    let engine = EmbeddingEngine::new();
    let question = "How is residential rent deposit refunded under Andhra tenancy rules?";

    let chunk = "Standard deposit is 3 months under statutory rental terms.";
    insert_doc_with_chunk(
        &pool,
        &engine,
        "legal-clean",
        "tenancy_act.pdf",
        "legal_docs",
        chunk,
    )
    .await;

    let retriever = DocumentRetriever::new(RagStore::new(pool));
    let resp = router()
        .route_and_execute(
            "Scenario",
            Some("legal"),
            None,
            None,
            question,
            USER,
            "legal",
            None,
            &retriever,
            None,
        )
        .await
        .expect("scenario request must succeed");

    assert!(!resp.is_empty_knowledge);
    assert!(
        resp.output_text.contains("### Key Evidence [1]"),
        "Output must format evidence using clean markdown header"
    );
    assert!(
        !resp.output_text.contains("<evidence_chunk"),
        "Output must NOT contain <evidence_chunk> XML tags"
    );
    assert!(
        !resp.output_text.contains("</evidence_chunk>"),
        "Output must NOT contain </evidence_chunk> XML tags"
    );
}
