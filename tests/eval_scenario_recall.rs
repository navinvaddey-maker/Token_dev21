//! Retrieval evaluation for Scenario mode against the REAL local corpus (`token_compress.db`,
//! document `legal_docs_india.pdf`). Rules §9: "Evaluation before claims" — measure recall@k / MRR
//! before touching `min_similarity` or the relevance filter.
//!
//! # Method and limits (read before trusting the numbers)
//! - Ground truth = the chunk each question was written from (author-labelled by reading the chunk
//!   text; NOT independently annotated). 20 positive + 4 negative (out-of-corpus) questions.
//!   That is a small set: treat differences of one question (~5 points) as noise.
//! - Chunk embeddings are the ones stored at ingest time; query embeddings use the CURRENT backend
//!   (fastembed ONNX if loadable, else hash fallback). If those differ the numbers are invalid.
//! - Works on a COPY of the database so the live file is never written.
//!
//! Run: `cargo test --test eval_scenario_recall -- --ignored --nocapture`

#![allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

use std::collections::HashSet;
use std::sync::Arc;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use token_compress_engine::rag::retriever::DocumentRetriever;
use token_compress_engine::rag::store::RagStore;
use token_compress_engine::rag::types::{RetrievalQuery, SearchResult};
use token_compress_engine::scenario::{
    ScenarioDomainRegistry, ScenarioEgressGuard, ScenarioModeRouter, ScenarioNamespaceGuard,
    ScenarioStyleRegistry,
};
use token_compress_engine::utils::tokens::estimate_tokens;

const SRC_DB: &str = "token_compress.db";
const COPY_DB: &str = "target/eval_scenario_copy.db";
const SOURCE_FILE: &str = "legal_docs_india.pdf";
const OWNER: &str = "ed1b56f8-a75d-436d-9f50-be53128ccbd5";
const TOP_K: usize = 5;

/// (question, gold `chunk_index` within `legal_docs_india.pdf`).
const POSITIVES: &[(&str, i64)] = &[
    ("Does insufficient funds count as dishonour of a cheque?", 3),
    ("Is a cheque returned because the account was closed covered under section 138?", 3),
    ("Can a cheque bounce case be settled between the parties at any stage?", 4),
    ("Does a lease of more than one year need to be registered?", 6),
    ("What does a mortgage transfer to the lender?", 6),
    ("Can a buyer force the seller to complete a sale agreement?", 7),
    ("How many witnesses are needed to attest a will?", 8),
    ("How can I get a temporary injunction from a civil court?", 10),
    ("Where do I complain about defective goods or poor service?", 11),
    ("What are the rights of a person at the time of arrest?", 13),
    ("When can police issue a notice instead of arresting for offences up to 7 years?", 13),
    ("What happens between police investigation and filing the charge sheet?", 14),
    ("What is the punishment for stalking?", 15),
    ("What is judicial separation?", 17),
    ("Can inter-faith couples divorce under the Special Marriage Act?", 18),
    ("How is property divided in a divorce in India?", 19),
    ("What is the deadline to send a demand notice after a cheque bounces?", 20),
    ("What does cause of action mean?", 21),
    ("What is a cognizable offence?", 21),
    ("What facts do I need for a cheque bounce case?", 2),
    // Long scenario narratives: the real Scenario-mode input shape.
    ("My client's cheque for Rs 5 lakh was returned by the bank because the account was closed, and the payee wants to know if section 138 applies and what notice must be sent.", 3),
    ("A landlord gave a tenant an eleven month lease and now the tenant refuses to leave; does a lease for more than one year need a registered instrument under the Transfer of Property Act?", 6),
    ("Husband and wife of different religions married under the Special Marriage Act want to file for divorce; which grounds apply and how is the property divided?", 18),
];

/// Questions with NO answer in the corpus: a good system returns nothing.
const NEGATIVES: &[&str] = &[
    "What is the recipe for chicken biryani?",
    "How do I configure a Kubernetes ingress controller?",
    "Explain the offside rule in football.",
    "Which vitamins help with muscle recovery after exercise?",
    // Long off-topic narrative that deliberately contains coincidental words (`recovery`, `rule`).
    "Our startup wants to deploy a machine learning model on kubernetes with autoscaling and monitoring for the recovery of failed pods under the rule of high availability.",
];

#[derive(Default)]
struct Row {
    hits: usize,
    rr_sum: f64,
    neg_with_results: usize,
}

async fn open_copy() -> Option<sqlx::SqlitePool> {
    if !std::path::Path::new(SRC_DB).exists() {
        println!("SKIP: {SRC_DB} not found");
        return None;
    }
    std::fs::copy(SRC_DB, COPY_DB).expect("copy db");
    let opts = SqliteConnectOptions::new().filename(COPY_DB);
    Some(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .expect("open db copy"),
    )
}

async fn run_one(
    retriever: &DocumentRetriever,
    question: &str,
    min_sim: f32,
    lexical: bool,
) -> Vec<SearchResult> {
    let permitted = vec!["legal_docs".to_string(), "legal".to_string()];
    let q = RetrievalQuery {
        query_text: question.to_string(),
        user_id: OWNER.to_string(),
        document_ids: None,
        top_k: TOP_K,
        min_similarity: min_sim,
    };
    let filter = |r: &SearchResult| {
        ScenarioNamespaceGuard::is_chunk_permitted(
            r.document_domain.as_deref(),
            Some(&r.document_filename),
            &permitted,
        ) && (!lexical
            || ScenarioNamespaceGuard::is_chunk_content_relevant(question, &r.chunk.content))
    };
    retriever
        .retrieve_filtered(&q, None, &filter)
        .await
        .expect("retrieval must not error")
}

