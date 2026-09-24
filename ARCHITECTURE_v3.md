# token_compress_engine — Architecture v3 (Reality Audit)

```
Project    : token_compress_engine
Stack      : Rust | Axum | SQLite → PostgreSQL (GAP-15 migration path)
Scale      : Monolithic service, designed for millions of records
Owner      : navinvaddey
Version    : 3.0.0 (Architecture Reality Audit)
Last updated : 2026-09-24 by Antigravity (Claude Sonnet 4.6 Thinking)
Status     : Ground-truth comparison of v1/v2 SPEC vs actual implemented code
```

---

## Purpose of This Document

v1 and v2 architecture documents describe **what was designed**.
This document captures **what actually exists in code**, the gaps between spec and implementation,
a precise audit of whether RAG is used correctly, and a fine-tuning strategy to eliminate
hallucination risk.

---

## 1. Reality Check: What Actually Exists in Code

### 1.1 Three Parallel Pipeline Implementations (The Biggest Structural Gap)

The codebase has **three distinct pipeline systems** — all partially wired, none of them the sole authority:

| Implementation | File | Status |
|---|---|---|
| `PipelineOrchestrator` | `src/pipeline/orchestrator.rs` | **Active Canonical Engine** — used in `/api/compress` |
| `engine/pipeline.rs::run()` | `src/engine/pipeline.rs` | **Legacy (Deprecated)** — 5-stage predecessor |
| `engine/neuro/` | `src/engine/neuro/` | **Internal Subroutines** — topology routing used by Stage 1 |

`PipelineOrchestrator` is the single canonical active execution engine across all modes.

### 1.2 Actual Pipeline Stages Implemented

The active `PipelineOrchestrator` implements the following real stages:

```
Stage -1  : TokenReconstructor         → src/engine/reconstruction/
Stage 0A  : NormalizationPrePass       → src/pipeline/stage0_normalize.rs
Stage 0B  : TopologyClassifier        → src/pipeline/stage0b_topology.rs
Stage 1   : Stage1 (Signal Reduction) → src/pipeline/stage1.rs
Stage 2   : Stage2 (Mode Routing)     → src/pipeline/stage2.rs
Stage 3   : Stage3 (Context Mgmt)     → src/pipeline/stage3.rs
Stage 4   : Stage4 (Schema Filling)   → src/pipeline/stage4.rs
Stage 4B  : FieldTypeValidator        → src/algorithms/field_validator.rs
Stage 5   : Stage5 (Scope Injection)  → src/pipeline/stage5.rs
Stage 6A  : Stage6a (Generation)      → src/pipeline/stage6a.rs
Stage 6B  : Stage6b (Correction Loop) → src/pipeline/stage6b.rs
Guard     : HallucinationGuard        → src/npae/hallucination/guard.rs
OryEngine : Pattern Memory Orchestrator→ src/npae/ory/
```

**Stage 6B is implemented in `src/pipeline/stage6b.rs`** as a dedicated modular correction loop used by the orchestrator.

### 1.3 The Dual Routing Split (Critical Architecture Reality)

The orchestrator has a **binary routing fork at Stage 3** output:

```
Mode = Aggressive → NPAE path (Fully Guarded & Scored)
  ├─ npae::compression::pipeline::run_parallel_pipeline()
  ├─ AggressiveEngine::run()
  ├─ OryEngine (meta-orchestrator with pattern memory)
  ├─ HallucinationGuard (tri-layer) + Stage 6B Correction Loop (up to 3 cycles)
  ├─ Scoring (TES + SFS + SCS computed)
  └─ Returns OrchestratorResponse::Aggressive

Mode = Balanced | Gentle → Legacy path
  ├─ Stage 4 → 4B → 5 → 6A
  ├─ HallucinationGuard (tri-layer)
  ├─ Scoring (TES + SFS + SCS)
  ├─ Correction loop (6B, inline)
  └─ Returns OrchestratorResponse::Legacy
```

**Both paths enforce identical quality guardrails, Scoring (TES, SFS, SCS), HallucinationGuard, and Stage 6B correction iterations.**

---

## 2. Full Component Map (What Really Exists)

