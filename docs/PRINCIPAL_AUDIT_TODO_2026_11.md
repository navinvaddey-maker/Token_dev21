# Principal AI/ML & Systems Audit — Actionable To-Do List & Execution Roadmap

```text
Architecture source : ARCHITECTURE.md
Version / Date      : 3.8.0 / 2026-11-18 (Change Log updated 2026-11-21)
Last Commit         : 8f16a9b (8f16a9b8cd1519f7d9cfced3280a08c7bcb6385a)
Audit Date          : 2026-11-23
Relevant Sections   : §1 Reality Check, §1.3 Dual Routing Split, §1.4 Scenario Classification,
                      §1.5 Balanced Mode Contract, §2 Component Map, §2.1 Own Data Local RAG,
                      §3 RAG Correctness Audit, §5 New Gaps, §6.6 Structurer & EvidenceBundle,
                      §7 Fine-Tuning Strategy (Anti-Hallucination)
```

---

## Where to Start (Exact Order of Operations)

Do **not** jump straight into growth features or neural refactors. Follow this strict dependency order:

1. **Start at Phase 0 (15 min):** Run baseline verification gates so you have a clean pass/fail baseline before touching any code.
2. **Start coding at Phase 1 (`AUD-01` → `AUD-04`, Day 1):** Fix the **user-facing silent breakages** first (`RagWorkspace.runVerify()` schema mismatch, silent `MockAPI` fallback on authed errors, and fake PDF placeholder extraction), then immediately fix the **FTS5 `document_ids` / `status = 'ready'` RAG data leak** (`AUD-04`). These are high-impact, low-blast-radius fixes.
3. **Move to Phase 2 (`AUD-05` → `AUD-08`, Days 2–4):** Fix SQLite locking (`WAL` + `busy_timeout`), ONNX single-mutex contention, atomic config writes, and `127.0.0.1:49152` `Origin: null` exposure.
4. **Move to Phase 3 (`AUD-09` → `AUD-15`, Days 4–8):** Replace the toy `HallucinationGuard` (`magic_token` / `fake_api_call()`), fix `Stage6b` candidate ranking, eliminate `DefaultHasher` / `HashMap` non-determinism in `OryEngine`, fix time-series `[CONFLICT]` false positives, and calibrate `estimate_tokens`.
5. **Finish with Phase 4 & Phase 5 (Week 2+):** Ship client stickiness/growth features on top of a trustworthy engine and lock in golden-set benchmarks.

> **Approval Gate Note (`.agents/skills/enterprise-subagent-orchestrator/SKILL.md`):** Before editing `config/unified.json` schemas, public API payloads, or >3 files in a single batch, review the step plan and confirm scope.

---

## Phase 0 — Baseline Check (Start Here First)

- [ ] **[AUD-00] Capture current build, lint, test, and security baseline** `Effort: XS` `[Verification]`
  - Run from repo root before any edits (status currently `[UNVERIFIED - NOT IN CONTEXT]`):
    - `cargo fmt --all -- --check`
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    - `cargo test --workspace`
    - `cargo deny check`
    - `gitleaks detect --no-banner`
    - `semgrep scan --config p/rust --config p/owasp-top-ten`
  - **Done when:** Baseline pass/fail output is recorded so regressions are distinguishable from pre-existing issues.

---

## Phase 1 — P0 Quick Wins: Broken UI Contracts & RAG Scope Leak (Days 1–2)

### 1.1 Frontend / API Contract Integrity

- [x] **[AUD-01] Fix broken `RagWorkspace.runVerify()` payload schema** `P0-Critical` `Effort: XS` `[Frontend]`
  - **Files:** `static/rag_workspace.js:277-306` vs `src/npae/grounding/verifier.rs:42-67`
  - **Problem:** UI sends `facts: [{ id: "F1", content: "..." }]`, but `VerifyGroundingRequest` expects `facts: Vec<String>` or `numbered_facts: Vec<NumberedFact>` (whose field is `text`, not `content`).
  - **Fix:** Send `facts: lines` (`string[]`) or `numbered_facts: lines.map((text, idx) => ({ id: \`F${idx + 1}\`, text }))`, and render structured `GroundingReport` badges (`passed`, `grounding_score`, `unsupported_claims`, `missing_items`) cleanly.
  - **Verify:** Manual + `tests/verify_frontend_ux.js` check hitting `POST /api/npae/verify` returns `200 OK` and detects both valid `FOUND [F1]` tables and fabricated numbers.

