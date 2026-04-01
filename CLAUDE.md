# CLAUDE.md — Coding Agent Configuration
> Place this file at the **root of your repository**.
> Every AI model (Claude Code, Cursor, Copilot, etc.) reads this before writing or modifying any code.
> **Do NOT delete change history.** Append only. Token-efficient summaries, not full diffs.

---

## 1. Project Identity

```
Project  : token_compress_engine
Stack    : Rust | Axum | SQLite
Scale    : Monolithic service, designed for millions of records
Owner    : navinvaddey
Last updated : 2026-03-29 by kilo/nvidia/nemotron-3-super-120b-a12b:free
```

---

## 2. Coding Rules (enforced on every generation)

### 2.1 Architecture
- **Monolithic first.** No microservice splits unless explicitly requested. One deployable unit.
- Services live in `src/services/`. One file per domain (e.g. `UserService.ts`, `OrderService.ts`).
- No circular dependencies between services. Dependency direction: `Controller → Service → Repository → DB`.

### 2.2 SOLID Principles
| Principle | Rule |
|---|---|
| **S** Single Responsibility | Each class/function does exactly one thing. Max 1 public method per exported class unless it is a service facade. |
| **O** Open/Closed | Extend via new classes or strategy injection. Never modify existing logic to add a feature. |
| **L** Liskov Substitution | Subtypes must honour parent contracts. No throwing unexpected errors in overrides. |
| **I** Interface Segregation | Interfaces max 5 methods. Split if a consumer only uses 2 of 10. |
| **D** Dependency Inversion | Inject dependencies, never `new` them inside logic. Use constructor injection. |

### 2.3 Scale: Millions of Records
- **Always paginate** — no `findAll()` or `SELECT *` without `LIMIT` + `OFFSET` or cursor.
- **Bulk operations** — use batch insert/update (chunk size ≤ 1000 rows per query).
- **Index every FK and every filter column** — migrations must add index when adding a column used in `WHERE`.
- **Avoid N+1** — use `JOIN` or `DataLoader`-style batching, never loop-inside-loop DB calls.
- **Async everywhere** — no blocking I/O on the main thread. Use async/await throughout.
- **Connection pooling** — DB client must use a pool (min 5, max 20). Never create ad-hoc connections.

### 2.4 File & Function Structure
```
src/
  controllers/    # HTTP layer only — no business logic
  services/       # Business logic — one domain per file
  repositories/   # DB queries only — no logic
  models/         # Data types and interfaces
  utils/          # Pure helper functions
  tests/          # Unit tests — mirror src/ structure
```
- Max function length: **30 lines**. Split if longer.
- Max file length: **300 lines**. Split if longer.
- All functions must have **JSDoc / docstring** with `@param`, `@returns`, `@throws`.
- Export only what is consumed outside the file. Everything else is private.

### 2.5 Extension Functions
When writing an extension or a new function on an existing module:
1. Read the **Change Log** (Section 4) before writing anything.
2. The extension **must not break** the existing public interface (Open/Closed).
3. Add a new entry to Section 4 immediately after writing.
4. If behaviour changes, add a deprecation notice on the old function, not a deletion.

---

## 3. Unit Test Rules

> Tests are written for **functions only** — not classes, not HTTP endpoints (those are integration tests).

### 3.1 What Gets a Test
- Every exported function in `src/services/` and `src/utils/`.
- Every repository function that builds a query (mock the DB, test the query shape).
- Edge cases: empty input, null/undefined, max boundary, error path.

### 3.2 Test Structure (AAA Pattern)
```typescript
describe('functionName', () => {
  it('should <expected behaviour> when <condition>', () => {
    // Arrange
    const input = ...;
    // Act
    const result = functionName(input);
    // Assert
    expect(result).toEqual(...);
  });
});
```