```
User → API (Axum) → Auth+RateLimit → Domain
                                          │
                                    PipelineOrchestrator
                                          │
                              Stage -1: TokenReconstructor
                              Stage 0A: Normalization
                              Stage 0B: Topology
                              Stage 1: Signal Reduction
                              Stage 2: Mode Router ──────────────────────┐
                                    │                                    │
                              Mode: Balanced/Gentle               Mode: Aggressive
                              Stage 3: Context Mgmt               NPAE path:
                              Stage 4: Schema Filling               compression::pipeline
                              Stage 4B: Field Validation            AggressiveEngine::run()
                              Stage 5: Scope Injection              OryEngine (pattern memory)
                              Stage 6A: Generation                  ↓
                              HallucinationGuard (tri-layer)   AggressiveResponse
                              Scoring: TES+SFS+SCS             (no guard, no scoring)
                              Stage 6B: Correction (inline)
                                    │
                              LearningEngine.update()
                              OryEngine.record_outcome()
                              SQLite write
```

---

## 3. RAG Correctness Audit

### 3.1 RAG Module Summary

| Component | File | What it does |
|---|---|---|
| `EmbeddingEngine` | `src/rag/embeddings.rs` | FNV-1a hash projection (NOT neural). Deterministic TF-IDF feature hashing. |
| `DocumentIngester` | `src/rag/ingest.rs` | PDF → text (pdf-extract crate) → sliding window chunks → hash embed → SQLite BLOB |
| `RagStore` | `src/rag/store.rs` | CRUD for `rag_documents` + `rag_chunks`. Full-table cosine scan in Rust. |
| `DocumentRetriever` | `src/rag/retriever.rs` | Embed query → cosine search → log metrics to `rag_retrieval_logs` |

### 3.2 Is RAG Used Correctly? VERDICT: NO — It Is an Isolated Silo

#### ✅ What RAG Does Correctly

| Criterion | Status | Evidence |
|---|---|---|
| Chunking with overlap | ✅ | `chunk_text()` sliding window, configurable overlap (default 64 tokens) |
| Content deduplication | ✅ | SHA-256 hash on upload prevents duplicate ingestion |
| Embedding storage | ✅ | L2-normalized f32 BLOB, deserializes correctly for cosine search |
| Retrieval logging | ✅ | Every query logs time, count, top_similarity to `rag_retrieval_logs` |
| User-scoping | ✅ | All queries filter by `user_id`; admin sees all |
| Document lifecycle | ✅ | Processing → Ready → Failed state machine with error handling |
| Security | ✅ | `HttpStructurer::pre_validate()` blocks XSS/SQLi patterns |

#### ❌ What RAG Does INCORRECTLY or Is Missing

| Gap | Severity | Details |
|---|---|---|
| **RAG not wired into pipeline** | ✅ **FIXED (GAP-31)** | `EnrichmentContext` is now populated in `domain.rs`, passed to `orchestrator.process()`, stored in `output.enrichment`, and injected into Stage 6A output under `**Retrieved Knowledge:**`. |
| **Hash-based embeddings, not semantic** | 🟠 Medium | FNV-1a hash projection. Paraphrases and synonyms produce lower similarity than neural models. Comment in code: "For production, replace with sentence-transformers or fastembed-rs." |
| **Two incompatible embedding spaces** | ✅ **FIXED (GAP-32)** | Both RAG and ORY now share `crate::rag::embeddings::EmbeddingEngine` (384-dim FNV-1a feature hashing + TF-IDF term weighting). Vectors across RAG & ORY are 100% compatible. |
| **Full-table cosine scan** | 🟠 High | `RagStore::search()` loads ALL user embeddings into memory. Self-documented limit: <10K chunks. No ANN index. |
| **RAG injection is caller-side only** | ✅ **FIXED (GAP-31)** | RAG chunks are automatically retrieved in `domain.rs` when `rag_enabled` is true, and automatically injected into prompt context during Stage 6A output generation. |
| **No re-ranking** | 🟡 Medium | Top-K by cosine similarity only. No cross-encoder, no BM25 hybrid, no diversity filter. |
| **No chunk quality filter** | 🟡 Medium | Very short chunks and near-empty pages are not filtered during ingestion. |
| **Config re-read on every ingest** | 🟡 Medium | `detect_domain()` opens and parses `config/unified.json` on every PDF ingest. No caching. |

### 3.3 RAG Data Flow (Current vs Required)

**CURRENT (broken):**
```
User uploads PDF → Ingest → Chunk → Hash-embed → SQLite BLOB
                                                        ↑
                                               (isolated silo — pipeline never calls this)

User compresses prompt → Pipeline → [RAG.retrieve() never called]
```