- [x] **[AUD-02] Remove silent `MockAPI.compress()` fallback for authenticated users & align score scale** `P0-Critical` `Effort: XS` `[Frontend]`
  - **Files:** `static/mock_api.js:23-72, 113-129`, `static/app.js:1042-1058, 1066-1078`
  - **Problem:** When `State.token` is set, any `/api/compress` failure (`400`, `408`, `429`, `500`) is swallowed by `catch (err)` and replaced with a fake `MockAPI` template (`id: 'mock-...'`, `tes_score: 0.97` on a 0–1 scale vs backend 0–10). Also `formattedPrompt` only highlights `[ROLE|CONTEXT|EXECUTION|CONSTRAINTS]` (mock tags), not backend `# ROLE:` / `# CONSTRAINTS:` / `<facts>` tags.
  - **Fix:**
    - In `API.compress()`, if `State.token` is truthy, rethrow the error to surface the toast + `error_id` instead of calling `MockAPI.compress()`.
    - Align `MockAPI.compress()` `tes_score` to `0–10` (e.g., `9.7` or compute consistently) so guest UI badges match live engine scale.
    - Update `renderResult()` tag highlighting in `static/app.js` to highlight real CRISP Markdown/XML headers.
  - **Verify:** Simulate `429`/`500` on `/api/compress` while signed in; confirm error toast displays and `State.lastResult.id` is never `'mock-*'`.

- [x] **[AUD-03] Stop fabricating prompt text on compressed PDF drop-ins** `P1-High` `Effort: XS` `[Frontend]`
  - **Files:** `static/app.js:741-795` (`extractVanillaPdfText`, `processIngestedFile`)
  - **Problem:** Unreadable/FlateDecode PDFs fall back to `[Document: ...]\nExecutive Summary & Agreement Structure ready for CRISP optimization.` and immediately trigger `compress(true)`.
  - **Fix:** If `extractVanillaPdfText` yields `< 10` valid words or detects compressed streams without extractable text, show an explicit error toast directing the user to upload via **Cloud Knowledge Base** (`POST /api/rag/upload`) or **Own Data** local companion, and do **not** overwrite `#prompt-input` or auto-compress.

### 1.2 RAG Isolation & Retrieval Correctness

- [x] **[AUD-04] Fix `RagStore::search_fts_scoped` ignoring `document_ids` & `d.status = 'ready'`, and harden FTS5 `MATCH`** `P0-Critical` `Effort: S` `[RAG / DB]`
  - **Files:** `src/rag/store.rs:551-630, 699-836`
  - **Problem:**
    1. `search_fts_scoped` omits `AND d.status = 'ready'` and ignores `document_ids`, leaking unselected/processing/failed documents into `search_hybrid_filtered_scoped`.
    2. `words.join(" OR ")` passes unquoted tokens into FTS5 `MATCH`, causing syntax errors on uppercase `NOT`/`AND`/`OR`/`NEAR` that are silently swallowed (`Ok(vec![])`).
    3. `hybrid_inner_scoped` mixes cosine similarity / fabricated `(0.95 - rank*0.05)` pseudo-scores with `5.0 * rrf`.
  - **Fix:**
    - Add `document_ids: Option<&[String]>` parameter to `search_fts_scoped` (returning `Ok(vec![])` if `Some(&[])`, or adding `AND d.id IN (...)`) and enforce `AND d.status = 'ready'`.
    - Quote every alphanumeric FTS5 token (`format!("\"{}\"", w.replace('"', ""))`) before joining with ` OR `. Only fall back to `Ok(vec![])` when the SQLite error specifically indicates a missing `rag_chunks_fts` table; log/propagate other errors.
    - Compute final hybrid `similarity_score` strictly from normalized RRF (matching `src/rag/local.rs:509`: `(vec_rrf + lex_rrf) * (k * 0.5)`).
  - **Verify:** Unit test in `src/rag/store.rs` with 2 docs (`doc_a` ready, `doc_b` processing or excluded by `document_ids`) + query containing `"NOT AND OR"` confirming zero cross-doc leaks and valid hits.

- [x] **[AUD-04B] Unify `/api/rag/search` to use `DocumentRetriever` (Hybrid RRF + Logging)** `P1-High` `Effort: XS` `[API / RAG]`
  - **Files:** `src/api.rs:1085-1122` (`rag_search`)
  - **Problem:** `rag_search` constructs `EmbeddingEngine::new()` per request, runs dense-only `state.rag_store.search_scoped`, and skips `rag_retrieval_logs`.
  - **Fix:** Route `rag_search` through `DocumentRetriever::new((*state.rag_store).clone()).retrieve_scoped(...)` inside `REQUEST_TIMEOUT_SECS` timeout.