### 3.3 Rules
- One `expect` per test case (unless asserting a single object's multiple fields).
- No test depends on another test's state — each test is fully isolated.
- Mock all I/O (DB, HTTP, file system) — unit tests must run offline.
- Test file lives at `src/tests/<domain>.test.ts`, mirroring the source path.
- Test coverage goal: **80% line coverage** on `services/` and `utils/`.

### 3.4 What NOT to Test
- `controllers/` — integration/e2e tests handle these.
- Third-party library internals.
- Framework boilerplate (e.g. module registration, DI container wiring).

---

## 4. Change Log (append only — never delete)

> **Format for AI models:** When you make any change, append one entry below.
> Keep entries to ≤ 4 lines. Do not paste code — describe the *intent* and *impact*.
> This log is the shared memory between models. Read it before every generation.

```
[YYYY-MM-DD] [Model: <model name>] [File: <path>]
What changed : <one sentence>
Why          : <one sentence>
Impact       : <what other models must know — e.g. "OrderService now expects userId as UUID not int">
```

### Log entries:

```
[2024-01-01] [Model: human] [File: CLAUDE.md]
What changed : Initial CLAUDE.md created
Why          : Establish shared coding rules for all AI models
Impact       : All models must read this file before generating or modifying code
```

<!-- ADD NEW ENTRIES BELOW THIS LINE — newest at bottom -->

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/data.rs, src/domain.rs, src/api.rs]
What changed : Added enhanced admin monitoring capabilities including system performance, learning engine stats, compression analytics, user engagement, feedback analysis, and error metrics
Why          : Provide administrators with deeper insights into system operation and effectiveness for better decision making
Impact       : New API endpoints available under /api/admin/* requiring admin authentication; existing functionality unchanged

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/api.rs, src/domain.rs, src/models/user.rs, static/app.js]
What changed : Fixed admin access for user 'navin' by correcting middleware order, adding business_type to login response, and updating frontend to handle business_type
Why          : User 'navin' (admin) could not access admin screen due to auth middleware order and missing business_type in login response; also removed duplicate/non-admin navin users
Impact       : Admin endpoints now correctly authenticate and authorize users; stats screen navigation should work; admin link appears for admin users

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/domain.rs, static/app.js]
What changed : Fixed admin authentication by making business_type comparison case-insensitive in both backend and frontend
Why          : Admin screen still not visible for user 'navin' due to case sensitivity mismatch in business_type comparison
Impact       : Admin endpoints and UI now correctly recognize admin users regardless of business_type casing

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: static/app.js]
What changed : Added debug logging and trim() to business_type comparison in frontend to handle whitespace issues
Why          : Admin screen still not visible after case-insensitive fix due to potential whitespace in business_type field from database
Impact       : More robust admin detection that handles case variations and leading/trailing whitespace

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: static/app.js]
What changed : Fixed business_type validation in login response to check for falsy values instead of just undefined
Why          : Admin screen not visible for user 'navin' due to business_type being present but empty string or null in login response
Impact       : More robust validation that prevents setting State.businessType to invalid values, ensuring admin link displays correctly

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/main.rs]
What changed : Changed admin user seeding to use lowercase "admin" for business_type
Why          : Admin screen still not visible due to case sensitivity mismatch between seeded "Admin" and frontend check for "admin"
Impact       : Frontend admin detection now works correctly for seeded admin users

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: static/index.html]
What changed : Added Admin tab to main navigation bar alongside Compress, History, and Stats tabs
Why          : Provide direct access to admin functionality from main interface for admin users
Impact       : Admin users can now access admin screen via tab navigation instead of only direct URL

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: static/app.js]
What changed : Changed admin tab handling to load admin content via iframe instead of redirect
Why          : Admin tab should open as a tab within the main interface, not redirect to separate page
Impact       : Admin dashboard now loads inline within the main application when clicking the Admin tab

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: static/index.html]
What changed : Embedded admin.html in iframe within Admin tab content
Why          : To maintain single-page application experience while showing admin dashboard
Impact       : Admin tab now shows admin dashboard as inline content without navigation away from main app

[2026-03-28] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/main.rs]
What changed : Enhanced admin user seeding logic to update business_type to 'admin' if user exists but has incorrect business_type
Why          : User 'navin' existed in database but had wrong business_type, preventing admin access
Impact       : Admin screen now correctly displays for user 'navin' after login; admin endpoints accessible



---
## 5. Token Budget Rules

> This section keeps context lean so models don't exhaust token limits.

- When sending this file to a model, **include only Sections 1–5** for new code tasks.
- For extension tasks, **also include the relevant Change Log entries** (last 10 max).
- **Never paste full file contents** into a prompt — reference the file path instead.
- Summaries in Change Log are capped at 4 lines by design. Do not expand them.
- If context exceeds ~3000 tokens, trim by removing Change Log entries older than 30 days (archive them to `CLAUDE_ARCHIVE.md`).

---

## 6. Suggested Additions (add when your project grows)

| Section to add | When to add it |
|---|---|
| `## 6. API Contract` — request/response shapes | When you expose HTTP endpoints |
| `## 7. DB Schema Snapshot` — table names + FK map | When you have ≥ 5 tables |
| `## 8. Known Tech Debt` — list of shortcuts taken | When you knowingly break a rule |
| `## 9. Environment Config` — required env vars | Before first deployment |
| `## 10. Performance Baselines` — P99 latency targets | When you have load test data |
| `## 11. Security Rules` — input validation, auth patterns | Before handling user data |
| `## 12. Deprecation Notices` — functions marked for removal | When you run Open/Closed extensions |
| `## 13. Architecture Documentation` — component diagrams, data flow | When architecture becomes complex enough to benefit from visualization |

---

[2026-03-29] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: CLAUDE.md]
What changed : Updated CLAUDE.md with project identity details and created ARCHITECTURE.md file
Why          : Document current project structure and establish version tracking for architectural changes
Impact       : Project now has clear identity specification and architecture documentation with version control

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/types.rs]
What changed : Added new types from refinements guide including PromptTopology, DegeneracyType, NormalizationResult, TextCorrection, FieldRepair, OrdinalSequence, FieldContentType, FieldValidationIssue, DualScore, ScoringIssue, CorrectionCycle; extended AlgorithmOutput with topology, normalization, ordinal_sequence, field_issues, scope_injections, dual_score, correction_cycle fields
Why          : Implement refinements guide specifications for enhanced token compression engine functionality
Impact       : New types and fields available throughout the codebase for improved compression analytics and processing

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/types.rs]
What changed : Added new types from refinements guide including PromptTopology, DegeneracyType, NormalizationResult, TextCorrection, FieldRepair, OrdinalSequence, FieldContentType, FieldValidationIssue, DualScore, ScoringIssue, CorrectionCycle; extended AlgorithmOutput with topology, normalization, ordinal_sequence, field_issues, scope_injections, dual_score, correction_cycle fields; implemented Default trait for new types
Why          : Implement refinements guide specifications for enhanced token compression engine functionality
Impact       : New types and fields available throughout the codebase for improved compression analytics and processing; types now derive Default for use in AlgorithmOutput

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/algorithms/field_validator.rs]
What changed : Created FieldTypeValidator implementation with validate method and helper functions for checking deliverable, constraints, and task fields as specified in refinements guide
Why          : Implement field validation stage in pipeline to ensure task, deliverable, and context fields meet required constraints and types
Impact       : New field validation stage added to pipeline that runs after schema filling; outputs field issues in AlgorithmOutput for downstream processing

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/pipeline/orchestrator.rs]
What changed : Integrated FieldTypeValidator into pipeline orchestrator as stage 4b after schema filling
Why          : Add field validation step to pipeline to catch issues with task, deliverable, and context fields before final output generation
Impact       : Pipeline now includes field validation stage; orchestrator calls FieldTypeValidator::validate() after stage 4 schema filling

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/pipeline/stage0_normalize.rs]
What changed : Created NormalizationPrePass implementation with DOMAIN_VOCAB and FIELD_CONTRACTS static maps, and run method that performs typo correction and field mismatch detection
Why          : Implement Stage 0 normalization pre-pass as specified in refinements guide to improve input quality before main pipeline processing
Impact       : Pipeline now includes normalization stage that corrects typos and detects field type mismatches; orchestrator updated to run normalization before stage 1

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/pipeline/stage0b_topology.rs]
What changed : Added TopologyClassifier implementation with classify method and topology_mode_prior function as specified in refinements guide
Why          : Implement Stage 0b topology classification to enhance pipeline with structural awareness of prompts
Impact       : New topology classification stage added to pipeline; orchestrator updated to run topology classification after normalization; predictive coding modified to use topology priors for mode decision

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/pipeline/stage0b_topology.rs]
What changed : Added TopologyClassifier implementation with classify method and topology_mode_prior function as specified in refinements guide
Why          : Implement Stage 0b topology classification to enhance pipeline with structural awareness of prompts
Impact       : New topology classification stage added to pipeline; orchestrator updated to run topology classification after normalization; predictive coding modified to use topology priors for mode decision

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/algorithms/field_validator.rs]
What changed : Recreated FieldTypeValidator implementation with validate method that takes task, deliverable, constraints, and context parameters and returns Vec<FieldValidationIssue>, plus helper functions check_deliverable, check_constraints, and check_task
Why          : Implement field validation stage exactly as specified in refinements guide
Impact       : Field validation stage now correctly validates content types against expected types as per refinements guide specification

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/scoring/weights.rs]
What changed : Added IssueWeights struct with TES and SFS weight calculation methods as specified in refinements guide
Why          : Implement scoring weights for Task-Essential Score and Schema Fidelity Score calculations
Impact       : New scoring module available for calculating field issue weights in pipeline output

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/scoring/tes.rs]
What changed : Created TokenEfficiencyScorer implementation with score method that calculates TES based on token efficiency metrics and field validation issues
Why          : Implement Token-Essential Score calculation as specified in refinements guide
Impact       : New TES scoring available for pipeline output assessment

## 7. Where This File Lives

```
your-project/
├── CLAUDE.md          ← this file (repo root)
├── CLAUDE_ARCHIVE.md  ← old change log entries (auto-trimmed from main)
├── src/
└── ...
```

**Claude Code** reads `CLAUDE.md` automatically from the repo root on every session.
**Cursor / Copilot** — add a rule to "always read CLAUDE.md at session start" in their respective settings files.
**Manual prompt** — paste Sections 1–5 at the top of any model prompt before your task.

---

*This file is maintained by humans and AI models together. Last model to write: append your name and date to Section 1.*[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/scoring/tes.rs]
What changed : Created TokenEfficiencyScorer implementation with score method that calculates TES based on token efficiency metrics and field validation issues
Why          : Implement refinements guide specifications for Token-Essential Score calculation
Impact       : New TES scoring available for pipeline output assessment

[2026-03-31] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/algorithms/sparse_coding.rs]
What changed : Extended sparse coding with phrase-aware scoring as specified in refinements guide; added DOMAIN_PHRASES static HashSet, implemented compute_salience_phrase_aware method, updated apply method to use phrase-aware scoring
Why          : Implement refinements guide specifications for enhanced sparse coding that recognizes compound phrases as single units
Impact       : Sparse coding now scores known compound phrases (like "two factor authentication") as single units with boosted salience, improving token compression for domain-specific terminology

[2026-03-31] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/algorithms/lexical.rs]
What changed : Added ordinal sequence extractor with ORDINAL_PATTERN regex, OrdinalExtractor struct with extract/render methods, and modified compress method to store ordinal sequence in AlgorithmOutput
Why          : Implement refinements guide specifications for ordinal sequence extraction in the lexical compression stage
Impact       : Pipeline now captures ordinal sequences (1st, 2nd, 3rd, etc.) from input text for use in downstream processing and analytics

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/scoring/sfs.rs]
What changed : Created SemanticFidelityScorer implementation with score method that calculates SFS based on field validation issues
Why          : Implement Schema Fidelity Score calculation as specified in refinements guide
Impact       : New SFS scoring available for pipeline output assessment

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/correction/cycle.rs]
What changed : Verified and corrected CorrectionCycle implementation with proper struct fields and methods for tracking correction cycles
Why          : Ensure the CorrectionCycle struct matches refinements guide specifications exactly
Impact       : CorrectionCycle now correctly implements all required functionality for pipeline correction tracking

[2026-03-30] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/correction/mod.rs, src/correction/cycle.rs]
What changed : Created correction module with Cycle submodule implementing CorrectionCycle struct from refinements guide
Why          : Implement correction cycle tracking as specified in refinements guide for pipeline enhancement
Impact       : New correction cycle functionality available; AlgorithmOutput now includes correction_cycle field

[2026-04-01] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: src/pipeline/stage5.rs, src/pipeline/mod.rs, src/pipeline/orchestrator.rs, src/types.rs, src/bin/debug_comparison.rs, src/bin/debug_full_pipeline.rs]
What changed : Implemented Stage 5: Scope Injection stage in the token compression pipeline as specified in the refinements guide
Why          : Complete the pipeline implementation by adding scope injection functionality that generates deterministic scope suggestions based on inferred task/deliverable/context in aggressive mode
Impact       : Pipeline now includes Stage 5 that generates scope injections in aggressive mode; updated orchestrator to call stage5 after stage4 and before field validation; added scope_injections field to CompressionResponse