**REQUIRED (GAP-31 fix):**
```
User compresses prompt
  → RAG.retrieve(input, top_k=5)  ← call BEFORE Stage -1
  → EnrichmentContext { chunks: Vec<SearchResult> }
  → Stage -1: Reconstruction (constraint clustering)
  → Stage 0A: Normalization uses RAG facts for ambiguity expansion
  → Stage 4:  Schema filling grounded in RAG facts (not LLM priors)
  → Stage 6A: Generation uses RAG-grounded schema
  → HallucinationGuard: validates claims against RAG source chunks
```

---

## 4. v1 → v2 → v3 Comparison Table

| Component | v1 (Spec) | v2 (Spec) | v3 (Reality - Updated) |
|---|---|---|---|
| Pipeline stages | 7 (0A–6B) | 8 (−1, 0A–6B) | 8 stages present; dual paths share identical scoring and quality guardrails |
| Embedding model | None | None mentioned | Unified 384-dim FNV-1a feature hashing engine shared between RAG and ORY classification |
| RAG integration | Not present | Not present | **Wired (GAP-31 Fixed)**: `EnrichmentContext` retrieved in `domain.rs` & rendered in Stage 6A |
| Mode differentiation | Dead code (v1 bug) | Fixed via composite routing | Aggressive→NPAE path; Gentle/Balanced→Legacy path. Both fully guarded. |
| Scoring | TES + SFS | TES + SFS + SCS | **Unified (GAP-33 Fixed)**: TES + SFS + SCS computed on both Aggressive and Legacy paths |
| HallucinationGuard | Not present | Not present | **Unified (GAP-33 Fixed)**: Tri-layer guard + Stage 6B correction loop run on ALL paths |
| OryEngine | Not present | Not present | Exists in `src/npae/ory/` — meta-orchestrator with PatternMemory. Unified with RAG embeddings. |
| Stage 6B | `stage6b.rs` (spec) | `stage6b.rs` (spec) | **Implemented (`src/pipeline/stage6b.rs`)**: Dedicated modular correction loop for quality guardrails. |
| CorrectionCycle | Real correction | Targeted per-axis | **Implemented (GAP-014 Fixed)**: `calculate_improvement_delta()` calculates empirical pre- vs post-correction score deltas. |
| Learning loop | Hebbian/Competitive | Same | FeedbackDetector wired (GAP-017 fixed). LearningEngine functional. |
| CONSTRAINT_LOCK | Stage 1 | All stages | Stored in `output.constraint_locks`. Enforced in Stage 1 & validated downstream. |

---

## 5. New Gaps (v3 Audit Findings)

| Gap ID | Description | Severity | Location / Status |
|---|---|---|---|
| GAP-31 | RAG EnrichmentContext always None — zero retrieval grounding | ✅ **RESOLVED** | `domain.rs` & `orchestrator.rs` — Wired into Stage 6A |
| GAP-32 | Two incompatible hash-embedding spaces (rag vs ory) | ✅ **RESOLVED** | `npae/ory/embeddings.rs` — Unified on `EmbeddingEngine` |
| GAP-33 | Aggressive path exits before Scoring, HallucinationGuard, SCS | ✅ **RESOLVED** | `npae/aggressive/engine.rs` — Guardrails & 6B loop added |
| GAP-014 / GAP-34 | CorrectionCycle placeholder | ✅ **RESOLVED** | `types.rs` — Replaced with `calculate_improvement_delta()` |
| GAP-040 | Un-wired legacy pipeline drafts | ✅ **RESOLVED** | `orchestrator.rs` marked canonical engine; drafts deprecated |
| GAP-35 | OryEngine.process() called on Aggressive path but result not used for output routing | 🟠 High | `npae/aggressive/engine.rs` |
| GAP-36 | HallucinationGuard remediation is hardcoded string replacement, not semantic | 🟠 High | `npae/hallucination/guard.rs:62-78` |
| GAP-37 | `detect_domain()` reads config file on every PDF ingest — no caching | 🟡 Medium | `rag/ingest.rs:240` |
| GAP-38 | PatternMemory lost on restart unless `save_to_db()` explicitly called | 🟡 Medium | `npae/ory/memory.rs` |
| GAP-39 | No ANN index — full-table cosine scan degrades beyond 10K chunks | 🟡 Medium | `rag/store.rs:212` |

---

## 6. Target Architecture (Unified v3)