---

## Phase 2 — Concurrency, Locking, State Races & Local Companion Security (Days 2–4)

- [ ] **[AUD-05] Enable SQLite WAL mode + `busy_timeout` and batch `PatternMemory::persist`** `P1-High` `Effort: S` `[DB / Infra]`
  - **Files:** `src/main.rs:53-59`, `src/npae/ory/memory.rs:49-81`
  - **Fix:**
    - Build `SqliteConnectOptions::from_str(&db_url)?` with `.journal_mode(SqliteJournalMode::Wal)`, `.busy_timeout(Duration::from_secs(5))`, `.synchronous(SqliteSynchronous::Normal)`, and `.foreign_keys(true)`.
    - Track `dirty_keys: HashSet<String>` in `PatternMemory` and wrap `PatternMemory::persist` in a single `pool.begin()` transaction writing only dirty patterns.
  - **Verify:** Concurrency integration test spawning 32 parallel `/api/compress` + `OryEngine::save_to_db` + `rag_upload` calls against a temp file-backed SQLite DB with zero `SQLITE_BUSY` errors.

- [ ] **[AUD-06] Fix ONNX embedding contention, batch PDF chunk embedding, and add cancellation on timeout** `P1-High` `Effort: M` `[RAG / Perf]`
  - **Files:** `src/rag/embeddings.rs:47-71, 108-157`, `src/rag/ingest.rs:273-356`, `src/classifier/mod.rs:59-85, 165-173`
  - **Fix:**
    - In `DocumentIngester::ingest_pdf_scoped` (`ingest.rs:335-340`), replace per-chunk `embeddings_engine.embed(&chunk.content)` loop with `embeddings_engine.embed_batch(&texts)`.
    - Pass an `Arc<AtomicBool>` cancellation flag into `ingest_pdf_scoped`'s `spawn_blocking` closure, setting it on timeout and checking it before embedding each batch.
    - Replace the single global `Mutex<TextEmbedding>` in `embeddings.rs` with a small bounded pool (e.g. `2–4` initialized `TextEmbedding` instances or configurable `FASTEMBED_POOL_SIZE`).
    - Cache `scenario_index` archetype rows + deserialized vectors in memory after `seed_archetype_vectors_on_startup()` instead of querying SQLite on every `/api/compress`.

- [ ] **[AUD-07] Eliminate `ConfigEnricher` vs `FileWatcher` race & make `ConfigLoader::save` atomic** `P1-High` `Effort: S` `[Config / Runtime]`
  - **Files:** `src/main.rs:115-166`, `src/npae/aggressive/config.rs:422-426`, `src/npae/aggressive/watcher.rs`
  - **Fix:**
    - Update `ConfigLoader::save` to write to a temp file in the same directory and `std::fs::rename` atomically.
    - Coordinate `FileWatcher` and `ConfigEnricher` via a shared `Arc<Mutex<()>>` (or unify into one background config task) so enricher saves update the watcher's `last_mtime` and never clobber concurrent manual edits.

- [ ] **[AUD-08] Harden endpoint bounds, semaphore coverage, and `rag` local companion CORS/auth** `P1-High` `Effort: S` `[Security / API]`
  - **Files:** `src/api.rs:335-349, 453-488, 788-815`, `src/bin/rag.rs:130-139`
  - **Fix:**
    - Remove `|| s == "null"` from `is_local_origin` in `src/bin/rag.rs:138` to block sandboxed-iframe / `data:` origin probing of `127.0.0.1:49152`. Add optional per-launch `RAG_AUTH_TOKEN` (`X-Local-Rag-Token`) support for `/retrieve` and `/reindex`.
    - Enforce `validate_prompt_bounds` on `req.output` in `npae_hallucination_check` (`api.rs:788`), bound `llm_answer` / `harness_prompt` / `facts` + acquire `compress_sem` + add inner `REQUEST_TIMEOUT_SECS` in `npae_verify_grounding` (`api.rs:808`), and acquire `compress_sem` on non-aggressive `/api/compress` (`api.rs:453`).
    - Remove the duplicate `domain::is_admin(&state.pool, &auth.user_id)` query in `admin_middleware` (`api.rs:344`) since `auth_middleware` (`api.rs:365-373`) already reloads `user.role` from SQLite on every request.
  - **Later (Phase B Security):** Migrate inline `onclick=` handlers out of `static/index.html` and `static/admin.html` so `'unsafe-inline'` can be removed from `script-src` in `security_headers_middleware` (`api.rs:307`), and move JWTs out of `localStorage` into `HttpOnly` cookies.

