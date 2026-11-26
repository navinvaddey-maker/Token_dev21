# AGENTS.md — Repository Orientation for Antigravity & Engineers

Welcome to `token_compress_engine`. This document serves as the high-signal orientation guide for human engineers and autonomous Antigravity agents.

## 1. Core Architecture & Canonical Engine
- **Canonical Engine**: `PipelineOrchestrator` (`src/pipeline/orchestrator.rs`) handles all compression pipeline execution across modes.
- **Legacy v1**: `src/engine/{pipeline,sparse_coding,predictive_coding,working_memory,selective_attention,chunking}.rs` are retained exclusively for historical regression tests and must not be used for new development.
- **Subroutines**: `src/engine/neuro/` provides internal topology routing used by Stage 1.

## 2. Module Map
- `src/api/` — Thin Axum request handlers, middleware, routing, and error mapping (zero direct SQL queries).
- `src/domain/` — Pure business logic, use cases, and validation (zero SQL or Axum dependencies).
- `src/data/` — Persistence layer (`Repository` implementing SQLite/sqlx interactions).
- `src/utils/` — Shared infrastructure utilities prefixed with `Oryx*` (`OryxTokenEstimator`, `OryxTextMatcher`, `OryxVectorMath`, `OryxHasher`, `OryxSecuritySanitizer`, `OryxSyncGuard`).
- `src/pipeline/`, `src/algorithms/`, `src/scoring/`, `src/npae/`, `src/rag/`, `src/scenario/`, `src/classifier/` — Core processing engines.
- `src/bin/{rag,tce}.rs` — Binary entrypoints (`rag.rs` runs the local offline RAG companion on `127.0.0.1:49152`).
- `crates/*` — Modular internal workspace crates (`schema-engine`, `hebbian-core`, `competitive-core`, `db`, `learning-engine`).
- `static/*` — SPA frontend assets (`mock_api.js`, `app.js`, `admin.js`, `rag_workspace.js` using `OryxDomUtils`).

## 3. Naming Discipline & Conventions
- **Shared Utility Structs (`src/utils/*`) & JS Helpers (`static/*.js`)**: Must use the `Oryx*` prefix (e.g., `OryxTokenEstimator`, `OryxDomUtils`).
- **Existing Rust Domain/Engine Structs & SQL Identifiers**: Preserve existing names (`snake_case` JSON keys and SQL columns).
- **Methods**: Use intention-revealing `verb_noun[_scoped]` names (e.g., `fetch_chunks_scoped` for tenant-isolated queries).

## 4. Pre-Flight & Verification Gates
Before making any changes or committing code, consult:
- `ARCHITECTURE.md`
- `CLAUDE.md`
- `.agents/rules/*.md`
- `./rust-env.sh`
- `.agents/workflows/review.md`

Run verification gates:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `node tests/verify_frontend_ux.js`
- `cargo deny check`
- `gitleaks detect --no-banner`
- `semgrep scan --config p/rust --config p/owasp-top-ten`