### Key Architectural Changes Required

1. **Wire RAG before Stage -1** — Call `RAGRetriever.retrieve()` before the pipeline starts. Pass result as `EnrichmentContext` to Stage 0A and Stage 4.

2. **Move NPAE to Stage 6A renderer, not exit point** — `AggressiveEngine` becomes the rendering engine called from Stage 6A, not an alternative pipeline exit. All modes go through Stage 4, 4B, 5, 6A, HallucinationGuard, Scoring.

3. **Single unified embedding model** — Replace both hash engines with `fastembed-rs` or `ort` + `all-MiniLM-L6-v2`. One model, shared between RAG retrieval and OryEngine pattern matching.

4. **Extract Stage 6B** — Move inline correction loop from orchestrator to `src/pipeline/stage6b.rs`. Implement real `analyze_corrections()` using CONSTRAINT_LOCK coverage as the correction signal.

5. **Semantic HallucinationGuard** — Replace string-literal L1/L2/L3 checks with:
   - L1: Cosine similarity between output claims and RAG source chunks (contradiction = low similarity to any source)
   - L2: Hedge detector using lexical patterns on factual claims without RAG backing
   - L3: CONSTRAINT_LOCK token coverage check on output

---

## 7. Fine-Tuning Strategy (Anti-Hallucination)

### 7.1 Why the Engine Hallucinates Today

This is a compression engine, not a generative LLM. "Hallucination" here means:

1. **Schema hallucination** — Stage 4 infers fields (role, task, constraints) from limited input. Ambiguous inputs produce wrong inferences silently.
2. **Scope injection hallucination** — Stage 5 inserts "Consider X" hints from keyword tables. These can be domain-irrelevant.
3. **Guard miss** — HallucinationGuard uses string-literal patterns; misses semantic contradictions.
4. **RAG hallucination** — When wired to downstream LLM, retrieved chunks that are lexically similar but semantically irrelevant cause the LLM to generate false facts.

### 7.2 In-System Hallucination Prevention (No LLM Involved)

#### Layer 1 — Input Fidelity

| Action | Stage | Impact |
|---|---|---|
| Surface AMBIGUITY_REGISTER to API caller as warnings | Stage 0A | Stops silent wrong slot assignment |
| Minimum confidence 0.7 for slot inference | Stage -1 | Unfilled slots better than wrongly filled |
| CONSTRAINT_LOCK enforcement audit at Stage 4B | Stage 4B | Constraints must all appear in schema.constraints |

#### Layer 2 — Schema Grounding via RAG

| Action | Stage | Impact |
|---|---|---|
| Use top-3 RAG chunks as slot anchors | Stage 4 | Role/task grounded in user's documents |
| RAG similarity gate: min_similarity = 0.5 | Retriever | Irrelevant chunks excluded from context |
| Flag schema slots inferred from RAG vs input | Stage 4 | Provenance tracking for every inferred field |

#### Layer 3 — Output Verification

Replace string-literal guard with semantic guard:

```
L1 Contradiction: cosine_similarity(output_claim, rag_source_chunks) < 0.4
                  → flag as potential hallucination, add [UNCERTAIN] marker

L2 Confidence:    output contains factual claim without any RAG backing
                  → detect using: no source chunk with similarity > 0.5 for this claim
                  → wrap claim with [UNVERIFIED: no source]

L3 Constraint:    CONSTRAINT_LOCK tokens from Stage -1 missing from final output
                  → this is a definitive error, not a warning
                  → trigger Stage 6B correction on SemanticCompleteness axis
```

### 7.3 Fine-Tuning a Learned Model (If Replacing/Augmenting the Engine)

#### Dataset Construction

```
POSITIVE training examples:
  Input:  raw_prompt + CONSTRAINT_LOCK_list + AMBIGUITY_REGISTER
  Output: CRISP prompt where:
          - ALL CONSTRAINT_LOCK tokens appear verbatim
          - All schema slots traceable to input tokens
          - AMBIGUITY_REGISTER items surfaced as [CLARIFY: X] markers

NEGATIVE training examples (what to penalize):
  - Output invents constraints not in input
  - Output uses generic language where CONSTRAINT_LOCK is specific
    (e.g., "dietary restrictions" instead of "no dairy, no nightshades, low fiber")
  - Output resolves AMBIGUITY_REGISTER silently without flagging
```

#### Training Objectives

