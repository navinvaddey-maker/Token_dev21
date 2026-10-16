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

use std::collections::HashSet;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use token_compress_engine::rag::retriever::DocumentRetriever;
use token_compress_engine::rag::store::RagStore;
use token_compress_engine::rag::types::{RetrievalQuery, SearchResult};
use token_compress_engine::scenario::ScenarioNamespaceGuard;

const SRC_DB: &str = "token_compress.db";
const COPY_DB: &str = "target/eval_scenario_copy.db";
const SOURCE_FILE: &str = "legal_docs_india.pdf";
const OWNER: &str = "ed1b56f8-a75d-436d-9f50-be53128ccbd5";
const TOP_K: usize = 5;

/// (question, gold chunk_index within `legal_docs_india.pdf`).
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
/// "lex_filter ON" rows above); the stop-word list and stem rule are copied from guards.rs.
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