---

## Phase 3 — Core AI/ML, Guardrails, Scoring & Grounding Correctness (Days 4–8)

- [ ] **[AUD-09] Replace toy `HallucinationGuard` checks (`magic_token`, `fake_api_call()`) with `ARCHITECTURE.md` §7.2 Layer 3** `P0-Critical` `Effort: L` `[AI/ML / Safety]`
  - **Files:** `src/npae/hallucination/guard.rs`, `l1_critique.rs`, `l2_confidence.rs`, `l3_constraint.rs`
  - **Fix:**
    - **L1 (`l1_critique.rs`):** Keep structural format contradiction checks (`"no json"` etc.) AND check sentence-level cosine/lexical alignment between generated scaffolding claims and `original_prompt` + `guard_source_text`.
    - **L2 (`l2_confidence.rs`):** Detect factual/numeric spans in output lacking support in `guard_source_text` (or hedge claims missing `[UNCERTAIN]`/`[VERIFY]`).
    - **L3 (`l3_constraint.rs`):** Verify all extracted `CONSTRAINT_LOCK` / negation / forbidden-topic constraints are honored in the active output (removing reliance on `"fake_api_call()"` / `"magic_token"` sentinel strings in production logic).
  - **Verify:** Adversarial test suite with dropped negations, invented numeric claims, and format contradictions must fail `run_tri_layer` without containing `"magic_token"` or `"fake_api_call()"`.

- [ ] **[AUD-10] Fix `Stage6b::CandidateSnapshot::is_better_than` ranking order** `P1-High` `Effort: XS` `[Pipeline / Safety]`
  - **Files:** `src/pipeline/stage6b.rs:68-84`
  - **Problem:** `self_compressed != other_compressed` currently beats `self.guard_report.passed != other.guard_report.passed`.
  - **Fix:** Put `guard_report.passed` **first** in the comparison hierarchy, followed by `self_compressed`, then `min_axis()` and `avg_score()`.

- [ ] **[AUD-11] Fix `OryLearner` `DefaultHasher` instability & `PatternMemory::find_match` random domain fallback** `P1-High` `Effort: S` `[AI/ML / Determinism]`
  - **Files:** `src/npae/ory/learner.rs:10, 887-895`, `src/npae/ory/memory.rs:89-108`
  - **Fix:**
    - Replace `std::collections::hash_map::DefaultHasher` in `generate_fingerprint` with deterministic SHA-256 (first 8 bytes hex) or FNV-1a.
    - Replace `PatternMemory::find_match`'s arbitrary `HashMap` domain-only fallback with deterministic similarity matching on stored objective text/embeddings (cosine `>= 0.85` with deterministic tie-breaking by `pattern_id`).

- [ ] **[AUD-12] Fix `detect_numeric_conflicts` false positives on time-series & multi-entity evidence** `P1-High` `Effort: S` `[Grounding]`
  - **Files:** `src/npae/aggressive/structurer/evidence.rs:841-926`
  - **Problem:** `extract_domain_terms` strips non-alphabetic tokens (`Q1`, `Q2`, `2025`, `2026`, `v1`, `v2`), falsely flagging normal quarter-over-quarter metrics as `[CONFLICT]`.
  - **Fix:** Extract temporal/version qualifiers (`Q1..Q4`, 4-digit years, `FY\d{2,4}`, `v\d+`, month names) per item and do **not** flag a conflict when two items carry distinct non-empty period/entity qualifiers.

- [ ] **[AUD-13] Fix `DerivedRagContext::derive` 80% overlap filter dropping confirming quantitative facts** `P2-Medium` `Effort: XS` `[RAG / Grounding]`
  - **Files:** `src/npae/aggressive/rag_context.rs:240-256`
  - **Fix:** Only drop a candidate sentence under `overlap_ratio >= 0.80` if the sentence adds **no** numeric, currency, or percentage values absent from (or unconfirmed in) the prompt, and retain provenance for verification queries.