#[tokio::test]
#[ignore = "needs local token_compress.db; run explicitly for retrieval evaluation"]
async fn eval_recall_vs_min_similarity() {
    let Some(pool) = open_copy().await else {
        return;
    };
    let retriever = DocumentRetriever::new(RagStore::new(pool));

    // Sanity: the corpus the labels refer to is actually what is loaded.
    let probe = run_one(&retriever, "cheque bounce", 0.0, false).await;
    assert!(
        probe.iter().any(|r| r.document_filename == SOURCE_FILE),
        "expected {SOURCE_FILE} in the corpus"
    );

    let thresholds = [0.45_f32, 0.40, 0.35, 0.30, 0.25, 0.20];
    println!(
        "\n=== Scenario retrieval eval: {} positives, {} negatives, top_k={TOP_K} ===",
        POSITIVES.len(),
        NEGATIVES.len()
    );
    println!(
        "{:<8} {:<9} {:>10} {:>8} {:>16}",
        "min_sim", "lex_filter", "recall@5", "MRR", "neg_false_pos"
    );

    for lexical in [true, false] {
        for &t in &thresholds {
            let mut row = Row::default();
            for &(q, gold) in POSITIVES {
                let res = run_one(&retriever, q, t, lexical).await;
                // rank of the gold chunk among results from the source document (1-based)
                let pos = res.iter().position(|r| {
                    r.document_filename == SOURCE_FILE && r.chunk.chunk_index == gold
                });
                if let Some(p) = pos {
                    row.hits += 1;
                    row.rr_sum += 1.0 / (p as f64 + 1.0);
                }
            }
            for q in NEGATIVES {
                if !run_one(&retriever, q, t, lexical).await.is_empty() {
                    row.neg_with_results += 1;
                }
            }
            let n = POSITIVES.len() as f64;
            println!(
                "{:<8.2} {:<9} {:>9.0}% {:>8.3} {:>11}/{}",
                t,
                if lexical { "ON" } else { "OFF" },
                100.0 * row.hits as f64 / n,
                row.rr_sum / n,
                row.neg_with_results,
                NEGATIVES.len()
            );
        }
    }

    // Per-question misses at the current production setting, to see WHY (0.45 + filter ON).
    println!("\n--- misses at min_sim=0.45, lexical filter ON ---");
    let mut seen = HashSet::new();
    for &(q, gold) in POSITIVES {
        let res = run_one(&retriever, q, 0.45, true).await;
        let ok = res
            .iter()
            .any(|r| r.document_filename == SOURCE_FILE && r.chunk.chunk_index == gold);
        if !ok && seen.insert(q) {
            let got: Vec<i64> = res.iter().map(|r| r.chunk.chunk_index).collect();
            println!("MISS gold={gold} got={got:?} | {q}");
        }
    }

    // Which chunks leak for out-of-corpus questions under the production filter?
    println!("\n--- negative leaks at min_sim=0.45, lexical filter ON ---");
    for q in NEGATIVES {
        let res = run_one(&retriever, q, 0.45, true).await;
        let got: Vec<(i64, f32)> = res
            .iter()
            .map(|r| (r.chunk.chunk_index, r.similarity_score))
            .collect();
        println!("{q} -> {got:?}");
    }

    // Filter variants, evaluated on the UNFILTERED candidate set so only the filter differs.
    println!("\n=== relevance-filter variants (min_sim=0.45) ===");
    println!(
        "{:<10} {:>10} {:>8} {:>16}",
        "variant", "recall@5", "MRR", "neg_false_pos"
    );
    for mode in [Mode::Any1, Mode::Min2, Mode::Frac50, Mode::ShortMin2] {
        let mut row = Row::default();
        for &(q, gold) in POSITIVES {
            let res: Vec<SearchResult> = run_one(&retriever, q, 0.45, false)
                .await
                .into_iter()
                .filter(|r| relevant(q, &r.chunk.content, mode))
                .collect();
            if let Some(p) = res
                .iter()
                .position(|r| r.document_filename == SOURCE_FILE && r.chunk.chunk_index == gold)
            {
                row.hits += 1;
                row.rr_sum += 1.0 / (p as f64 + 1.0);
            }
        }
        for q in NEGATIVES {
            let any = run_one(&retriever, q, 0.45, false)
                .await
                .into_iter()
                .any(|r| relevant(q, &r.chunk.content, mode));
            if any {
                row.neg_with_results += 1;
            }
        }
        let n = POSITIVES.len() as f64;
        println!(
            "{:<10} {:>9.0}% {:>8.3} {:>11}/{}",
            format!("{mode:?}"),
            100.0 * row.hits as f64 / n,
            row.rr_sum / n,
            row.neg_with_results,
            NEGATIVES.len()
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    /// Production rule: at least one query term (or its stem) appears in the chunk.
    Any1,
    /// At least two distinct query terms must appear (one when the query has a single term).
    Min2,
    /// At least half of the distinct query terms must appear.
    Frac50,
    /// Queries with <= 4 distinct terms need 2 matches (1 if only one term); longer queries keep
    /// the production rule of 1 match, because long narratives legitimately match few terms.
    ShortMin2,
}

/// Local re-implementation of `is_chunk_content_relevant` with a configurable match count.
/// `Mode::Any1` must reproduce the production filter exactly (checked by the numbers matching the
/// "`lex_filter` ON" rows above); the stop-word list and stem rule are copied from `guards.rs`.
fn relevant(query: &str, chunk: &str, mode: Mode) -> bool {
    const STOP: &[&str] = &[
        "this", "that", "with", "from", "they", "been", "have", "what", "which", "when", "where",
        "how", "who", "whom", "than", "then", "these", "those", "each", "every", "some", "such",
        "only", "into", "over", "after", "also", "would", "could", "should", "about", "there",
        "their", "please", "search",
    ];
    let q = query.to_lowercase();
    let c = chunk.to_lowercase();
    let mut terms: Vec<&str> = q
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.len() >= 4 && !STOP.contains(w))
        .collect();
    terms.sort_unstable();
    terms.dedup();
    if terms.is_empty() {
        return true;
    }
    let matched = terms
        .iter()
        .filter(|term| {
            let stem = if term.chars().count() > 5 {
                term.char_indices()
                    .last()
                    .map_or(**term, |(i, _)| &term[..i])
            } else {
                term
            };
            c.contains(stem)
        })
        .count();
    match mode {
        Mode::Any1 => matched >= 1,
        Mode::Min2 => matched >= 2.min(terms.len()),
        Mode::Frac50 => matched * 2 >= terms.len(),
        Mode::ShortMin2 => {
            matched
                >= if terms.len() <= 4 {
                    2.min(terms.len())
                } else {
                    1
                }
        }
    }
}

/// Evaluates prompt compression ratio, schema contract preservation, and reasoning grounding
/// across domains in Scenario Mode.
///
/// Rules §2 ("Do not fabricate benchmarks") & §9 ("Evaluation before claims").
///
/// Run: `cargo test --test eval_scenario_recall -- eval_scenario_compression_benchmark --ignored --nocapture`
#[tokio::test]
#[ignore = "needs local token_compress.db; run explicitly for scenario compression evaluation"]
async fn eval_scenario_compression_benchmark() {
    let Some(pool) = open_copy().await else {
        return;
    };
    let retriever = DocumentRetriever::new(RagStore::new(pool.clone()));

    // Load domain, style, and egress registries
    let domain_reg = Arc::new(
        ScenarioDomainRegistry::load_from_file("config/scenario/domains.json")
            .expect("load domains.json"),
    );
    let style_reg = Arc::new(
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .expect("load styles.json"),
    );
    let egress_guard = Arc::new(
        ScenarioEgressGuard::load_from_file("config/scenario/egress.json")
            .expect("load egress.json"),
    );

    let router = ScenarioModeRouter::new(
        domain_reg,
        style_reg,
        egress_guard,
        "prompts/styles".to_string(),
    );

    // Build orchestrator with local database pool
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

    // Test across distinct scenarios in legal domain with local corpus
    let benchmark_queries = [
        (
            "What is Section 43A compensation liability for body corporate sensitive data?",
            "legal",
            Some("india"),
        ),
        (
            "Does insufficient funds count as dishonour of a cheque under section 138?",
            "legal",
            Some("india"),
        ),
        (
            "Can a cheque bounce case be settled between the parties at any stage?",
            "legal",
            Some("india"),
        ),
        (
            "Does a lease of more than one year need to be registered under Transfer of Property Act?",
            "legal",
            Some("india"),
        ),
        (
            "My client's cheque for Rs 5 lakh was returned by the bank because the account was closed, and the payee wants to know if section 138 applies and what notice must be sent.",
            "legal",
            Some("india"),
        ),
    ];

    println!("\n=== Scenario Mode Prompt Compression & Contract Integrity Benchmark ===");
    println!(
        "{:<45} | {:>10} | {:>12} | {:>11} | {:>13} | {:>12}",
        "Query (truncated)",
        "Raw Tokens",
        "Comp. Tokens",
        "Compression",
        "Contract Int.",
        "Fact Matrix"
    );
    println!("{:-<116}", "");

    let mut total_raw_tokens: u64 = 0;
    let mut total_comp_tokens: u64 = 0;
    let mut all_contracts_valid = true;
    let mut all_matrices_present = true;

    for (query, domain, geo) in &benchmark_queries {
        // 1. Raw prompt execution (compression_mode = None)
        let raw_resp = router
            .route_and_execute(
                "Scenario",
                Some(domain),
                None,
                *geo,
                query,
                OWNER,
                domain,
                None,
                &retriever,
                None,
            )
            .await
            .expect("raw scenario execution must succeed");

        // 2. Aggressive compressed prompt execution
        let comp_resp = router
            .route_and_execute(
                "Scenario",
                Some(domain),
                None,
                *geo,
                query,
                OWNER,
                domain,
                Some("aggressive"),
                &retriever,
                Some(&orchestrator),
            )
            .await
            .expect("compressed scenario execution must succeed");

        let raw_tokens = estimate_tokens(&raw_resp.output_text);
        let comp_tokens = estimate_tokens(&comp_resp.output_text);

        total_raw_tokens += u64::from(raw_tokens);
        total_comp_tokens += u64::from(comp_tokens);

        // Verify Contract Integrity: all 8 sections must exist in both
        let contract_valid = comp_resp.output_text.contains("=== SYSTEM PROMPT ===")
            && comp_resp
                .output_text
                .contains("# 1. SYSTEM DIRECTIVE & SPECIALIZED ROLE")
            && comp_resp
                .output_text
                .contains("# 4. OPERATIONAL CONSTRAINTS & GAP WARNINGS")
            && comp_resp
                .output_text
                .contains("# 5. DETERMINISTIC TOOLS & RESOURCE CAPABILITIES")
            && comp_resp
                .output_text
                .contains("# 6. TARGET RESPONSE SCHEMA CONTRACT")
            && comp_resp
                .output_text
                .contains("# 7. RECOMMENDED EXECUTION MODEL")
            && comp_resp
                .output_text
                .contains("# 8. REGULATORY & COMPLIANCE DISCLAIMER")
            && comp_resp.output_text.contains("=== USER PROMPT ===")
            && comp_resp
                .output_text
                .contains("# 2. USER SCENARIO OBJECTIVE");

        if !contract_valid {
            all_contracts_valid = false;
        }

        // Verify Aggressive Compressed Fact Matrix clean synthesis (no raw scaffold markers)
        let fact_matrix_clean = comp_resp
            .output_text
            .contains("# 3. VERIFIED GROUNDING CONTEXT (COMPRESSED FACT MATRIX)")
            && !comp_resp.output_text.contains("### Key Evidence [");

        if !fact_matrix_clean {
            all_matrices_present = false;
        }

        let ratio_pct = if raw_tokens > 0 {
            100.0 * (1.0 - (f64::from(comp_tokens) / f64::from(raw_tokens)))
        } else {
            0.0
        };

        let display_q: String = if query.chars().count() > 45 {
            query.chars().take(42).collect::<String>() + "..."
        } else {
            query.to_string()
        };

        println!(
            "{:<45} | {:>10} | {:>12} | {:>10.1}% | {:>13} | {:>12}",
            display_q,
            raw_tokens,
            comp_tokens,
            ratio_pct,
            if contract_valid {
                "PRESERVED"
            } else {
                "BROKEN"
            },
            if fact_matrix_clean { "CLEAN" } else { "DIRTY" }
        );
    }

    let net_savings_pct = if total_raw_tokens > 0 {
        100.0 * (1.0 - (total_comp_tokens as f64 / total_raw_tokens as f64))
    } else {
        0.0
    };

    println!("{:-<116}", "");
    println!(
        "{:<45} | {:>10} | {:>12} | {:>10.1}% | {:>13} | {:>12}",
        "TOTAL AGGREGATE",
        total_raw_tokens,
        total_comp_tokens,
        net_savings_pct,
        if all_contracts_valid {
            "100% PASS"
        } else {
            "FAIL"
        },
        if all_matrices_present {
            "100% PASS"
        } else {
            "FAIL"
        }
    );

    assert!(
        all_contracts_valid,
        "All prompt contracts must be preserved"
    );
    assert!(
        all_matrices_present,
        "Fact matrices must be synthesized cleanly"
    );
    assert!(
        total_comp_tokens <= total_raw_tokens,
        "Compressed tokens should not exceed raw tokens"
    );
}

// ── VER-01: Self-contained Cloud (document_ids) & Own Data (LocalIndexStore) Evaluation ──

/// RAII cleanup guard for temporary local-RAG evaluation directories.
struct EvalTempDirGuard {
    path: std::path::PathBuf,
}

impl Drop for EvalTempDirGuard {
    fn drop(&mut self) {
        // Best-effort cleanup of temporary evaluation directory on Drop; ignore I/O errors.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Builds a valid `.docx` ZIP archive in memory with one page break between paragraphs.
///
/// Mirrors `src/rag/local.rs` test fixture format so `LocalIndexStore::sync_folder` parses
/// multi-page `.docx` documents via `zip` + `xml-rs`.
fn build_eval_docx(paragraphs: &[&str]) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut buf);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        assert!(
            writer.start_file("word/document.xml", options).is_ok(),
            "start word/document.xml in zip"
        );
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
        );
        for (idx, para) in paragraphs.iter().enumerate() {
            if idx > 0 {
                xml.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
            }
            let escaped = para
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            xml.push_str("<w:p><w:r><w:t>");
            xml.push_str(&escaped);
            xml.push_str("</w:t></w:r></w:p>");
        }
        xml.push_str("</w:body></w:document>");
        assert!(
            writer.write_all(xml.as_bytes()).is_ok(),
            "write word/document.xml bytes"
        );
        assert!(writer.finish().is_ok(), "finish docx zip archive");
    }
    buf.into_inner()
}

