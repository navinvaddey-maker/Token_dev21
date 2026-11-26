---
trigger: always_on
---

# Enterprise Architecture & Tiered Subagent Workflow (Always Active)

These rules complement [.agents/rules/nrule.md](file:///Users/navinvaddey/token_compress_engine/.agents/rules/nrule.md) and [.agents/rules/rust-standards.md](file:///Users/navinvaddey/token_compress_engine/.agents/rules/rust-standards.md). Precedence follows `nrule.md` §0 (`nrule.md` > `rust-standards.md` / `enterprise-workflow.md` unless explicitly overridden by `ARCHITECTURE.md` or Navin).

---

## 1. Mandatory Input Protocol (Every Request from Navin)

For any task involving architecture, code, config, schema, or execution:

1. **Enterprise Architecture Analysis**:
   - Re-read `ARCHITECTURE.md` (per `nrule.md` §1) and map the request across the enterprise layers:
     - **Presentation & Local Companion**: `static/{index.html,admin.html,app.js,admin.js,rag_workspace.js,mock_api.js}` and `src/bin/rag.rs` (`127.0.0.1:49152` Own Data IPC)
     - **API & Security Boundary**: `src/api/`, `src/main.rs`, `src/errors.rs` (CORS, security headers, 60s global + 15s handler / 30s–65s upload timeouts, `DefaultBodyLimit`, `auth_middleware`, `RateLimiter`)
     - **Domain & Scenario Layer**: `src/domain/`, `src/scenario/`, `src/classifier/`, `src/npae/aggressive/{intent,role}.rs` (3-Axis Intent Tensor $D \times P \times I$)
     - **Pipeline, NPAE & Ory**: `src/pipeline/orchestrator.rs` (canonical; `src/engine/pipeline.rs` is deprecated), Stages `-1..6B`, `src/algorithms/`, `src/scoring/`, `src/npae/`, `src/npae/grounding/verifier.rs`
     - **RAG & Embeddings**: `src/rag/{embeddings,ingest,store,retriever,local,types}.rs`, `EnrichmentContext` (`src/types.rs`), untrusted XML fencing (`structurer/render.rs`, `scenario/router.rs`, `stage6a.rs`)
     - **Persistence & Config**: `src/data/`, `src/db/schema_registry.rs`, `crates/db/`, `migrations/`, `src/npae/aggressive/config.rs` (`ConfigHandle` / `DomainIndex`), `config/{unified.json,scenario/*.json}`
2. **Research First**:
   - Inspect real files, callers, `Cargo.toml`, `Cargo.lock`, and `migrations/` before proposing changes.
   - Surface architectural fit, security/multi-tenancy boundaries (`tenant_id`, `user_id`), concurrency/blocking risks, and any drift against `ARCHITECTURE.md` or `rust-standards.md` (per `nrule.md` §1.4, STOP and flag contradictions).
3. **Collaborate with Navin & Approval Gate**:
   - Address **Navin** by name with your research, enterprise impact, proposed plan, and every open question.
   - **STOP before editing files, running mutating commands, or spawning execution subagents** until **Navin** explicitly approves the plan.
   - *(Exception: pure read-only factual questions about existing code may be answered directly after reading the real code, without spawning execution subagents.)*

---

## 2. Coding Disciplines, Naming Conventions & Error Catching

### 2.1 Naming Conventions
- **Rust (`src/`, `crates/`)**: `snake_case` modules, functions, variables; `PascalCase` structs, enums, traits; `SCREAMING_SNAKE_CASE` constants. `Cargo.toml` forbids `unsafe_code` and denies `todo`/`dbg_macro`; `unwrap_used`/`expect_used`/`panic` are `warn` and enforced as errors via `cargo clippy ... -- -D warnings`.
- **New Shared Utility Structs (`src/utils/*`)**: Must use the `Oryx*` prefix (e.g. `OryxTokenEstimator`, `OryxTextMatcher`, `OryxVectorMath`, `OryxHasher`, `OryxSecuritySanitizer`, `OryxSyncGuard`).
- **Existing Rust Domain/Engine Structs & SQL Identifiers**: Preserve existing names (`snake_case` JSON wire keys and SQL columns).
- **Methods**: Use intention-revealing `verb_noun` or `verb_noun_scoped` names rather than vague verbs (`check`, `handle`, `do_work`).
- **SQL & Migrations (`migrations/`)**: `NNNN_snake_case_description.sql`, `snake_case` tables/columns/indexes, forward-only migrations.
- **Frontend JS (`static/*.js`)**: `camelCase` functions/variables, `UPPER_SNAKE_CASE` constants, kebab-case/BEM CSS classes. New helper classes must use the `Oryx*` prefix (e.g., `OryxDomUtils`).
- **Python Utility Scripts (`*.py`)**: `snake_case` functions/variables, `PascalCase` classes, `UPPER_SNAKE_CASE` constants.
- **JSON / API Payloads**: `snake_case` fields matching `src/api/` and `src/types.rs`.

### 2.2 Exception & Error Catching (Zero Silent Failures)
- **Rust**: Use `Result<T, E>` + `?` propagation everywhere. Prefer `thiserror` (`AppError` in `src/errors.rs`) in library/domain modules and `anyhow` only at binary edges (`src/main.rs`, `src/bin/rag.rs`). Preserve context; sanitize DB/internal/engine errors behind `error_id` correlation UUIDs at the HTTP boundary.
- **JavaScript (`static/*.js`)**: Wrap all `fetch`/async flows in explicit `try { ... } catch (err) { ... }`, validate `res.ok`, escape rendered HTML (`escapeHtml`), never swallow errors in empty `catch` blocks, and never log secrets/tokens.
- **Python (`*.py`)**: Catch specific exceptions (`try: ... except ValueError as exc:`), never bare `except:`, log actionable context, and exit non-zero on failure.

### 2.3 Documentation & Traceability (`nrule.md` §5)
- Every `pub` Rust item and non-trivial function requires `///` doc comments (`# Arguments` where non-obvious, `# Errors`, `# Panics`, `# Safety` if applicable, and `Invariants`).
- Modules require `//!` headers referencing the `ARCHITECTURE.md` section they implement, plus `// ARCH:`, `// FIX:`, `// TODO(owner):`, and `// UNVERIFIED:` tags where applicable.

---

## 3. Tiered Custom Subagent Execution Policy (Post-Approval Only)

After **Navin** approves the plan, orchestrate work using `.agents/skills/enterprise-subagent-orchestrator/SKILL.md`:

| Phase | Subagent Name | `invoke_subagent` `Model` | Write Tools |
|---|---|---|---|
| **Planning** | `ea-architect-planner` | **High** (`pro`) | `false` |
| **Code Execution & Tests** | `code-executor` | **Medium** (`flash`) | `true` |
| **Surgical Editing** | `surgical-editor` | **Low** (`flash_lite`) | `true` |
| **Final Review** | `ea-reviewer` | **High** (`pro`) | `false` |

- **Editing vs Execution Split**: `surgical-editor` (`flash_lite`) executes tightly scoped, spec-driven edits (single-block changes, doc comments, naming cleanups, config/markdown/UI updates). `code-executor` (`flash`) handles multi-file Rust logic and runs the full `.agents/workflows/review.md` gates (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `gitleaks detect --no-banner`, `semgrep scan --config p/rust --config p/owasp-top-ten`, plus `squawk migrations/*.sql` and `SQLX_OFFLINE=true cargo check` when applicable).
- **Final Gate**: Every change set must pass `ea-reviewer` (`pro`) and all applicable `.agents/workflows/review.md` gates after the final edit before reporting completion to Navin.
