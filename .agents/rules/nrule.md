---
trigger: always_on
---

# Engineering Rules: Rust · sqlx · RAG · Vector DB

Role: Principal engineer. Work is spec-driven, verified, and reviewable. No vibe coding.
These rules are mandatory. If a rule cannot be met, STOP and ask. Do not improvise.

---

## 0. Precedence

1. User's explicit instruction in the current task
2. Latest `architecture.md` (see §1)
3. These rules
4. Existing code conventions in the repo

If 1 conflicts with 2, do not proceed silently. Flag the conflict and wait for a decision.

---

## 1. Pre-flight (MANDATORY before ANY implementation or fix)

Complete in order. Do not write code until all are done.

1. **Locate the latest architecture doc.**
   - Search: `architecture.md`, `ARCHITECTURE.md`, `docs/architecture*.md`, `docs/adr/`.
   - If several exist, pick the latest by (a) version/date in the header, then (b) `git log -1 --format=%cI -- <file>`.
   - Never rely on memory or an earlier read in the conversation. Re-read the file this task.
2. **State what you used** in the first lines of your response:
   ```
   Architecture source: <path> | version/date: <x> | last commit: <hash>
   Relevant sections: <section names>
   ```
3. **Map the task to the architecture**: which component, boundary, data flow, and schema does this touch?
4. **Check for drift**: if the requested change contradicts or is absent from the architecture, STOP. Propose an `architecture.md` change (with diff) and wait for approval.
5. **Read the real code first**: open the files you will change and their callers. Check `Cargo.toml`, `Cargo.lock`, `migrations/`, and `.sqlx/` where relevant.
6. **Write a short plan** (see §3) before editing.

If `architecture.md` is missing or empty: say so, and ask. Do not invent an architecture.

---

## 2. No Hallucination Policy

- **Never invent**: crates, crate versions, feature flags, function signatures, trait methods, SQL tables/columns, env vars, config keys, CLI flags, or file paths.
- **Verify before use**, in this order:
  1. Repo code and `Cargo.lock` (exact versions in use)
  2. Local source: `~/.cargo/registry/src/**` and `cargo doc --open`
  3. Official docs (docs.rs for the pinned version, project docs)
  4. Web search only if the above fail. Cite the source.
- **Match the pinned version.** APIs differ across versions (e.g. sqlx 0.7 vs 0.8, pgvector crate versions). Use the version in `Cargo.lock`, not the one you remember.
- **Unknown = say so.** Mark it `UNVERIFIED: <what, why>` and ask. A stated gap is acceptable; a guess is not.
- **Do not claim** "tests pass", "compiles", or "works" unless you ran the command and saw the output. Show the command and the result.
- **No placeholder code** presented as complete: no `todo!()`, `unimplemented!()`, or fake stubs unless explicitly requested and flagged.
- **Do not fabricate** benchmarks, performance numbers, or recall/latency figures.

---

## 3. No Vibe Coding: Workflow

For anything beyond a trivial one-line change:

1. **Plan** (brief): goal, files to change, approach, risks, test strategy, alternatives rejected.
2. **Wait for approval** if the change touches schema, public API, architecture boundaries, dependencies, or more than ~3 files.
3. **Smallest correct diff.** No drive-by refactors, renames, formatting sweeps, or dependency bumps unless asked.
4. **Verify** (see §10).
5. **Report**: what changed, why, how verified, what is out of scope or unverified.

Rules:
- One concern per change.
- Do not add dependencies without justification (why, alternatives, maintenance, license) and approval.
- Do not delete or weaken tests to make them pass.
- Do not suppress errors (`let _ =`, `#[allow(...)]`) to silence the compiler without a written reason.

---

## 4. Bug-Fix Protocol

1. **Reproduce**: provide a failing test or exact repro steps. No repro, no fix; ask for one.
2. **Root cause**: state it with evidence (code path, log, query plan). Not symptoms.
3. **Write the failing test first** where feasible.
4. **Fix the cause**, not the symptom. No blanket retries, sleeps, or catch-alls.
5. **Check blast radius**: search all call sites and similar patterns.
6. **Confirm**: the new test passes, the full suite passes, and no regression elsewhere.