/// Builds a minimal valid multi-page PDF 1.4 document with exact `xref` byte offsets.
///
/// Enables `LocalIndexStore::sync_folder` to extract real `.pdf` pages via `pdf-extract`.
fn build_eval_pdf(pages: &[&str]) -> Vec<u8> {
    use std::fmt::Write as _;

    let mut out = String::from("%PDF-1.4\n");
    let mut offsets: Vec<usize> = vec![0]; // obj 0 is free head

    // Obj 1: Catalog, Obj 2: Pages, Obj 3: Font
    offsets.push(out.len());
    out.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let page_count = pages.len();
    let kids: Vec<String> = (0..page_count)
        .map(|i| format!("{} 0 R", 4 + i * 2))
        .collect();
    offsets.push(out.len());
    // std::fmt::Write for String is infallible.
    let _ = writeln!(
        out,
        "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj",
        kids.join(" "),
        page_count
    );

    offsets.push(out.len());
    out.push_str("3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");

    for (idx, text) in pages.iter().enumerate() {
        let page_obj = 4 + idx * 2;
        let content_obj = page_obj + 1;
        let escaped = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let stream_body = format!("BT\n/F1 11 Tf\n72 720 Td\n({escaped}) Tj\nET\n");

        offsets.push(out.len());
        // std::fmt::Write for String is infallible.
        let _ = writeln!(
            out,
            "{page_obj} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_obj} 0 R >>\nendobj"
        );

        offsets.push(out.len());
        // std::fmt::Write for String is infallible.
        let _ = writeln!(
            out,
            "{content_obj} 0 obj\n<< /Length {} >>\nstream\n{stream_body}endstream\nendobj",
            stream_body.len()
        );
    }

    let xref_offset = out.len();
    // std::fmt::Write for String is infallible.
    let _ = writeln!(out, "xref\n0 {}", offsets.len());
    out.push_str("0000000000 65535 f \n");
    for &off in offsets.iter().skip(1) {
        // std::fmt::Write for String is infallible.
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    // std::fmt::Write for String is infallible.
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        offsets.len()
    );
    out.into_bytes()
}

