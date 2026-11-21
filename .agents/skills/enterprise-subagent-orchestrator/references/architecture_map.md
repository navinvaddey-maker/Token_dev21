# `token_compress_engine` — Enterprise Architecture Reference Map

> Canonical source: [ARCHITECTURE.md](file:///Users/navinvaddey/token_compress_engine/ARCHITECTURE.md) (v3.8.0)

## 1. Workspace & Crate Topology
- Root package (`token_compress_engine`, Rust `edition = "2021"`):
  - Web server binary: `src/main.rs`
  - Local Offline RAG Companion binary: `src/bin/rag.rs` (`127.0.0.1:49152`, `--stdin` JSON IPC)
- Internal workspace crates (`Cargo.toml`):
  - `crates/schema-engine`
  - `crates/hebbian-core`
  - `crates/competitive-core`
  - `crates/db`
  - `crates/learning-engine` (note: currently specifies `sqlx` `postgres` + `runtime-tokio-native-tls` features vs root `sqlite` + `runtime-tokio-rustls`)
- Workspace compiler/clippy lints (`Cargo.toml`):
  - `unsafe_code = "forbid"`
  - `clippy::todo = "deny"`, `clippy::dbg_macro = "deny"`
  - `clippy::unwrap_used = "warn"`, `clippy::expect_used = "warn"`, `clippy::panic = "warn"` (enforced as errors by `cargo clippy ... -- -D warnings`)
- Standalone root utility scripts (outside the Rust service runtime): `download_model.py`, `fix_migrations.py`, `generate_architecture_pdf.py`.

## 2. Enterprise Layers & Key Modules

| Enterprise Layer | Key Files & Symbols | Responsibilities & Invariants |
|---|---|---|
| **Presentation & Local IPC** | `static/{index.html,admin.html,app.js,admin.js,rag_workspace.js,mock_api.js}`, `src/bin/rag.rs`, `src/rag/local.rs` | SPA UI (`mock_api.js` provides `API` gateway) + exclusive Ask RAG toggle (`"cloud"` vs `"own"`). Own Data keeps `.pdf`/`.docx` local via `127.0.0.1:49152/retrieve`. XSS-safe `escapeHtml`. |
| **API Boundary & Security** | `src/api.rs`, `src/main.rs`, `src/errors.rs` | Axum router, `build_cors_layer()` (`ALLOWED_ORIGINS`), security headers, 60s global middleware + 15s handler (30s/65s upload) timeouts, `DefaultBodyLimit`, `auth_middleware` (DB role/tenant reload), `RateLimiter` (`DashMap`, `MAX_RATE_BUCKETS=50_000`), `AppError` (`E001`–`E010`, `E099`, `error_id` UUID). |
| **Auth, Tenancy & State** | `src/domain.rs`, `src/lib.rs` (`AppState`) | HS256 JWT (`iss`, `aud`, `>=32B` `JWT_SECRET` `OnceLock`), `bcrypt` in `spawn_blocking`, `/api/admin/*` tenant isolation, `AppState` (`pool`, `learning_pool`, `pipeline`, `npae_config`, `ory_engine`, `rag_store`, `sessions`, `embedding_engine`, `scenario_router`, `pdf_ingest_sem`, `aggressive_sem`, `compress_sem`). |
| **Scenario & Classification** | `src/classifier/`, `src/scenario/`, `src/npae/aggressive/{intent,role}.rs` | 5-layer Intent × Vertical composition + 3-Axis Intent Tensor ($D \times P \times I$, `ActorPerspective`), `ScenarioClassifier`, `ScenarioModeRouter`. |
| **Canonical Compression Pipeline** | `src/pipeline/orchestrator.rs`, `src/engine/reconstruction/`, `stage0..stage6b`, `src/algorithms/`, `src/scoring/`, `src/npae/` | `PipelineOrchestrator` is canonical (`src/engine/pipeline.rs` is deprecated). Stages `-1 → 0A → 0B → 1 → 2` then Balanced/Gentle (`3 → 4 → 4B → 5 → 6A`) or Aggressive NPAE (`compression::pipeline`, `AggressiveEngine`, `structurer/`, `ory/`). Unified `HallucinationGuard`, `TES`/`SFS`/`SCS`, Stage `6B`, plus `src/npae/grounding/verifier.rs` (`POST /api/npae/verify`). |
| **RAG, Embeddings & Fencing** | `src/rag/{embeddings,ingest,store,retriever,local,types}.rs`, `src/types.rs` (`EnrichmentContext`), `structurer/render.rs`, `scenario/router.rs`, `stage6a.rs` | `EmbeddingEngine` (`all-MiniLM-L6-v2` 384-dim + deterministic fallback), SQLite FTS5 + dense cosine RRF (`k=60.0`), untrusted XML fencing (`<facts trust="untrusted">`, `<context trust="untrusted">`, `<retrieved_context trust="untrusted">`). |
| **Persistence & Config** | `src/data.rs` (`Repository`), `src/db/schema_registry.rs`, `crates/db/`, `migrations/0001..0016`, `src/npae/aggressive/config.rs`, `config/{unified.json,scenario/*.json}` | `SqlitePoolOptions` (min 5, max 20), `Repository` (core CRUD + tenant-scoped analytics), `learned_patterns` table, `ConfigHandle` (`O(1)` `DomainIndex`), `SchemaRegistry` in-memory 3-tier lookup. |

## 3. Quality & Review Gates (`.agents/workflows/review.md`)
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace`
4. `cargo deny check`
5. `gitleaks detect --no-banner`
6. `semgrep scan --config p/rust --config p/owasp-top-ten`
7. If migrations changed and `squawk` is installed: `squawk migrations/*.sql`
8. When SQL queries/schema change: `cargo sqlx prepare` + `SQLX_OFFLINE=true cargo check` (per `nrule.md` §7 / `rust-standards.md`)