- [ ] **[AUD-14] Decouple `detect_knowledge_level` from `intent_vec` `max_val` & calibrate `estimate_tokens`** `P2-Medium` `Effort: M` `[AI/ML / Scoring]`
  - **Files:** `src/npae/aggressive/intent.rs:155-202`, `src/npae/aggressive/engine.rs:553-574`, `src/utils/tokens.rs:1-5`
  - **Fix:**
    - Base `detect_knowledge_level` strictly on domain vocabulary density and technical syntax markers rather than normalized `intent_vec` `max_val`.
    - Upgrade `estimate_tokens` from `words * 1.3` to a BPE-calibrated subword estimator (handling code symbols, camelCase/snake_case, URLs, JSON punctuation, and numbers accurately, or integrating `tiktoken-rs`).
    - Bound `EMBED_CACHE` (`src/npae/aggressive/intent.rs:294`) with a max capacity (e.g., `10_000` entries) as defense-in-depth.

- [ ] **[AUD-15] Clean up Scenario mode & deprecated legacy pipeline artifacts** `P2-Medium` `Effort: S` `[Cleanup / Architecture]`
  - **Files:** `src/scenario/router.rs:279-294, 587-602`, `src/scenario/pipeline.rs:260-289`, `src/engine/pipeline.rs`, `src/data.rs`
  - **Fix:**
    - In `ScenarioModeRouter`, surface or log `orch.process()` errors instead of silent `if let Ok(...)` and reuse per-user session state from `AppState.sessions` where appropriate.
    - Replace synthetic `"Dispatched tool ..."` wording in `ScenarioToolRouter::execute_tools` with clear `"Declared Tool Capability (Not Executed)"` metadata so downstream LLMs never mistake declared tools for executed tool calls.
    - Remove deprecated `src/engine/pipeline.rs` (`GAP-040`) once remaining `PipelineOutput` references in `src/data.rs` are decoupled.

---

## Phase 4 — Growth & Client-Value Features (Week 2+)

- [ ] **[GRW-01] Inline 1-Click "Verify LLM Reply" inside Workbench** `High ROI` `Effort: S` `[Product / UX]`
  - **Files:** `static/index.html`, `static/app.js`, `static/rag_workspace.js`
  - Auto-bind `StructuredPromptResponse.grounding_facts` + `optimized_prompt` into a "Verify LLM Output" panel directly on the main Optimize Prompt tab (eliminating manual line-by-line fact copying).

- [ ] **[GRW-02] Interactive Clarifying Question Quick-Fill Pills** `High ROI` `Effort: S` `[Product / UX]`
  - **Files:** `static/app.js`, `static/index.html`
  - Render `clarifying_questions` returned by `AggressiveEngine` as clickable answer chips that append resolved context and re-run compression in 1 click.

- [ ] **[GRW-03] Real Word-Level Diff + Shareable "Token & Grounding Audit Card"** `Medium ROI` `Effort: M` `[Growth / Viral]`
  - **Files:** `static/app.js`
  - Replace the single `<del>` summary line with an inline diff + real backend `# ROLE:`/`# CONSTRAINTS:`/`<facts>` highlighting, plus a 1-click Markdown/SVG summary export (`Tokens Saved`, `$ Saved / 1k runs`, `Constraint Recall`, `Grounding Score`).

- [ ] **[GRW-04] Developer CLI (`tce`) / CI Prompt Linter & Local Companion Installer UX** `High ROI` `Effort: M` `[Adoption / B2B]`
  - **Files:** `src/bin/rag.rs` (extend or add `src/bin/tce.rs`)
  - Package `rag` + `tce` prebuilt binaries / `--stdin` CI mode so teams can gate prompt budgets and grounding verification in CI/CD and local IDE workflows without `cargo run --bin rag`.

---

## Phase 5 — Verification & Benchmark Sign-Off

- [ ] **[VER-01] Expand & run automated golden benchmarks + security gates** `Effort: M` `[QA / Release Gate]`
  - Expand `tests/eval_aggressive_accuracy.rs` to ≥ 100 adversarial prompts (target: **Domain Macro-F1 ≥ 0.92**, **Constraint Recall ≥ 0.98**).
  - Run `tests/eval_scenario_recall.rs` with `document_ids` filtering on both Cloud and Own Data indices (target: **Recall@5 ≥ 0.88**, **MRR ≥ 0.75**, **Cross-Doc Leak = 0%**).
  - Run `tests/eval_grounding_harness.rs` (target: **Unsupported-Number Recall = 1.00**, **Injection Quarantine Recall ≥ 0.98**).
  - Pass all `.agents/workflows/review.md` gates cleanly after final edit:
    - `cargo fmt --all -- --check`
    - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    - `cargo test --workspace`
    - `cargo deny check`
    - `gitleaks detect --no-banner`
    - `semgrep scan --config p/rust --config p/owasp-top-ten`