/// Parameters for seeding a Cloud evaluation document and its chunks into `RagStore`.
struct SeedCloudDoc<'a> {
    tenant_id: &'a str,
    user_id: &'a str,
    doc_id: &'a str,
    filename: &'a str,
    domain: &'a str,
    status: token_compress_engine::rag::types::DocumentStatus,
    chunks_text: &'a [&'a str],
}

/// Seeds a document and its embedded chunks into `RagStore` under an explicit `(tenant_id, user_id)`.
async fn seed_eval_cloud_document(
    store: &RagStore,
    engine: &token_compress_engine::rag::embeddings::EmbeddingEngine,
    spec: SeedCloudDoc<'_>,
) {
    use token_compress_engine::rag::types::{
        ChunkMetadata, RagChunk, RagDocument, DEFAULT_EMBEDDING_MODEL, DEFAULT_MODEL_VERSION,
    };

    let now = chrono::Utc::now();
    let chunk_count = i64::try_from(spec.chunks_text.len()).expect("chunk count fits in i64");
    let doc = RagDocument {
        id: spec.doc_id.to_string(),
        user_id: spec.user_id.to_string(),
        filename: spec.filename.to_string(),
        domain: Some(spec.domain.to_string()),
        content_hash: format!("hash-{}-{}", spec.tenant_id, spec.doc_id),
        page_count: Some(chunk_count),
        chunk_count: Some(chunk_count),
        status: spec.status,
        created_at: now,
        updated_at: now,
    };
    assert!(
        store
            .insert_document_with_tenant(&doc, spec.tenant_id)
            .await
            .is_ok(),
        "insert_document_with_tenant must succeed for {}",
        spec.doc_id
    );

    let rag_chunks: Vec<RagChunk> = spec
        .chunks_text
        .iter()
        .enumerate()
        .map(|(idx, content)| RagChunk {
            id: format!("{}-chunk-{idx}", spec.doc_id),
            document_id: spec.doc_id.to_string(),
            chunk_index: i64::try_from(idx).expect("chunk index fits in i64"),
            content: (*content).to_string(),
            embedding: Some(engine.embed(content)),
            metadata: Some(ChunkMetadata {
                page_number: Some(i64::try_from(idx + 1).expect("page number fits in i64")),
                ..Default::default()
            }),
            token_count: Some(i64::from(estimate_tokens(content))),
            created_at: now,
            embedding_model: DEFAULT_EMBEDDING_MODEL.to_string(),
            model_version: DEFAULT_MODEL_VERSION.to_string(),
        })
        .collect();

    assert!(
        store.insert_chunks(&rag_chunks).await.is_ok(),
        "insert_chunks must succeed for {}",
        spec.doc_id
    );
}

struct CloudEvalMetrics {
    queries: usize,
    recall_at_5: f64,
    mrr: f64,
    leak_pct: f64,
    cross_doc_leaks: usize,
    processing_leaks: usize,
    empty_ids_leaks: usize,
    other_tenant_leaks: usize,
    default_tenant_doc_leaks: usize,
    sentinel_unfiltered_leaks: usize,
}

struct OwnDataEvalMetrics {
    queries: usize,
    recall_at_5: f64,
    mrr: f64,
    leak_pct: f64,
    total_leaks: usize,
    cross_ext_leaks: usize,
    cross_source_leaks: usize,
}

struct CloudEvalScope<'a> {
    tenant_id: &'a str,
    user_id: &'a str,
    other_tenant: &'a str,
    other_user: &'a str,
    legal_chunks: &'a [&'a str],
}

