---
trigger: model_decision
description: "Apply when modifying PipelineOrchestrator, Balanced Mode, SparseCoding, 3-Axis Intent Tensor (DxPxI), NPAE Aggressive Structurer, HallucinationGuard, Grounding Verifier, or Continuation Ledgers."
---

# Pipeline & Prompt Invariants

When modifying core pipeline orchestration, compression modes, intent tensors, or grounding verifiers, adhere strictly to these invariants:

1. **Balanced Mode Contract**:
   - 45% base keep ratio.
   - `PROTECTED_QUALIFIERS` + digits/units + multi-word `constraint_locks` locked at salience `1.0`.
   - `CONVERSATIONAL_FILLER` at `0.05` with zero positional bonus.
   - Active-output SCS/SFS verification must never use pre-sparse `clean_tokens`.
2. **Stage 6B Priority**:
   - `CandidateSnapshot::is_better_than` sorts by `guard_report.passed` first, then non-expanding, then `min_axis` / `avg_score`.
3. **3-Axis Intent Tensor ($D \times P \times I$)**:
   - `ActorPerspective::Consumer` skips `detect_business_type`.
   - `rule_locked` blocks Ory domain override.
4. **SchemaRegistry**:
   - 3-tier zero-SQLite in-memory lookup & non-destructive Stage 4 merge.
5. **Stateful Continuation Invariants**:
   - `SL-1`: constraint supersession + `scrub_superseded_terms`.
   - `SL-2`: 2-turn domain hysteresis.
   - `SL-3`: monotonic `[F#]` fact registration before `pack_aligned`.
6. **Grounding & Evidence Invariants**:
   - Unit-aware numeric verification (`$500 != 500 != 500%`).
   - Temporal/entity qualifier exemption in `detect_numeric_conflicts`.