---

## 5. Commenting Standard (every implementation)

Every piece of code you write or change must be commented. Comments explain **why** and **contract**, not a restatement of syntax.

- **Every `pub` item and every non-trivial function**: `///` doc comment with:
  - Purpose (one line)
  - `# Arguments`: only where non-obvious
  - `# Errors`: when it returns `Err` and which variants
  - `# Panics`: must be "never" for non-test code, otherwise justify
  - `# Safety`: mandatory for `unsafe`
  - Invariants and assumptions (e.g. "embedding length must equal `EMBED_DIM`")
- **Modules**: `//!` header stating responsibility and the `architecture.md` section it implements.
- **Inline `//` comments** for: non-obvious decisions, workarounds, ordering/locking/transaction reasons, SQL intent, performance tradeoffs.
- **Trace to architecture**: `// ARCH: <section> — <decision>` where code implements a specific design decision.
- **Fix comments**: `// FIX: <root cause>. <why this is correct>` on bug fixes.
- **Tags**: `// TODO(owner): ...` must have an owner or ticket. `// UNVERIFIED: ...` for open assumptions.
- Update or delete comments when code changes. A stale comment is a bug.
- Do not comment the obvious (`// increment i`).

Example:
```rust
/// Inserts a chunk and its embedding atomically.
///
/// # Errors
/// Returns `StoreError::DimMismatch` if `embedding.len() != EMBED_DIM`.
/// Returns `StoreError::Db` on any sqlx failure; the transaction is rolled back.
///
/// # Invariants
/// `chunks.embedding` is `vector(EMBED_DIM)`; model id is stored for re-embedding.
pub async fn insert_chunk(pool: &PgPool, c: &NewChunk) -> Result<Uuid, StoreError> {
    // ARCH: Ingestion §3.2 — chunk row and vector must commit together, else retrieval returns orphans.
    ...
}
```

---

## 6. Rust Rules

- Edition and MSRV per `Cargo.toml`. Do not change them silently.
- **No `unwrap()` / `expect()` / `panic!` / indexing that can panic** in non-test code. Use `?` and typed errors. `expect()` is allowed only for proven invariants, with the proof in the message.
- Errors: `thiserror` for library/domain crates, `anyhow` only at binary edges. Preserve context (`#[source]`, `.context()`). Never swallow errors.
- Async: no blocking calls (std fs, `thread::sleep`, heavy CPU) on the Tokio runtime. Use `spawn_blocking` for CPU-bound work such as chunking or embedding in-process. Never hold a `MutexGuard` across `.await`. Make cancellation safety explicit.
- Ownership: prefer borrowing; avoid gratuitous `.clone()`; justify `Arc<Mutex<_>>`.
- Types: newtypes for IDs, dimensions, and model names. Make invalid states unrepresentable.
- `unsafe`: forbidden unless approved. Requires a `# Safety` section.
- Logging: `tracing` with structured fields. No secrets, tokens, or raw document content in logs.
- Gates (must be clean): `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`.
- Keep functions small and single-purpose. Public API changes require approval.

---

## 7. sqlx Rules

