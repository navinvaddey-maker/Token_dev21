//! Local offline-first RAG companion binary (`src/bin/rag.rs` -> `rag` / `rag.exe`).
//!
//! Implements `ARCHITECTURE.md` §2.1 (Own Data mode):
//! - Watches a local directory for `.pdf` and `.docx` documents without uploading raw files.
//! - Runs incremental SHA-256 indexing and local `all-MiniLM-L6-v2` + BM25 hybrid retrieval.
//! - Serves `GET /health`, `POST /retrieve`, and `POST /reindex` on `127.0.0.1:49152`
//!   or processes strict JSON requests via `--stdin`.

use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use token_compress_engine::rag::embeddings::EmbeddingEngine;
use token_compress_engine::rag::local::{
    LocalHealthResponse, LocalIndexStore, LocalRagError, LocalRetrieveRequest,
    DEFAULT_INDEX_FILENAME,
};
use token_compress_engine::utils::sync::OryxSyncGuard;
use tower_http::cors::{AllowOrigin, CorsLayer};

const DEFAULT_PORT: u16 = 49152;
const DEFAULT_POLL_SECS: u64 = 3;

#[derive(Debug, Clone)]
struct CompanionConfig {
    watch_dir: PathBuf,
    index_path: PathBuf,
    port: u16,
    poll_secs: u64,
    stdin_mode: bool,
}

#[derive(Clone)]
struct CompanionState {
    watch_dir: PathBuf,
    index_path: PathBuf,
    store: Arc<RwLock<LocalIndexStore>>,
    embeddings: EmbeddingEngine,
}

fn parse_args() -> Result<CompanionConfig> {
    let mut watch_dir = std::env::var("RAG_WATCH_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./rag_docs"));

    let mut index_path: Option<PathBuf> = std::env::var("RAG_INDEX_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);

    let mut port: u16 = std::env::var("RAG_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let mut poll_secs: u64 = DEFAULT_POLL_SECS;
    let mut stdin_mode = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--watch" => {
                let val = args
                    .next()
                    .context("--watch requires a directory path argument")?;
                watch_dir = PathBuf::from(val);
            }
            "--index" => {
                let val = args
                    .next()
                    .context("--index requires a file path argument")?;
                index_path = Some(PathBuf::from(val));
            }
            "--port" => {
                let val = args.next().context("--port requires a port number")?;
                port = val
                    .parse::<u16>()
                    .with_context(|| format!("invalid port '{val}'"))?;
            }
            "--poll-secs" => {
                let val = args
                    .next()
                    .context("--poll-secs requires an integer value")?;
                poll_secs = val
                    .parse::<u64>()
                    .with_context(|| format!("invalid poll interval '{val}'"))?
                    .max(1);
            }
            "--stdin" => {
                stdin_mode = true;
            }
            "--help" | "-h" => {
                println!(
                    "rag — Local Offline-First RAG Companion\n\n\
                     OPTIONS:\n  \
                       --watch <DIR>       Folder to watch for .pdf/.docx (default: ./rag_docs)\n  \
                       --index <FILE>      Local index JSON path (default: <WATCH>/.rag_local_index.json)\n  \
                       --port <PORT>       Loopback HTTP port (default: 49152)\n  \
                       --poll-secs <SECS>  Incremental scan interval (default: 3)\n  \
                       --stdin             Read JSON requests from stdin and write JSON to stdout"
                );
                std::process::exit(0);
            }
            other => {
                anyhow::bail!("unknown argument '{other}'; pass --help for usage");
            }
        }
    }

    let resolved_index = index_path.unwrap_or_else(|| watch_dir.join(DEFAULT_INDEX_FILENAME));
    Ok(CompanionConfig {
        watch_dir,
        index_path: resolved_index,
        port,
        poll_secs,
        stdin_mode,
    })
}

fn is_local_origin(origin: &HeaderValue) -> bool {
    let Ok(s) = origin.to_str() else {
        return false;
    };
    s.starts_with("http://127.0.0.1")
        || s.starts_with("http://localhost")
        || s.starts_with("https://127.0.0.1")
        || s.starts_with("https://localhost")
}

fn sync_blocking(
    watch_dir: &Path,
    index_path: &Path,
    store_lock: &Arc<RwLock<LocalIndexStore>>,
    embeddings: &EmbeddingEngine,
) -> Result<token_compress_engine::rag::local::SyncSummary, String> {
    // Clone or operate on a working copy outside the lock so readers never stall during ONNX embedding.
    let mut working_store = {
        let guard = OryxSyncGuard::read_rwlock_recovering_poison(store_lock);
        guard.clone()
    };

    let summary = working_store
        .sync_folder(watch_dir, index_path, embeddings)
        .map_err(|e| e.to_string())?;

    if summary.indexed > 0 || summary.removed > 0 {
        let mut guard = OryxSyncGuard::write_rwlock_recovering_poison(store_lock);
        *guard = working_store;
    }

    Ok(summary)
}

fn build_health(state: &CompanionState) -> LocalHealthResponse {
    let guard = OryxSyncGuard::read_rwlock_recovering_poison(&state.store);
    LocalHealthResponse {
        status: "ok".to_string(),
        watched_dir: state.watch_dir.to_string_lossy().to_string(),
        indexed_files: guard.files.len(),
        indexed_chunks: guard.total_chunks(),
        neural_embeddings: state.embeddings.is_neural(),
        extensions: vec![".pdf".to_string(), ".docx".to_string()],
    }
}