/// Evaluates Cloud `document_ids` scoping, `status='ready'` enforcement, and cross-tenant isolation.
async fn eval_cloud_document_ids(
    store: &RagStore,
    retriever: &DocumentRetriever,
    engine: &token_compress_engine::rag::embeddings::EmbeddingEngine,
    scope: CloudEvalScope<'_>,
) -> CloudEvalMetrics {
    use token_compress_engine::rag::types::DocumentStatus;

    let CloudEvalScope {
        tenant_id,
        user_id,
        other_tenant,
        other_user,
        legal_chunks,
    } = scope;

    let finance_chunks = [
        "Corporate treasury policy mandates a minimum liquid cash reserve equal to eighteen percent of quarterly operating expenses held in short-term Treasury bills.",
        "Foreign exchange risk hedging requires forward contracts covering seventy-five percent of forecasted EUR and JPY quarterly revenue exposure.",
        "Any single capital expenditure exceeding two hundred fifty thousand USD requires prior written authorization from the CFO and Audit Committee.",
        "Counterparty credit exposure to any individual commercial bank is capped at fifteen million USD across overnight deposits and money market funds.",
        "Quarterly working capital reconciliation and cash conversion cycle audit reports must be closed within ten business days after quarter end.",
        "Corporate procurement card single-transaction expense limit is capped at five thousand USD unless pre-approved by the VP of Finance.",
    ];
    let medical_chunks = [
        "Phase II clinical protocol inclusion criteria require adult participants aged 18 to 65 years with baseline HbA1c between 7.5 and 10.0 percent.",
        "All serious adverse events must be reported electronically to the Independent Data Safety Monitoring Board within twenty-four hours of investigator awareness.",
        "Investigational biologic vials require uninterrupted cold-chain storage between 2 and 8 degrees Celsius with calibrated continuous telemetry logging.",
        "Primary efficacy endpoint measures mean change in HbA1c from baseline at Week 24 evaluated via mixed-model repeated measures analysis.",
        "Protocol exclusion criteria bar subjects with estimated glomerular filtration rate below 45 mL per minute or myocardial infarction within six months.",
        "Emergency treatment unblinding requires Principal Investigator authorization and immediate immutable audit trail entry inside the EDC system.",
    ];
    let processing_trap_chunks = [
        "UNAPPROVED PROCESSING DRAFT: indemnification cap 999 months NOT AND OR exclusion gross negligence.",
        "UNAPPROVED PROCESSING DRAFT: corporate treasury liquidity reserve 99 percent NOT AND OR Treasury bills.",
        "UNAPPROVED PROCESSING DRAFT: clinical protocol HbA1c 15 percent cold-chain storage 99 hours NOT AND OR.",
    ];

    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id,
            user_id,
            doc_id: "doc-legal-ready",
            filename: "master_services_agreement.pdf",
            domain: "legal",
            status: DocumentStatus::Ready,
            chunks_text: legal_chunks,
        },
    )
    .await;
    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id,
            user_id,
            doc_id: "doc-finance-ready",
            filename: "treasury_risk_policy.pdf",
            domain: "finance",
            status: DocumentStatus::Ready,
            chunks_text: &finance_chunks,
        },
    )
    .await;
    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id,
            user_id,
            doc_id: "doc-medical-ready",
            filename: "clinical_trial_protocol.pdf",
            domain: "medical",
            status: DocumentStatus::Ready,
            chunks_text: &medical_chunks,
        },
    )
    .await;
    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id,
            user_id,
            doc_id: "doc-processing-trap",
            filename: "draft_unapproved_trap.pdf",
            domain: "legal",
            status: DocumentStatus::Processing,
            chunks_text: &processing_trap_chunks,
        },
    )
    .await;
    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id: other_tenant,
            user_id: other_user,
            doc_id: "doc-other-tenant-legal",
            filename: "other_tenant_msa.pdf",
            domain: "legal",
            status: DocumentStatus::Ready,
            chunks_text: &[
                "OTHER TENANT SECRET: indemnification cap is 99 years and SIAC arbitration is waived.",
            ],
        },
    )
    .await;

    // Seed an unmistakable Cloud-only sentinel doc under tenant_id="default" for cross-tenant & Own Data isolation verification.
    let cloud_only_sentinel = "CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731: internal cloud liability ceiling is 42 million USD.";
    seed_eval_cloud_document(
        store,
        engine,
        SeedCloudDoc {
            tenant_id: "default",
            user_id,
            doc_id: "doc-cloud-sentinel",
            filename: "cloud_sentinel_policy.pdf",
            domain: "legal",
            status: DocumentStatus::Ready,
            chunks_text: &[cloud_only_sentinel],
        },
    )
    .await;

    let cloud_cases: &[(&str, &str, i64)] = &[
        (
            "What is the indemnification liability cap and what exceptions apply for gross negligence?",
            "doc-legal-ready",
            0,
        ),
        (
            "How many days written notice are required for termination for convenience or material breach cure?",
            "doc-legal-ready",
            1,
        ),
        (
            "Which arbitration venue and SIAC rules govern contractual disputes?",
            "doc-legal-ready",
            2,
        ),
        (
            "What is the indemnification cap AND arbitration venue NOT OR survival period?",
            "doc-legal-ready",
            0,
        ),
        (
            "What minimum liquid cash reserve ratio must corporate treasury hold in Treasury bills?",
            "doc-finance-ready",
            0,
        ),
        (
            "What percentage of forecasted EUR and JPY revenue exposure must forward contracts hedge?",
            "doc-finance-ready",
            1,
        ),
        (
            "Which capital expenditure USD threshold requires CFO and Audit Committee written approval?",
            "doc-finance-ready",
            2,
        ),
        (
            "What adult age range and baseline HbA1c percentage are required for Phase II eligibility?",
            "doc-medical-ready",
            0,
        ),
        (
            "Within how many hours must serious adverse events be reported to the Safety Monitoring Board?",
            "doc-medical-ready",
            1,
        ),
        (
            "What Celsius temperature range and telemetry logging are required for biologic cold-chain storage?",
            "doc-medical-ready",
            2,
        ),
    ];

    let mut cloud_hits: usize = 0;
    let mut cloud_rr_sum: f64 = 0.0;
    let mut cloud_total_returned: usize = 0;
    let mut cloud_cross_doc_leaks: usize = 0;

    for &(query_text, target_doc_id, gold_chunk_idx) in cloud_cases {
        let q = RetrievalQuery {
            query_text: query_text.to_string(),
            user_id: user_id.to_string(),
            document_ids: Some(vec![target_doc_id.to_string()]),
            top_k: TOP_K,
            min_similarity: 0.15,
        };
        let res = retriever
            .retrieve_filtered_scoped(tenant_id, &q, None, &|_: &SearchResult| true)
            .await;
        assert!(
            res.is_ok(),
            "cloud retrieval must succeed for {target_doc_id}"
        );
        let results = res.unwrap_or_default();

        assert!(
            results.len() <= TOP_K,
            "returned results must respect top_k={TOP_K}"
        );
        cloud_total_returned += results.len();
        for item in &results {
            if item.chunk.document_id != target_doc_id {
                cloud_cross_doc_leaks += 1;
            }
        }
        if let Some(pos) = results.iter().position(|r| {
            r.chunk.document_id == target_doc_id && r.chunk.chunk_index == gold_chunk_idx
        }) {
            cloud_hits += 1;
            cloud_rr_sum += 1.0 / (pos as f64 + 1.0);
        }
    }

    // Verify processing document & empty document_ids list yield 0 results.
    let processing_probe = retriever
        .retrieve_filtered_scoped(
            tenant_id,
            &RetrievalQuery {
                query_text: "indemnification cap NOT AND OR".to_string(),
                user_id: user_id.to_string(),
                document_ids: Some(vec!["doc-processing-trap".to_string()]),
                top_k: TOP_K,
                min_similarity: 0.0,
            },
            None,
            &|_: &SearchResult| true,
        )
        .await;
    assert!(processing_probe.is_ok(), "processing probe must not error");
    let processing_rows = processing_probe.unwrap_or_default();
    let processing_leaks = processing_rows.len();
    assert!(
        processing_rows.is_empty(),
        "processing document probe leaked {processing_leaks} row(s)"
    );

    let empty_ids_probe = retriever
        .retrieve_filtered_scoped(
            tenant_id,
            &RetrievalQuery {
                query_text: "indemnification cap".to_string(),
                user_id: user_id.to_string(),
                document_ids: Some(vec![]),
                top_k: TOP_K,
                min_similarity: 0.0,
            },
            None,
            &|_: &SearchResult| true,
        )
        .await;
    assert!(
        empty_ids_probe.is_ok(),
        "empty document_ids probe must not error"
    );
    let empty_ids_rows = empty_ids_probe.unwrap_or_default();
    let empty_ids_leaks = empty_ids_rows.len();
    assert!(
        empty_ids_rows.is_empty(),
        "empty document_ids probe leaked {empty_ids_leaks} row(s)"
    );

    // Explicit Cloud cross-tenant isolation probes under (eval-ver01-tenant, eval-ver01-user).
    let other_tenant_probe = retriever
        .retrieve_scoped(
            tenant_id,
            &RetrievalQuery {
                query_text: "OTHER TENANT SECRET indemnification cap SIAC arbitration".to_string(),
                user_id: user_id.to_string(),
                document_ids: Some(vec!["doc-other-tenant-legal".to_string()]),
                top_k: TOP_K,
                min_similarity: 0.0,
            },
            None,
        )
        .await;
    assert!(
        other_tenant_probe.is_ok(),
        "other-tenant document_ids probe must not error"
    );
    let other_tenant_rows = other_tenant_probe.unwrap_or_default();
    let other_tenant_leaks = other_tenant_rows.len();

    let default_tenant_doc_probe = retriever
        .retrieve_scoped(
            tenant_id,
            &RetrievalQuery {
                query_text:
                    "CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731 internal cloud liability ceiling"
                        .to_string(),
                user_id: user_id.to_string(),
                document_ids: Some(vec!["doc-cloud-sentinel".to_string()]),
                top_k: TOP_K,
                min_similarity: 0.0,
            },
            None,
        )
        .await;
    assert!(
        default_tenant_doc_probe.is_ok(),
        "default-tenant document_ids cross-tenant probe must not error"
    );
    let default_tenant_doc_rows = default_tenant_doc_probe.unwrap_or_default();
    let default_tenant_doc_leaks = default_tenant_doc_rows.len();

    // 3) Unfiltered (document_ids: None) sentinel search under eval-ver01-tenant must not surface "default" tenant doc.
    //    Use production min_similarity=0.45 (and also verify a positive control under "default" tenant hits doc-cloud-sentinel).
    let sentinel_default_control = retriever
        .retrieve_scoped(
            "default",
            &RetrievalQuery {
                query_text: "CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731".to_string(),
                user_id: user_id.to_string(),
                document_ids: None,
                top_k: TOP_K,
                min_similarity: 0.45,
            },
            None,
        )
        .await;
    assert!(
        sentinel_default_control.as_ref().is_ok_and(|rows| rows
            .iter()
            .any(|r| r.chunk.document_id == "doc-cloud-sentinel")),
        "positive control under default tenant must find doc-cloud-sentinel"
    );

    let sentinel_unfiltered_probe = retriever
        .retrieve_scoped(
            tenant_id,
            &RetrievalQuery {
                query_text: "CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731".to_string(),
                user_id: user_id.to_string(),
                document_ids: None,
                top_k: TOP_K,
                min_similarity: 0.45,
            },
            None,
        )
        .await;
    assert!(
        sentinel_unfiltered_probe.is_ok(),
        "unfiltered sentinel cross-tenant probe must not error"
    );
    let sentinel_unfiltered_rows = sentinel_unfiltered_probe.unwrap_or_default();
    let sentinel_unfiltered_leaks = sentinel_unfiltered_rows.len();

    cloud_cross_doc_leaks += processing_leaks
        + empty_ids_leaks
        + other_tenant_leaks
        + default_tenant_doc_leaks
        + sentinel_unfiltered_leaks;

    assert!(
        other_tenant_rows.is_empty(),
        "other-tenant document_ids probe leaked {other_tenant_leaks} row(s)"
    );
    assert!(
        default_tenant_doc_rows.is_empty(),
        "default-tenant document_ids probe leaked {default_tenant_doc_leaks} row(s) across tenants"
    );
    assert!(
        sentinel_unfiltered_rows.is_empty(),
        "unfiltered sentinel probe leaked {sentinel_unfiltered_leaks} row(s) from default tenant"
    );

    let cloud_n = cloud_cases.len() as f64;
    let recall_at_5 = cloud_hits as f64 / cloud_n;
    let mrr = cloud_rr_sum / cloud_n;
    let leak_pct = if cloud_total_returned > 0 {
        100.0 * (cloud_cross_doc_leaks as f64) / (cloud_total_returned as f64)
    } else {
        0.0
    };

    CloudEvalMetrics {
        queries: cloud_cases.len(),
        recall_at_5,
        mrr,
        leak_pct,
        cross_doc_leaks: cloud_cross_doc_leaks,
        processing_leaks,
        empty_ids_leaks,
        other_tenant_leaks,
        default_tenant_doc_leaks,
        sentinel_unfiltered_leaks,
    }
}