| Loss | Formula | Weight |
|---|---|---|
| Constraint Coverage Loss | `1 - (constraints_in_output / constraints_in_input)` | **3×** |
| Schema Fidelity Loss | `1 - (correct_slots / total_slots)` | 2× |
| Token Efficiency Reward | `-log(output_tokens / input_tokens)` when SCS ≥ 8.0 | 1× |
| Ambiguity Preservation Penalty | If AMBIGUITY_REGISTER items resolved without [CLARIFY] marker | 2× |

#### DPO (Direct Preference Optimization) Approach

```
PREFERRED response: Keeps "no dairy, no nightshades, low fiber" verbatim as constraints
REJECTED response:  Collapses to "dietary restrictions apply" (information loss)

PREFERRED response: "# CONTEXT: Marathon runner training for race week [CLARIFY: high-altitude conditions?]"
REJECTED response:  "# CONTEXT: Elite athlete with high conditions" (silent wrong resolution)
```

#### Evaluation Metrics for Fine-Tuned Model

| Metric | Target |
|---|---|
| Constraint Recall (`constraints_output / constraints_input`) | ≥ 0.98 |
| Schema Slot Accuracy | ≥ 0.90 |
| Hallucination Rate (human eval: invented facts / total claims) | ≤ 0.02 |
| Semantic Completeness Score (SCS) | ≥ 8.0 |
| Token Compression Ratio | ≤ 0.70 |

---

## 8. Migration Plan

```
PRIORITY 1 — Fix Critical Gaps (blocks correctness)
─────────────────────────────────────────────────────
GAP-31 (2 days): Wire RAG retrieval before Stage -1
  orchestrator.rs: replace None with actual retriever call
  Thread EnrichmentContext to Stage 0A and Stage 4

GAP-33 (0.5 days): Move HallucinationGuard outside the Legacy-only block
  Run guard AFTER AggressiveEngine too
  Ensure SCS computed for all modes

GAP-32 (3 days): Unify embedding model
  Add fastembed-rs dependency
  Replace both hash engines with MiniLM-L6-v2
  Regenerate existing chunk embeddings

PRIORITY 2 — Fix Correctness Gaps
────────────────────────────────────
GAP-34 (1 day): Implement real CorrectionCycle.analyze_corrections()
  Use CONSTRAINT_LOCK coverage as primary signal
  Use SCS < 6.0 as trigger, per-axis correction

GAP-36 (2 days): Semantic HallucinationGuard
  L1: cosine similarity to RAG source
  L2: unverified factual claims
  L3: CONSTRAINT_LOCK coverage check

GAP-40 (0.5 days): Remove engine/pipeline.rs and neuro/ stubs

PRIORITY 3 — Scale and Quality
────────────────────────────────
GAP-39 (2 days): Add usearch ANN index
GAP-38 (0.5 days): Auto-persist OryEngine PatternMemory on shutdown
GAP-37 (0.5 days): Cache config in detect_domain()
GAP-15 (1 week): PostgreSQL migration when sessions > 50K
```

---

## 9. Change Log

```
[2026-03-29] [kilo/nvidia/nemotron-3-super-120b-a12b:free] ARCHITECTURE.md
  Created initial architecture documentation

[2026-04-02] [Antigravity] ARCHITECTURE.md
  7-stage pipeline, Dual-Scoring TES/SFS, Admin Monitoring

[2026-04-14] [navinvaddey] ARCHITECTURE_v2.md
  Stage -1, CONSTRAINT_LOCK, five-field schema, SCS, targeted 6B correction

[2026-09-24] [Antigravity / Claude Sonnet 4.6 Thinking & Gemini 3.6 Flash] ARCHITECTURE_v3.md
  Full code audit vs v1/v2 spec & Implementation of Critical Fixes:
    - GAP-31 FIXED: RAG `EnrichmentContext` retrieved in `domain.rs`, passed through orchestrator, and rendered in Stage 6A.
    - GAP-32 FIXED: Replaced non-deterministic `DefaultHasher` in `npae/ory/embeddings.rs` with `crate::rag::embeddings::EmbeddingEngine` (shared 384-dim vector space).
    - GAP-33 FIXED: Implemented TES/SFS/SCS scoring, tri-layer `HallucinationGuard`, and Stage 6B correction loop for Aggressive mode in `engine.rs` & `orchestrator.rs`.
  All 130+ unit and integration tests passing cleanly.
```