async fn health_handler(State(state): State<CompanionState>) -> Json<LocalHealthResponse> {
    Json(build_health(&state))
}

async fn reindex_handler(State(state): State<CompanionState>) -> Response {
    let watch_dir = state.watch_dir.clone();
    let index_path = state.index_path.clone();
    let store = Arc::clone(&state.store);
    let embeddings = state.embeddings;

    let res = tokio::task::spawn_blocking(move || {
        sync_blocking(&watch_dir, &index_path, &store, &embeddings)
    })
    .await;

    match res {
        Ok(Ok(_summary)) => (StatusCode::OK, Json(build_health(&state))).into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "error": err })),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "error": join_err.to_string() })),
        )
            .into_response(),
    }
}

async fn retrieve_handler(
    State(state): State<CompanionState>,
    Json(req): Json<LocalRetrieveRequest>,
) -> Response {
    let store = Arc::clone(&state.store);
    let embeddings = state.embeddings;

    // ARCH: §6 Async Rules — run ONNX query embedding & scoring in spawn_blocking.
    let res = tokio::task::spawn_blocking(move || {
        let guard = OryxSyncGuard::read_rwlock_recovering_poison(&store);
        guard.retrieve(&req, &embeddings)
    })
    .await;

    match res {
        Ok(Ok(resp)) => (StatusCode::OK, Json(resp)).into_response(),
        Ok(Err(LocalRagError::Validation(msg))) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "error": msg })),
        )
            .into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "error": err.to_string() })),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "error": join_err.to_string() })),
        )
            .into_response(),
    }
}

fn run_stdin_loop(state: &CompanionState) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_res in stdin.lock().lines() {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let output = match serde_json::from_str::<LocalRetrieveRequest>(trimmed) {
            Ok(req) => {
                let guard = OryxSyncGuard::read_rwlock_recovering_poison(&state.store);
                match guard.retrieve(&req, &state.embeddings) {
                    Ok(resp) => serde_json::to_string(&resp)?,
                    Err(err) => serde_json::to_string(
                        &json!({ "status": "error", "error": err.to_string() }),
                    )?,
                }
            }
            Err(err) => serde_json::to_string(
                &json!({ "status": "error", "error": format!("invalid request JSON: {err}") }),
            )?,
        };
        writeln!(stdout, "{output}")?;
        stdout.flush()?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = parse_args()?;
    let embeddings = EmbeddingEngine::new();
    let initial_store = LocalIndexStore::load_or_empty(&cfg.index_path)
        .with_context(|| format!("failed to load local index at {}", cfg.index_path.display()))?;
    let store_lock = Arc::new(RwLock::new(initial_store));

    // Perform initial incremental scan before serving requests.
    let initial_summary = sync_blocking(&cfg.watch_dir, &cfg.index_path, &store_lock, &embeddings)
        .map_err(|e| anyhow::anyhow!("initial folder sync failed: {e}"))?;

    tracing::info!(
        watch_dir = %cfg.watch_dir.display(),
        indexed = initial_summary.indexed,
        skipped = initial_summary.skipped_unchanged,
        removed = initial_summary.removed,
        failed = initial_summary.failed,
        neural = embeddings.is_neural(),
        "Local offline RAG index synchronized"
    );

    let state = CompanionState {
        watch_dir: cfg.watch_dir.clone(),
        index_path: cfg.index_path.clone(),
        store: Arc::clone(&store_lock),
        embeddings,
    };

    if cfg.stdin_mode {
        return tokio::task::spawn_blocking(move || run_stdin_loop(&state))
            .await
            .context("stdin worker failed")?;
    }

    // Background periodic folder watcher using SHA-256 incremental sync.
    let watcher_state = state.clone();
    let poll_interval = Duration::from_secs(cfg.poll_secs);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(poll_interval);
        ticker.tick().await; // skip immediate tick right after startup sync
        loop {
            ticker.tick().await;
            let watch_dir = watcher_state.watch_dir.clone();
            let index_path = watcher_state.index_path.clone();
            let store = Arc::clone(&watcher_state.store);
            let emb = watcher_state.embeddings;
            match tokio::task::spawn_blocking(move || {
                sync_blocking(&watch_dir, &index_path, &store, &emb)
            })
            .await
            {
                Ok(Ok(summary)) if summary.indexed > 0 || summary.removed > 0 => {
                    tracing::info!(
                        indexed = summary.indexed,
                        removed = summary.removed,
                        skipped = summary.skipped_unchanged,
                        "Incremental local folder sync updated index"
                    );
                }
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, "Periodic local folder sync failed");
                }
                Err(join_err) => {
                    tracing::warn!(error = %join_err, "Periodic sync task panicked");
                }
                _ => {}
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| is_local_origin(origin)))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/retrieve", post(retrieve_handler))
        .route("/reindex", post(reindex_handler))
        .with_state(state)
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], cfg.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind local companion server to {addr}"))?;

    tracing::info!(
        url = %format!("http://{addr}"),
        watch_dir = %cfg.watch_dir.display(),
        "Local offline RAG companion listening (loopback only)"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("local companion HTTP server failed")?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Shutting down local offline RAG companion");
}