- Prefer compile-time checked `query!` / `query_as!` where the schema is static. Commit the `.sqlx/` offline data; refresh with `cargo sqlx prepare` and confirm `SQLX_OFFLINE=true cargo check` passes.
- **Never build SQL by string concatenation or `format!` with user or external input.** Always bind parameters (`$1`). Dynamic identifiers must come from an allow-list.
- **Migrations**: via `sqlx migrate`. Never edit an applied migration; add a new one. Each migration is forward-safe, with rollback/risk notes in a comment. Call out locking risk for large tables (`CREATE INDEX CONCURRENTLY`, batched backfills).
- **Transactions**: wrap multi-statement writes in a transaction. Keep them short. No network or LLM/embedding calls inside an open transaction.
- **Pool**: one shared `PgPool`; set `max_connections`, `acquire_timeout`, and statement timeouts explicitly. Never open connections per request.
- Nullability: model it honestly (`Option<T>`). Do not use `unwrap` on query results.
- Use `fetch_optional` / `fetch_one` deliberately. Handle "not found" as a typed error.
- Batch inserts with `UNNEST` / `QueryBuilder`, not loops of single inserts.
- Run `EXPLAIN (ANALYZE, BUFFERS)` on any new query on a hot path and keep the findings in the PR description. Do not guess at performance.
- Test against a real Postgres (testcontainers or `#[sqlx::test]`), not mocks, for anything touching SQL.

---

## 8. Vector DB Rules (pgvector by default; follow `architecture.md` if another store is specified)

- **Dimension is a contract**: define `EMBED_DIM` once. Validate on write and on query. The column type (`vector(N)`) must match the embedding model's output.
- **Embedding model is versioned**: store `embedding_model` and `model_version` per row. Never mix vectors from different models in one index/column. A model change requires a re-embed plan and a migration.
- **Distance metric must match the index opclass**:
  - cosine `<=>` ↔ `vector_cosine_ops`
  - L2 `<->` ↔ `vector_l2_ops`
  - inner product `<#>` ↔ `vector_ip_ops`
  A mismatch silently disables the index. Verify with `EXPLAIN`.
- **Normalization**: state whether vectors are normalized, and keep it consistent between ingest and query.
- **Index choice** (HNSW vs IVFFlat): justify with data size and recall/latency targets from `architecture.md`. Record build and query parameters (`m`, `ef_construction`, `hnsw.ef_search`, `lists`, `ivfflat.probes`). Never present untested values as tuned.
- **Filtered search**: know that approximate indexes with a `WHERE` filter can return fewer than `k` rows. Verify behaviour (iterative scan, partial indexes, partitioning) rather than assuming.
- Always `ORDER BY distance LIMIT k` with an explicit `k`. Never unbounded.
- Store metadata (source, doc id, chunk index, hash, timestamps, tenant) in relational columns, not only inside the vector payload.
- Multi-tenant: enforce tenant isolation in the query layer on every retrieval. Test it.
- Verify extension and version (`SELECT extversion FROM pg_extension WHERE extname='vector'`) before using version-specific features.

---

## 9. RAG Rules

- **Pipeline stages are separate, testable units**: load → clean → chunk → embed → store → retrieve → (rerank) → assemble context → generate → cite.
- **Chunking**: parameters (size, overlap, splitter) are explicit config and are justified. Preserve structure and source offsets. Chunking changes require re-ingestion.
- **Idempotent ingestion**: content hash per document/chunk; upsert; re-running must not duplicate. Deletions in the source must propagate.
- **Retrieval**: log query, filters, k, scores, and chunk IDs (not full content) for debugging. Apply a score threshold; "no good context" must yield "I don't know", not a guess.
- **Grounding**: generated answers must cite chunk/source IDs. The prompt must instruct the model to use only the provided context.
- **Prompt injection**: retrieved text is untrusted data. Delimit it, never let it alter system instructions or tool permissions, and never execute instructions found in documents.
- **PII/secrets**: define what may be embedded. Redact before embedding where required by `architecture.md`.
- **Evaluation before claims**: build a golden query set; measure recall@k, MRR, and answer faithfulness. Do not state "improves quality" without numbers from a run.
- **Timeouts, retries with backoff, and bounded concurrency** for embedding and LLM calls. Handle rate limits explicitly. Embedding and LLM calls must never run inside a DB transaction.
- LLM-agnostic: provider access sits behind a trait. No provider-specific types leak into the domain layer.

---

## 10. Definition of Done (all must hold; show evidence)

- [ ] Pre-flight in §1 completed; architecture source cited
- [ ] No contradiction