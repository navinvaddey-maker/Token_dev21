//! Integration tests for exclusive Cloud Data vs Own Data Ask RAG routing
//! (`ARCHITECTURE.md` §2.1).

use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use token_compress_engine::rag::embeddings::EmbeddingEngine;
use token_compress_engine::rag::retriever::DocumentRetriever;
use token_compress_engine::rag::store::RagStore;
use token_compress_engine::rag::types::embedding_to_bytes;
use token_compress_engine::scenario::{
    ScenarioDataSource, ScenarioDomainRegistry, ScenarioEgressGuard, ScenarioModeRouter,
    ScenarioStyleRegistry,
};
use token_compress_engine::types::{ChunkMetadata, RagChunk};

const USER: &str = "user-local-rag-test";

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite must open");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations must apply");
    sqlx::query("INSERT INTO users (id, username, password_hash, email) VALUES ($1, $1, 'x', $2)")
        .bind(USER)
        .bind(format!("{USER}@test.local"))
        .execute(&pool)
        .await
        .expect("insert test user");
    pool
}

fn build_router() -> ScenarioModeRouter {
    ScenarioModeRouter::new(
        Arc::new(
            ScenarioDomainRegistry::load_from_file("config/scenario/domains.json")
                .expect("load domains"),
        ),
        Arc::new(
            ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
                .expect("load styles"),
        ),
        Arc::new(
            ScenarioEgressGuard::load_from_file("config/scenario/egress.json")
                .expect("load egress"),
        ),
        "prompts/styles".to_string(),
    )
}

#[tokio::test]
async fn own_data_mode_uses_only_local_chunks_and_bypasses_cloud_db() {
    let pool = migrated_pool().await;
    let engine = EmbeddingEngine::new();

    // Seed a cloud document with an unmistakably distinct secret marker.
    let cloud_content = "CLOUD_ONLY_SECRET_MARKER: cloud liability cap is 999 USD.";
    let blob = embedding_to_bytes(&engine.embed(cloud_content));
    sqlx::query(
        "INSERT INTO rag_documents (id, user_id, filename, domain, content_hash, status)
         VALUES ('cloud-1', $1, 'cloud_doc.pdf', 'legal_docs', 'hash-cloud-1', 'ready')",
    )
    .bind(USER)
    .execute(&pool)
    .await
    .expect("insert cloud doc");
    sqlx::query(
        "INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding)
         VALUES ('cloud-1-c0', 'cloud-1', 0, $1, $2)",
    )
    .bind(cloud_content)
    .bind(blob)
    .execute(&pool)
    .await
    .expect("insert cloud chunk");

    let retriever = DocumentRetriever::new(RagStore::new(pool));
    let local_chunk = RagChunk {
        domain_tag: "local_own_data".to_string(),
        content: "LOCAL_ONLY_FACT: Termination requires 45 days written notice.".to_string(),
        metadata: Some(ChunkMetadata {
            page_number: Some(2),
            source_file: "local_nda.docx".to_string(),
        }),
    };

    // Regular mode with Own Data
    let reg_resp = build_router()
        .route_and_execute_with_source(
            "Regular",
            None,
            Some("balanced"),
            None,
            "What is the termination notice period?",
            USER,
            "general",
            None,
            &retriever,
            None,
            ScenarioDataSource::Own {
                local_chunks: vec![local_chunk.clone()],
            },
        )
        .await
        .expect("own data regular mode succeeds");

    assert!(!reg_resp.is_empty_knowledge);
    assert!(reg_resp.output_text.contains("LOCAL_ONLY_FACT"));
    assert!(!reg_resp.output_text.contains("CLOUD_ONLY_SECRET_MARKER"));
    assert_eq!(reg_resp.searched_namespaces, vec!["local_own_data"]);
    assert_eq!(
        reg_resp.source_badge,
        "Source: own local folder only (127.0.0.1:49152)"
    );
    assert!(reg_resp
        .citations
        .iter()
        .any(|c| c.contains("local_nda.docx")));

    // Scenario mode with Own Data
    let scen_resp = build_router()
        .route_and_execute_with_source(
            "Scenario",
            Some("legal"),
            None,
            None,
            "What is the termination notice period?",
            USER,
            "general",
            None,
            &retriever,
            None,
            ScenarioDataSource::Own {
                local_chunks: vec![local_chunk],
            },
        )
        .await
        .expect("own data scenario mode succeeds");

    assert!(!scen_resp.is_empty_knowledge);
    assert!(scen_resp.output_text.contains("LOCAL_ONLY_FACT"));
    assert!(!scen_resp.output_text.contains("CLOUD_ONLY_SECRET_MARKER"));
    assert_eq!(
        scen_resp.source_badge,
        "Source: own local folder only (127.0.0.1:49152)"
    );

    // Cloud Data mode remains isolated from local chunks and uses cloud SQLite DB.
    let cloud_resp = build_router()
        .route_and_execute_with_source(
            "Scenario",
            Some("legal"),
            None,
            None,
            "What is the cloud liability cap?",
            USER,
            "general",
            None,
            &retriever,
            None,
            ScenarioDataSource::Cloud,
        )
        .await
        .expect("cloud data scenario mode succeeds");

    assert!(!cloud_resp.is_empty_knowledge);
    assert!(cloud_resp.output_text.contains("CLOUD_ONLY_SECRET_MARKER"));
    assert!(!cloud_resp.output_text.contains("LOCAL_ONLY_FACT"));
    assert_eq!(
        cloud_resp.source_badge,
        "Source: internal knowledge base only"
    );
}

#[tokio::test]
async fn own_data_mode_with_empty_chunks_returns_empty_knowledge() {
    let pool = migrated_pool().await;
    let retriever = DocumentRetriever::new(RagStore::new(pool));

    let resp = build_router()
        .route_and_execute_with_source(
            "Regular",
            None,
            Some("concise"),
            None,
            "Any local policy?",
            USER,
            "general",
            None,
            &retriever,
            None,
            ScenarioDataSource::Own {
                local_chunks: vec![],
            },
        )
        .await
        .expect("empty own data succeeds cleanly");

    assert!(resp.is_empty_knowledge);
    assert!(resp.citations.is_empty());
}