/// Evaluates Own Data `LocalIndexStore` hybrid retrieval, extension/document scoping, and `ScenarioModeRouter` isolation.
async fn eval_own_data_local_rag(
    retriever: &DocumentRetriever,
    engine: &token_compress_engine::rag::embeddings::EmbeddingEngine,
    user_id: &str,
    legal_chunks: &[&str],
) -> OwnDataEvalMetrics {
    use token_compress_engine::rag::local::{
        LocalIndexStore, LocalRetrieveRequest, DEFAULT_INDEX_FILENAME,
    };
    use token_compress_engine::scenario::ScenarioDataSource;
    use token_compress_engine::types::{
        ChunkMetadata as PromptChunkMetadata, RagChunk as PromptRagChunk,
    };

    let temp_dir = std::env::temp_dir().join(format!("tce_ver01_own_{}", uuid::Uuid::new_v4()));
    assert!(
        std::fs::create_dir_all(&temp_dir).is_ok(),
        "create eval temp dir"
    );
    let _temp_guard = EvalTempDirGuard {
        path: temp_dir.clone(),
    };

    let legal_docx_bytes = build_eval_docx(legal_chunks);
    let hr_docx_bytes = build_eval_docx(&[
        "Employee business travel hotel lodging reimbursement is capped at 180 USD per night before local taxes.",
        "Full-time employees receive sixteen weeks of paid parental leave following birth or adoption placement.",
        "Annual tuition reimbursement for accredited degree or certification coursework is capped at 5250 USD.",
        "One-time ergonomic home office equipment stipend provides up to 1200 USD for desk, chair, and monitor.",
        "Paid bereavement leave grants up to ten business days upon loss of an immediate family member.",
        "Employer 401k retirement plan matches employee contributions dollar-for-dollar up to six percent of eligible salary.",
    ]);
    let security_pdf_bytes = build_eval_pdf(&[
        "External network transport mandates TLS 1.3 with AEAD cipher suites and strict HSTS preload headers.",
        "Privileged production console authentication requires phishing-resistant FIDO2 hardware MFA security keys.",
        "Customer KMS encryption keys undergo automated cryptographic rotation every ninety calendar days.",
        "Centralized SIEM security audit logs are retained in immutable WORM storage for three hundred sixty-five days.",
        "Critical P1 security incidents require executive notification and containment triage within four hours.",
        "Daily disaster recovery snapshots are replicated to an airgapped immutable vault tested monthly.",
    ]);

    assert!(
        std::fs::write(temp_dir.join("legal_handbook.docx"), &legal_docx_bytes).is_ok(),
        "write legal_handbook.docx"
    );
    assert!(
        std::fs::write(temp_dir.join("hr_benefits.docx"), &hr_docx_bytes).is_ok(),
        "write hr_benefits.docx"
    );
    assert!(
        std::fs::write(temp_dir.join("security_controls.pdf"), &security_pdf_bytes).is_ok(),
        "write security_controls.pdf"
    );

    let index_path = temp_dir.join(DEFAULT_INDEX_FILENAME);
    let mut local_store =
        LocalIndexStore::load_or_empty(&index_path).expect("load empty LocalIndexStore");

    let sync_summary = local_store
        .sync_folder(&temp_dir, &index_path, engine)
        .expect("sync_folder must succeed");
    assert_eq!(sync_summary.indexed, 3, "expected 3 local files indexed");
    assert_eq!(sync_summary.failed, 0, "expected 0 extraction failures");
    assert_eq!(
        local_store.total_chunks(),
        18,
        "expected 18 total local chunks"
    );

    // (query, gold_source_file, gold_page_number_1_based, scoped_extensions)
    let own_cases: &[(&str, &str, u32, &[&str])] = &[
        (
            "What is the indemnification liability cap and gross negligence exception?",
            "legal_handbook.docx",
            1,
            &[".docx"],
        ),
        (
            "How many days prior written notice are required for termination for convenience?",
            "legal_handbook.docx",
            2,
            &[".docx"],
        ),
        (
            "Which SIAC arbitration seat and number of arbitrators apply to disputes?",
            "legal_handbook.docx",
            3,
            &[".docx"],
        ),
        (
            "What is the nightly USD cap for employee business travel hotel lodging reimbursement?",
            "hr_benefits.docx",
            1,
            &[".docx"],
        ),
        (
            "How many weeks of paid parental leave do full-time employees receive?",
            "hr_benefits.docx",
            2,
            &[".docx"],
        ),
        (
            "What is the annual USD tuition reimbursement cap for accredited coursework?",
            "hr_benefits.docx",
            3,
            &[".docx"],
        ),
        (
            "Which TLS version and HSTS policy are mandated for external network transport?",
            "security_controls.pdf",
            1,
            &[".pdf"],
        ),
        (
            "What FIDO2 hardware token MFA requirement applies to privileged production access?",
            "security_controls.pdf",
            2,
            &[".pdf"],
        ),
        (
            "How often do customer KMS encryption keys undergo automated rotation?",
            "security_controls.pdf",
            3,
            &[".pdf"],
        ),
    ];

    let mut own_hits: usize = 0;
    let mut own_rr_sum: f64 = 0.0;
    let mut own_total_returned: usize = 0;
    let mut own_cross_ext_leaks: usize = 0;

    for &(query_text, gold_file, gold_page, scoped_exts) in own_cases {
        let req = LocalRetrieveRequest {
            query: query_text.to_string(),
            top_k: TOP_K,
            file_extensions: scoped_exts.iter().map(|s| (*s).to_string()).collect(),
        };
        let resp = local_store
            .retrieve(&req, engine)
            .expect("LocalIndexStore::retrieve must succeed");

        own_total_returned += resp.chunks.len();
        for c in &resp.chunks {
            if !scoped_exts.contains(&c.file_extension.as_str()) {
                own_cross_ext_leaks += 1;
            }
            if c.content
                .contains("CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731")
            {
                own_cross_ext_leaks += 1;
            }
        }
        if let Some(pos) = resp
            .chunks
            .iter()
            .position(|c| c.source_file == gold_file && c.page_number == gold_page)
        {
            own_hits += 1;
            own_rr_sum += 1.0 / (pos as f64 + 1.0);
        }
    }

    // Verify single-document scoping + ScenarioModeRouter with ScenarioDataSource::Own
    // never leaks Cloud SQLite chunks or unselected local documents.
    let domain_reg = ScenarioDomainRegistry::load_from_file("config/scenario/domains.json")
        .expect("load domains.json");
    let style_reg =
        ScenarioStyleRegistry::load_from_file("config/scenario/styles.json", "prompts/styles")
            .expect("load styles.json");
    let egress_guard = ScenarioEgressGuard::load_from_file("config/scenario/egress.json")
        .expect("load egress.json");

    let router = ScenarioModeRouter::new(
        Arc::new(domain_reg),
        Arc::new(style_reg),
        Arc::new(egress_guard),
        "prompts/styles".to_string(),
    );

    let own_route_cases: &[(&str, &str, &str)] = &[
        (
            "What is the indemnification liability cap and gross negligence exception?",
            "legal_handbook.docx",
            ".docx",
        ),
        (
            "What is the nightly USD cap for employee business travel hotel lodging reimbursement?",
            "hr_benefits.docx",
            ".docx",
        ),
        (
            "How often do customer KMS encryption keys undergo automated rotation?",
            "security_controls.pdf",
            ".pdf",
        ),
    ];

    let mut own_cross_source_leaks: usize = 0;
    for &(query_text, target_file, ext) in own_route_cases {
        let local_resp = local_store
            .retrieve(
                &LocalRetrieveRequest {
                    query: query_text.to_string(),
                    top_k: TOP_K,
                    file_extensions: vec![ext.to_string()],
                },
                engine,
            )
            .expect("local retrieve for router must succeed");

        // Apply single-document scope to simulate client selecting one local source file.
        let scoped_chunks: Vec<PromptRagChunk> = local_resp
            .chunks
            .into_iter()
            .filter(|c| c.source_file == target_file)
            .map(|c| PromptRagChunk {
                domain_tag: "local_own_data".to_string(),
                content: c.content,
                metadata: Some(PromptChunkMetadata {
                    page_number: Some(c.page_number),
                    source_file: c.source_file,
                }),
            })
            .collect();
        assert!(
            !scoped_chunks.is_empty(),
            "single-document scoped local chunks must be non-empty for {target_file}"
        );

        let routed_resp = router
            .route_and_execute_with_source(
                "Scenario",
                Some("legal"),
                None,
                None,
                query_text,
                user_id,
                "legal",
                None,
                retriever,
                None,
                ScenarioDataSource::Own {
                    local_chunks: scoped_chunks,
                },
            )
            .await
            .expect("ScenarioModeRouter Own Data must succeed");

        assert!(!routed_resp.is_empty_knowledge);
        assert_eq!(
            routed_resp.source_badge,
            "Source: own local folder only (127.0.0.1:49152)"
        );

        // Check zero cloud DB leakage & zero unselected local document leakage.
        if routed_resp
            .output_text
            .contains("CLOUD_DB_ONLY_SENTINEL_DO_NOT_LEAK_7731")
            || routed_resp
                .output_text
                .contains("cloud_sentinel_policy.pdf")
        {
            own_cross_source_leaks += 1;
        }
        for cit in &routed_resp.citations {
            if !cit.contains(target_file) {
                own_cross_source_leaks += 1;
            }
        }
    }

    let own_n = own_cases.len() as f64;
    let recall_at_5 = own_hits as f64 / own_n;
    let mrr = own_rr_sum / own_n;
    let total_leaks = own_cross_ext_leaks + own_cross_source_leaks;
    let leak_pct = if own_total_returned > 0 {
        100.0 * (total_leaks as f64) / (own_total_returned as f64)
    } else {
        0.0
    };

    OwnDataEvalMetrics {
        queries: own_cases.len(),
        recall_at_5,
        mrr,
        leak_pct,
        total_leaks,
        cross_ext_leaks: own_cross_ext_leaks,
        cross_source_leaks: own_cross_source_leaks,
    }
}

/// VER-01 self-contained retrieval evaluation for `document_ids` filtering on Cloud
/// (`RagStore` + `DocumentRetriever`) and Own Data (`LocalIndexStore` + `ScenarioModeRouter`).
///
/// Targets (`docs/PRINCIPAL_AUDIT_TODO_2026_11.md` VER-01):
/// - `Recall@5 >= 0.88`
/// - `MRR >= 0.75`
/// - `Cross-Doc Leak = 0%` and `Cross-Source Leak = 0%`
#[tokio::test]
async fn eval_document_ids_cloud_and_own_data_recall() {
    use token_compress_engine::rag::embeddings::EmbeddingEngine;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory SQLite pool must open");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations must apply cleanly");

    let tenant_id = "eval-ver01-tenant";
    let user_id = "eval-ver01-user";
    let other_tenant = "eval-other-tenant";
    let other_user = "eval-other-user";

    for (uid, tid) in [(user_id, tenant_id), (other_user, other_tenant)] {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, email, tenant_id) VALUES ($1, $1, 'x', $2, $3)",
        )
        .bind(uid)
        .bind(format!("{uid}@eval.local"))
        .bind(tid)
        .execute(&pool)
        .await
        .expect("test user insert must succeed");
    }

    let engine = EmbeddingEngine::new();
    let store = RagStore::new(pool);
    let retriever = DocumentRetriever::new(store.clone());

    let legal_chunks = [
        "Indemnification liability cap is strictly limited to twelve months of paid subscription fees, excluding gross negligence or willful misconduct.",
        "Termination for convenience requires sixty calendar days prior written notice, while material breach allows a thirty day written cure window.",
        "Any contractual dispute shall be resolved by binding arbitration seated in Singapore under SIAC rules before three neutral arbitrators.",
        "Mutual confidentiality obligations regarding proprietary source code and pricing schedules survive termination for five years from disclosure.",
        "Background intellectual property remains the sole property of the licensor, whereas custom deliverables vest in the client upon full invoice settlement.",
        "Force majeure events including earthquakes, sovereign embargoes, or submarine cable cuts excuse delayed performance beyond fifteen days.",
    ];

    let cloud = eval_cloud_document_ids(
        &store,
        &retriever,
        &engine,
        CloudEvalScope {
            tenant_id,
            user_id,
            other_tenant,
            other_user,
            legal_chunks: &legal_chunks,
        },
    )
    .await;

    let own = eval_own_data_local_rag(&retriever, &engine, user_id, &legal_chunks).await;

    println!("\n=== VER-01 Document-Scoped Retrieval Evaluation (Cloud + Own Data) ===");
    println!(
        "{:<35} | {:>7} | {:>8} | {:>7} | {:>14}",
        "Index / Scope", "Queries", "Recall@5", "MRR", "Cross-Doc Leak"
    );
    println!("{:-<83}", "");
    println!(
        "{:<35} | {:>7} | {:>8.3} | {:>7.3} | {:>13.1}%",
        "Cloud (document_ids + status=ready)",
        cloud.queries,
        cloud.recall_at_5,
        cloud.mrr,
        cloud.leak_pct
    );
    println!(
        "{:<35} | {:>7} | {:>8.3} | {:>7.3} | {:>13.1}%",
        "Own Data (LocalIndexStore + Router)", own.queries, own.recall_at_5, own.mrr, own.leak_pct
    );
    println!("{:-<83}", "");
    println!(
        "Cloud probes: processing_leaks={}, empty_ids_leaks={}, other_tenant_leaks={}, default_tenant_doc_leaks={}, sentinel_unfiltered_leaks={}, cross_doc_leaks={}",
        cloud.processing_leaks,
        cloud.empty_ids_leaks,
        cloud.other_tenant_leaks,
        cloud.default_tenant_doc_leaks,
        cloud.sentinel_unfiltered_leaks,
        cloud.cross_doc_leaks
    );
    println!(
        "Own Data probes: cross_ext_leaks={}, cross_source_leaks={}, neural_embeddings={}",
        own.cross_ext_leaks,
        own.cross_source_leaks,
        engine.is_neural()
    );

    assert!(
        cloud.recall_at_5 >= 0.88,
        "Cloud Recall@5 ({:.3}) must be >= 0.88",
        cloud.recall_at_5
    );
    assert!(
        cloud.mrr >= 0.75,
        "Cloud MRR ({:.3}) must be >= 0.75",
        cloud.mrr
    );
    assert_eq!(
        cloud.cross_doc_leaks, 0,
        "Cloud cross-doc/non-ready/cross-tenant leaks must be 0"
    );

    assert!(
        own.recall_at_5 >= 0.88,
        "Own Data Recall@5 ({:.3}) must be >= 0.88",
        own.recall_at_5
    );
    assert!(
        own.mrr >= 0.75,
        "Own Data MRR ({:.3}) must be >= 0.75",
        own.mrr
    );
    assert_eq!(
        own.total_leaks, 0,
        "Own Data cross-extension and cross-source leaks must be 0"
    );
}
