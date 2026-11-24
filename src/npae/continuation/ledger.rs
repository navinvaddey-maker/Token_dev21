//! Persistent turn-by-turn prompt state ledger (`PromptStateLedger`).
//!
//! // ARCH: ARCHITECTURE.md §6.7 — maintains locked 3-axis classification (`SL-2` domain
//! // hysteresis), lifecycle-tracked constraints (`Active` / `Superseded` / `Revoked`),
//! // monotonic SHA-256 deduplicated `[F#]` facts (`SL-3`), execution phase progress, and
//! // resolved ambiguity slots across multi-turn continuations.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::npae::schema::types::{DeliverableType, LengthBound, StructuredPrompt};

use super::router::ConstraintOp;

/// Minimum candidate confidence required before a foreign domain can advance the `SL-2` hysteresis streak.
const DOMAIN_HYSTERESIS_MIN_CONFIDENCE: f32 = 0.65;

/// Number of consecutive turns supporting a new domain required to override a locked domain (`SL-2`).
const DOMAIN_HYSTERESIS_REQUIRED_STREAK: u8 = 2;

/// Locked 3-axis intent classification (`Domain × Perspective × Intent`) stored in the ledger.
///
/// // ARCH: ARCHITECTURE.md §6.7 (`SL-2`) — prevents single-turn noisy domain flips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockedAxis {
    /// Canonical domain identifier (for example `"software"` or `"finance"`).
    pub domain: String,
    /// Actor perspective key (`"consumer"`, `"practitioner"`, or `"investor"`).
    pub perspective: String,
    /// Primary intent key (`"build"`, `"explain"`, `"debug"`, `"analyze"`, or `"transform"`).
    pub intent: String,
    /// Confidence score in `[0.0, 1.0]` at the time the axis was locked or updated.
    pub confidence: f32,
}

/// Category of a tracked constraint inside [`PromptStateLedger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    /// Required inclusion, capability, or quantitative bound.
    Inclusion,
    /// Forbidden topic, library, ingredient, or exclusion rule.
    Exclusion,
    /// Requested output format constraint.
    Format,
    /// Output length bound constraint.
    LengthBound,
    /// Tone or style constraint.
    Tone,
}

/// Lifecycle state of a [`ConstraintEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintStatus {
    /// Currently enforced on the prompt.
    Active,
    /// Replaced by a newer constraint (`by_id`).
    Superseded {
        /// Identifier of the superseding constraint (for example `"C4"`).
        by_id: String,
    },
    /// Explicitly removed or relaxed by the user.
    Revoked,
}

/// Versioned constraint entry with turn provenance and supersession tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintEntry {
    /// Stable monotonic constraint identifier (for example `"C1"`, `"C2"`).
    pub id: String,
    /// Constraint category.
    pub kind: ConstraintKind,
    /// Human-readable constraint text.
    pub text: String,
    /// Current lifecycle status.
    pub status: ConstraintStatus,
    /// Turn number when this constraint was introduced.
    pub introduced_turn: u32,
    /// Turn number when this constraint was last updated.
    pub updated_turn: u32,
}

/// Lifecycle state of an execution phase in [`PromptStateLedger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    /// Scheduled for future execution.
    Pending,
    /// Currently targeted by the active continuation turn.
    Active,
    /// Completed in an earlier turn.
    Done,
    /// Explicitly deferred by the user.
    Deferred,
}

/// Tracked execution phase entry inside [`PromptStateLedger`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseEntry {
    /// 1-based phase number.
    pub phase_number: u32,
    /// Concise phase title.
    pub name: String,
    /// Phase objective or scope description.
    pub description: String,
    /// Current lifecycle state.
    pub status: PhaseStatus,
}

/// Append-only, SHA-256 deduplicated fact entry (`SL-3`).
///
/// # Invariants
/// `id` (`"F1"`, `"F2"`, …) is assigned monotonically on first insertion and is never reused or
/// renumbered across turns. `content_hash` is the lowercase hex SHA-256 digest of normalized `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactEntry {
    /// Stable monotonic fact identifier without brackets (`"F1"`, `"F2"`, …).
    pub id: String,
    /// Normalized single-line fact statement.
    pub text: String,
    /// Lowercase SHA-256 hex digest of `text`.
    pub content_hash: String,
    /// Provenance label (for example `"user"`, `"rag"`, or `"structured"`).
    pub source: String,
    /// Turn number when this fact was first registered.
    pub introduced_turn: u32,
}

/// Stateful multi-turn prompt ledger preserving canonical constraints, facts, phases, and slots.
///
/// // ARCH: ARCHITECTURE.md §6.7 — enforces `SL-2` domain hysteresis and `SL-3` monotonic `[F#]`
/// // fact identities across delta follow-ups and retyped prompt edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptStateLedger {
    /// Caller-assigned or generated ledger session identifier.
    pub ledger_id: String,
    /// Current 1-based turn counter (`0` prior to initialization).
    pub turn_count: u32,
    /// Primary task objective anchored on the initial turn.
    pub objective: String,
    /// Deliverable classification of the objective.
    pub deliverable_type: DeliverableType,
    /// Locked 3-axis classification subject to `SL-2` hysteresis.
    pub locked_axis: Option<LockedAxis>,
    /// Ordered constraint history (`Active`, `Superseded`, `Revoked`).
    pub constraints: Vec<ConstraintEntry>,
    /// Execution phases tracked across turns.
    pub phases: Vec<PhaseEntry>,
    /// Append-only fact registry (`SL-3`).
    pub facts: Vec<FactEntry>,
    /// Unresolved ambiguity or gap slots remaining to be filled.
    pub open_slots: Vec<String>,
    /// Resolved slot values keyed deterministically by slot name.
    pub resolved_slots: BTreeMap<String, String>,
    /// Active output format specification.
    pub output_format: String,
    /// Active tone specification.
    pub tone: String,
    /// Active length bound specification.
    pub length_bound: LengthBound,
    pending_domain: Option<String>,
    pending_domain_streak: u8,
    next_constraint_seq: u32,
    next_fact_seq: u32,
}

impl PromptStateLedger {
    /// Creates an empty [`PromptStateLedger`] for a new session or conversation branch.
    ///
    /// # Arguments
    /// * `ledger_id` - Unique identifier for this stateful prompt chain.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn new(ledger_id: impl Into<String>) -> Self {
        Self {
            ledger_id: ledger_id.into(),
            turn_count: 0,
            objective: String::new(),
            deliverable_type: DeliverableType::default(),
            locked_axis: None,
            constraints: Vec::new(),
            phases: Vec::new(),
            facts: Vec::new(),
            open_slots: Vec::new(),
            resolved_slots: BTreeMap::new(),
            output_format: String::new(),
            tone: String::new(),
            length_bound: LengthBound { min: 0, max: 0 },
            pending_domain: None,
            pending_domain_streak: 0,
            next_constraint_seq: 1,
            next_fact_seq: 1,
        }
    }

    /// Advances the ledger turn counter and returns the new turn number.
    ///
    /// # Panics
    /// Never.
    pub fn advance_turn(&mut self) -> u32 {
        self.turn_count = self.turn_count.saturating_add(1);
        self.turn_count
    }

    /// Populates initial state from a compiled [`StructuredPrompt`] on Turn 1 (or after `TopicReset`).
    ///
    /// # Arguments
    /// * `structured` - Initial structured prompt compiled by the Aggressive pipeline.
    /// * `perspective` - Actor perspective label (`"consumer"`, `"practitioner"`, or `"investor"`).
    /// * `intent` - Primary intent label (`"build"`, `"analyze"`, etc.).
    /// * `confidence` - Classifier confidence in `[0.0, 1.0]`.
    /// * `open_slots` - Gap zones / clarifying slots remaining unresolved after Turn 1.
    ///
    /// # Panics
    /// Never.
    pub fn apply_initial_structured(
        &mut self,
        structured: &StructuredPrompt,
        perspective: &str,
        intent: &str,
        confidence: f32,
        open_slots: Vec<String>,
    ) {
        if self.turn_count == 0 {
            self.turn_count = 1;
        }
        let turn = self.turn_count;

        self.objective = structured
            .objective
            .as_ref()
            .map(|o| o.primary_task.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| structured.context.description.trim().to_string());

        self.deliverable_type = structured
            .objective
            .as_ref()
            .map(|o| o.deliverable_type.clone())
            .unwrap_or_default();

        self.locked_axis = Some(LockedAxis {
            domain: structured.context.domain.clone(),
            perspective: perspective.to_string(),
            intent: intent.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
        });
        self.pending_domain = None;
        self.pending_domain_streak = 0;

        self.output_format
            .clone_from(&structured.constraints.output_format);
        self.tone.clone_from(&structured.constraints.tone);
        self.length_bound = structured.constraints.length_bound.clone();

        for inc in &structured.constraints.required_inclusions {
            self.push_unique_constraint(ConstraintKind::Inclusion, inc, turn);
        }
        for forb in &structured.constraints.forbidden_topics {
            self.push_unique_constraint(ConstraintKind::Exclusion, forb, turn);
        }

        self.phases = structured
            .execution_phases
            .iter()
            .enumerate()
            .map(|(idx, phase)| PhaseEntry {
                phase_number: u32::from(phase.phase_number),
                name: phase.name.clone(),
                description: phase.description.clone(),
                status: if idx == 0 {
                    PhaseStatus::Active
                } else {
                    PhaseStatus::Pending
                },
            })
            .collect();

        self.open_slots.clear();
        for slot in open_slots {
            let trimmed = slot.trim();
            if !trimmed.is_empty()
                && !self
                    .open_slots
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(trimmed))
            {
                self.open_slots.push(trimmed.to_string());
            }
        }
    }

    /// Applies parsed [`ConstraintOp`] mutations to the ledger without discarding prior history.
    ///
    /// # Arguments
    /// * `ops` - Ordered constraint operations extracted by [`crate::npae::continuation::router::ContinuationRouter`].
    ///
    /// # Panics
    /// Never.
    pub fn apply_constraint_ops(&mut self, ops: &[ConstraintOp]) {
        let turn = self.turn_count.max(1);
        for op in ops {
            match op {
                ConstraintOp::AddInclusion { text } => {
                    self.push_unique_constraint(ConstraintKind::Inclusion, text, turn);
                }
                ConstraintOp::AddExclusion { text } => {
                    self.push_unique_constraint(ConstraintKind::Exclusion, text, turn);
                }
                ConstraintOp::Override {
                    old_target,
                    new_text,
                    is_exclusion,
                } => {
                    self.apply_override(old_target, new_text, *is_exclusion, turn);
                }
                ConstraintOp::Revoke { target } => {
                    self.apply_revoke(target, turn);
                }
            }
        }
    }

    /// Returns all currently active inclusion constraints in insertion order.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn active_inclusions(&self) -> Vec<&ConstraintEntry> {
        self.constraints
            .iter()
            .filter(|c| c.status == ConstraintStatus::Active && c.kind == ConstraintKind::Inclusion)
            .collect()
    }

    /// Returns all currently active exclusion constraints in insertion order.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn active_exclusions(&self) -> Vec<&ConstraintEntry> {
        self.constraints
            .iter()
            .filter(|c| c.status == ConstraintStatus::Active && c.kind == ConstraintKind::Exclusion)
            .collect()
    }

    /// Returns the text of all constraints marked [`ConstraintStatus::Superseded`].
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn superseded_targets(&self) -> Vec<&str> {
        self.constraints
            .iter()
            .filter_map(|c| match c.status {
                ConstraintStatus::Superseded { .. } => Some(c.text.as_str()),
                ConstraintStatus::Active | ConstraintStatus::Revoked => None,
            })
            .collect()
    }

    /// Registers facts into the append-only `[F#]` store (`SL-3`), deduplicating by SHA-256 content hash.
    ///
    /// Returns the newly inserted [`FactEntry`] items minted during this call (in insertion order).
    /// Existing facts retain their original `[F#]` identifiers and are never renumbered or deleted.
    ///
    /// # Arguments
    /// * `incoming` - Slice of `(fact_text, source_label)` tuples.
    ///
    /// # Panics
    /// Never.
    pub fn register_facts<S: AsRef<str>>(&mut self, incoming: &[(S, S)]) -> Vec<FactEntry> {
        let turn = self.turn_count.max(1);
        let mut minted = Vec::new();

        for (raw_text, raw_source) in incoming {
            let normalized = raw_text
                .as_ref()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if normalized.is_empty() {
                continue;
            }
            let content_hash = compute_sha256_hex(normalized.as_bytes());
            if self.facts.iter().any(|f| f.content_hash == content_hash) {
                continue;
            }

            let id = format!("F{}", self.next_fact_seq);
            self.next_fact_seq = self.next_fact_seq.saturating_add(1);
            let entry = FactEntry {
                id,
                text: normalized,
                content_hash,
                source: raw_source.as_ref().trim().to_string(),
                introduced_turn: turn,
            };
            self.facts.push(entry.clone());
            minted.push(entry);
        }

        minted
    }

    /// Marks `phase_number` as [`PhaseStatus::Active`], transitioning earlier active phases to `Done`.
    ///
    /// Returns `true` if `phase_number` exists in `self.phases`, or `false` otherwise.
    ///
    /// # Arguments
    /// * `phase_number` - 1-based execution phase number to activate.
    ///
    /// # Panics
    /// Never.
    pub fn set_active_phase(&mut self, phase_number: u32) -> bool {
        if !self.phases.iter().any(|p| p.phase_number == phase_number) {
            return false;
        }
        for phase in &mut self.phases {
            if phase.phase_number == phase_number {
                phase.status = PhaseStatus::Active;
            } else if phase.status == PhaseStatus::Active {
                phase.status = if phase.phase_number < phase_number {
                    PhaseStatus::Done
                } else {
                    PhaseStatus::Pending
                };
            }
        }
        true
    }

    /// Records a resolved slot value and removes any case-insensitive match from `open_slots`.
    ///
    /// Returns `true` if a matching slot was present in `open_slots` prior to resolution.
    ///
    /// # Arguments
    /// * `slot_name` - Name of the ambiguity/gap slot being resolved.
    /// * `value` - User-supplied value resolving the slot.
    ///
    /// # Panics
    /// Never.
    pub fn resolve_slot(&mut self, slot_name: &str, value: &str) -> bool {
        let trimmed_name = slot_name.trim();
        let trimmed_value = value.trim();
        if trimmed_name.is_empty() || trimmed_value.is_empty() {
            return false;
        }

        let prev_len = self.open_slots.len();
        self.open_slots
            .retain(|s| !s.eq_ignore_ascii_case(trimmed_name));
        let was_open = self.open_slots.len() < prev_len;

        self.resolved_slots
            .insert(trimmed_name.to_string(), trimmed_value.to_string());
        was_open
    }

    /// Evaluates `SL-2` domain hysteresis to prevent single-turn noisy domain overrides.
    ///
    /// Returns `true` when the domain override is accepted (either because `explicit_reset` is
    /// `true`, no domain is locked yet, or `candidate_domain` has achieved `>= 2` consecutive
    /// high-confidence signals). When `true` is returned on an existing locked axis, the locked
    /// domain and confidence are updated immediately.
    ///
    /// # Arguments
    /// * `candidate_domain` - Domain predicted for the current turn.
    /// * `candidate_confidence` - Confidence score in `[0.0, 1.0]` for `candidate_domain`.
    /// * `explicit_reset` - Whether the turn triggered a `TopicReset`.
    ///
    /// # Panics
    /// Never.
    pub fn should_override_domain(
        &mut self,
        candidate_domain: &str,
        candidate_confidence: f32,
        explicit_reset: bool,
    ) -> bool {
        let candidate = candidate_domain.trim();
        if candidate.is_empty() {
            return false;
        }

        if explicit_reset {
            self.pending_domain = None;
            self.pending_domain_streak = 0;
            if let Some(locked) = &mut self.locked_axis {
                locked.domain = candidate.to_string();
                locked.confidence = candidate_confidence.clamp(0.0, 1.0);
            }
            return true;
        }

        let Some(locked) = &mut self.locked_axis else {
            return true;
        };

        if candidate.eq_ignore_ascii_case(&locked.domain) {
            self.pending_domain = None;
            self.pending_domain_streak = 0;
            return false;
        }

        if candidate_confidence < DOMAIN_HYSTERESIS_MIN_CONFIDENCE {
            self.pending_domain = None;
            self.pending_domain_streak = 0;
            return false;
        }

        let same_pending = self
            .pending_domain
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case(candidate));
        if same_pending {
            self.pending_domain_streak = self.pending_domain_streak.saturating_add(1);
        } else {
            self.pending_domain = Some(candidate.to_string());
            self.pending_domain_streak = 1;
        }

        if self.pending_domain_streak >= DOMAIN_HYSTERESIS_REQUIRED_STREAK {
            locked.domain = candidate.to_string();
            locked.confidence = candidate_confidence.clamp(0.0, 1.0);
            self.pending_domain = None;
            self.pending_domain_streak = 0;
            return true;
        }

        false
    }

    fn allocate_constraint_id(&mut self) -> String {
        let id = format!("C{}", self.next_constraint_seq);
        self.next_constraint_seq = self.next_constraint_seq.saturating_add(1);
        id
    }

    fn push_unique_constraint(
        &mut self,
        kind: ConstraintKind,
        raw_text: &str,
        turn: u32,
    ) -> String {
        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        if let Some(existing) = self.constraints.iter().find(|c| {
            c.status == ConstraintStatus::Active
                && c.kind == kind
                && c.text.eq_ignore_ascii_case(trimmed)
        }) {
            return existing.id.clone();
        }

        let id = self.allocate_constraint_id();
        // FIX: Medium #2 — supersede any active opposite-kind constraint on the same core
        // target so contradictory `[STRICT]` and `[AVOID]` rules cannot coexist.
        let opposite_kind = match kind {
            ConstraintKind::Inclusion => Some(ConstraintKind::Exclusion),
            ConstraintKind::Exclusion => Some(ConstraintKind::Inclusion),
            ConstraintKind::Format | ConstraintKind::LengthBound | ConstraintKind::Tone => None,
        };
        if let Some(opp_kind) = opposite_kind {
            for entry in &mut self.constraints {
                if entry.status == ConstraintStatus::Active
                    && entry.kind == opp_kind
                    && constraint_matches_target(&entry.text, trimmed)
                {
                    entry.status = ConstraintStatus::Superseded { by_id: id.clone() };
                    entry.updated_turn = turn;
                }
            }
        }

        self.constraints.push(ConstraintEntry {
            id: id.clone(),
            kind,
            text: trimmed.to_string(),
            status: ConstraintStatus::Active,
            introduced_turn: turn,
            updated_turn: turn,
        });
        id
    }

    fn apply_override(&mut self, old_target: &str, new_text: &str, is_exclusion: bool, turn: u32) {
        let new_kind = if is_exclusion {
            ConstraintKind::Exclusion
        } else {
            ConstraintKind::Inclusion
        };
        let new_id = self.push_unique_constraint(new_kind, new_text, turn);
        if new_id.is_empty() {
            return;
        }

        let mut matched_any = false;
        for entry in &mut self.constraints {
            if entry.id == new_id || entry.status != ConstraintStatus::Active {
                continue;
            }
            if constraint_matches_target(&entry.text, old_target) {
                entry.status = ConstraintStatus::Superseded {
                    by_id: new_id.clone(),
                };
                entry.updated_turn = turn;
                matched_any = true;
            }
        }

        if !matched_any && !old_target.trim().is_empty() {
            let tombstone_id = self.allocate_constraint_id();
            self.constraints.push(ConstraintEntry {
                id: tombstone_id,
                kind: ConstraintKind::Inclusion,
                text: old_target.trim().to_string(),
                status: ConstraintStatus::Superseded { by_id: new_id },
                introduced_turn: turn,
                updated_turn: turn,
            });
        }
    }

    fn apply_revoke(&mut self, target: &str, turn: u32) {
        let trimmed_target = target.trim();
        if trimmed_target.is_empty() {
            return;
        }
        // FIX: High #2 — match both UI `C#` constraint IDs and constraint text targets.
        for entry in &mut self.constraints {
            if entry.status == ConstraintStatus::Active
                && (entry.id.eq_ignore_ascii_case(trimmed_target)
                    || constraint_matches_target(&entry.text, trimmed_target))
            {
                entry.status = ConstraintStatus::Revoked;
                entry.updated_turn = turn;
            }
        }
    }
}

/// Tokenizes `text` into lowercase alphanumeric/hyphen boundary tokens.
pub(crate) fn tokenize_boundary(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Returns `true` when `haystack` contains `needle` as a contiguous token subsequence.
pub(crate) fn tokens_contain_subsequence(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Strips leading directive or exclusion prefixes from a lowercase constraint/target phrase.
pub(crate) fn strip_directive_or_exclusion_prefix(lower: &str) -> &str {
    for prefix in [
        "target/bound: ",
        "must not use ",
        "must include ",
        "must support ",
        "do not use ",
        "don't use ",
        "include ",
        "require ",
        "without ",
        "exclude ",
        "using ",
        "avoid ",
        "never ",
        "use ",
        "with ",
        "no ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    lower.trim()
}

/// Checks whether a constraint's text matches an override/revoke target string using token boundaries.
fn constraint_matches_target(entry_text: &str, target: &str) -> bool {
    let entry_low = entry_text.trim().to_lowercase();
    let target_low = target.trim().to_lowercase();
    if entry_low.is_empty() || target_low.is_empty() {
        return false;
    }

    // FIX: Medium #7 — use word/token-boundary matching instead of raw `.contains` so
    // short tokens like `"Go"` or `"AI"` never match `"MongoDB"` or `"FastAPI"`.
    let entry_tokens = tokenize_boundary(&entry_low);
    let target_tokens = tokenize_boundary(&target_low);
    if tokens_contain_subsequence(&entry_tokens, &target_tokens)
        || (!entry_tokens.is_empty() && tokens_contain_subsequence(&target_tokens, &entry_tokens))
    {
        return true;
    }

    let entry_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&entry_low));
    let target_core_tokens = tokenize_boundary(strip_directive_or_exclusion_prefix(&target_low));
    if tokens_contain_subsequence(&entry_core_tokens, &target_core_tokens)
        || (!entry_core_tokens.is_empty()
            && tokens_contain_subsequence(&target_core_tokens, &entry_core_tokens))
    {
        return true;
    }

    // FIX: Low — retain numeric tokens (`50`, `80`) alongside >= 3-char tokens so
    // `"under 50 ms"` never falsely matches `"under 80 kg"`.
    let entry_sig: Vec<&str> = entry_core_tokens
        .iter()
        .map(String::as_str)
        .filter(|t| t.len() >= 3 || t.chars().any(|c| c.is_ascii_digit()))
        .collect();
    let target_sig: Vec<&str> = target_core_tokens
        .iter()
        .map(String::as_str)
        .filter(|t| t.len() >= 3 || t.chars().any(|c| c.is_ascii_digit()))
        .collect();

    !target_sig.is_empty() && target_sig.iter().all(|tok| entry_sig.contains(tok))
}

/// Computes lowercase SHA-256 hex digest for canonical fact deduplication (`SL-3`).
fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npae::continuation::diff::ParentMatcher;
    use crate::npae::continuation::router::ContinuationRouter;
    use crate::npae::schema::types::{
        ExecutionPhase, HallucinationGuardConfig, PromptConstraints, PromptContext,
        PromptObjective, PromptRole,
    };

    fn sample_structured_prompt() -> StructuredPrompt {
        StructuredPrompt {
            role: PromptRole {
                primary: "Principal Backend Architect".to_string(),
                secondary: None,
                expertise_domains: vec!["software".to_string()],
                persona_constraints: Vec::new(),
                persona_anchor: String::new(),
            },
            objective: Some(PromptObjective {
                primary_task: "Design a Rust REST API with MongoDB".to_string(),
                deliverable_type: DeliverableType::Strategy,
                scope_boundary: Vec::new(),
            }),
            context: PromptContext {
                domain: "software".to_string(),
                description: "REST API service".to_string(),
                background: String::new(),
                user_knowledge_level: "expert".to_string(),
                audience: "backend engineers".to_string(),
                temporal_scope: "current".to_string(),
                intent_vector: Vec::new(),
                assumptions: Vec::new(),
            },
            constraints: PromptConstraints {
                output_format: "markdown".to_string(),
                length_bound: LengthBound {
                    min: 200,
                    max: 1000,
                },
                forbidden_topics: vec!["Exclude Redis".to_string()],
                required_inclusions: vec!["Use MongoDB".to_string(), "Include JWT".to_string()],
                tone: "concise".to_string(),
            },
            hallucination_guard: HallucinationGuardConfig {
                self_critique_enabled: true,
                confidence_threshold: 0.7,
                contradiction_check: true,
                claim_verification_rules: Vec::new(),
                uncertainty_markers: Vec::new(),
            },
            output_spec: None,
            execution_phases: vec![
                ExecutionPhase {
                    phase_number: 1,
                    name: "Data Model".to_string(),
                    description: "Schema and persistence".to_string(),
                    estimated_duration: "1w".to_string(),
                    deliverables: vec!["Schema".to_string()],
                },
                ExecutionPhase {
                    phase_number: 2,
                    name: "API Layer".to_string(),
                    description: "HTTP routes and auth".to_string(),
                    estimated_duration: "1w".to_string(),
                    deliverables: vec!["OpenAPI".to_string()],
                },
            ],
            validation_steps: Vec::new(),
            success_criteria: Vec::new(),
            constraints_meta: None,
            dynamic_instruction: String::new(),
        }
    }

    #[test]
    fn test_constraint_override_and_revoke_lifecycle() {
        let mut ledger = PromptStateLedger::new("led-1");
        let structured = sample_structured_prompt();
        ledger.apply_initial_structured(
            &structured,
            "practitioner",
            "build",
            0.9,
            vec!["deployment target".to_string()],
        );

        ledger.advance_turn();
        let delta = ParentMatcher::extract_delta(
            "Design a Rust REST API with MongoDB.",
            "Use PostgreSQL instead of MongoDB and remove Redis.",
            true,
        );
        let route = ContinuationRouter::classify(&delta, &ledger.open_slots, true, false);
        ledger.apply_constraint_ops(&route.constraint_ops);

        let inclusions: Vec<&str> = ledger
            .active_inclusions()
            .into_iter()
            .map(|c| c.text.as_str())
            .collect();
        assert!(inclusions.contains(&"PostgreSQL"));
        assert!(inclusions.contains(&"Include JWT"));
        assert!(!inclusions.contains(&"Use MongoDB"));
        assert_eq!(ledger.active_exclusions().len(), 0);
        assert!(ledger.superseded_targets().contains(&"Use MongoDB"));
    }

    #[test]
    fn test_sl2_domain_hysteresis() {
        let mut ledger = PromptStateLedger::new("led-sl2");
        let structured = sample_structured_prompt();
        ledger.apply_initial_structured(&structured, "practitioner", "build", 0.88, Vec::new());

        // Single-turn foreign domain signal must NOT flip locked domain.
        assert!(!ledger.should_override_domain("finance", 0.85, false));
        assert_eq!(
            ledger.locked_axis.as_ref().map(|a| a.domain.as_str()),
            Some("software")
        );

        // Low-confidence signal resets the pending streak.
        assert!(!ledger.should_override_domain("finance", 0.40, false));

        // Two consecutive high-confidence turns for the same foreign domain flip the lock.
        assert!(!ledger.should_override_domain("finance", 0.82, false));
        assert!(ledger.should_override_domain("finance", 0.84, false));
        assert_eq!(
            ledger.locked_axis.as_ref().map(|a| a.domain.as_str()),
            Some("finance")
        );

        // Explicit TopicReset overrides immediately in one turn.
        assert!(ledger.should_override_domain("healthcare", 0.70, true));
        assert_eq!(
            ledger.locked_axis.as_ref().map(|a| a.domain.as_str()),
            Some("healthcare")
        );
    }

    #[test]
    fn test_sl3_monotonic_fact_ids_and_dedup() {
        let mut ledger = PromptStateLedger::new("led-sl3");
        let structured = sample_structured_prompt();
        ledger.apply_initial_structured(&structured, "practitioner", "build", 0.9, Vec::new());

        let batch1 = ledger.register_facts(&[
            ("Q3 revenue reached $4.2M", "rag"),
            ("Latency SLA is 50ms", "user"),
        ]);
        assert_eq!(batch1.len(), 2);
        assert_eq!(batch1[0].id, "F1");
        assert_eq!(batch1[1].id, "F2");

        ledger.advance_turn();
        // Duplicate with extra whitespace must not mint a new F# ID; novel fact gets F3.
        let batch2 = ledger.register_facts(&[
            ("Q3   revenue reached  $4.2M", "rag"),
            ("Database target is PostgreSQL 16", "user"),
        ]);
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0].id, "F3");
        assert_eq!(ledger.facts.len(), 3);
        assert_eq!(ledger.facts[0].id, "F1");
        assert_eq!(ledger.facts[1].id, "F2");
        assert_eq!(ledger.facts[2].id, "F3");
        assert_eq!(ledger.facts[0].content_hash.len(), 64);
    }

    #[test]
    fn test_phase_activation_and_slot_resolution() {
        let mut ledger = PromptStateLedger::new("led-phase");
        let structured = sample_structured_prompt();
        ledger.apply_initial_structured(
            &structured,
            "practitioner",
            "build",
            0.9,
            vec!["deployment target".to_string()],
        );

        assert!(ledger.set_active_phase(2));
        assert_eq!(ledger.phases[0].status, PhaseStatus::Done);
        assert_eq!(ledger.phases[1].status, PhaseStatus::Active);
        assert!(!ledger.set_active_phase(99));

        assert!(ledger.resolve_slot("Deployment Target", "AWS ECS Fargate"));
        assert_eq!(ledger.open_slots.len(), 0);
        assert_eq!(
            ledger
                .resolved_slots
                .get("Deployment Target")
                .map(String::as_str),
            Some("AWS ECS Fargate")
        );
    }

    #[test]
    fn test_opposite_kind_constraint_supersedes_contradiction() {
        let mut ledger = PromptStateLedger::new("led-opp");
        ledger.advance_turn();
        ledger.apply_constraint_ops(&[ConstraintOp::AddInclusion {
            text: "Redis".to_string(),
        }]);
        assert_eq!(ledger.active_inclusions().len(), 1);
        assert_eq!(ledger.constraints[0].id, "C1");

        ledger.advance_turn();
        ledger.apply_constraint_ops(&[ConstraintOp::AddExclusion {
            text: "no Redis".to_string(),
        }]);
        assert_eq!(ledger.active_inclusions().len(), 0);
        assert_eq!(ledger.active_exclusions().len(), 1);
        assert_eq!(ledger.active_exclusions()[0].text, "no Redis");
        assert!(matches!(
            ledger.constraints[0].status,
            ConstraintStatus::Superseded { ref by_id } if by_id == "C2"
        ));

        ledger.advance_turn();
        ledger.apply_constraint_ops(&[ConstraintOp::AddInclusion {
            text: "Include Redis".to_string(),
        }]);
        assert_eq!(ledger.active_exclusions().len(), 0);
        assert_eq!(ledger.active_inclusions().len(), 1);
        assert!(matches!(
            ledger.constraints[1].status,
            ConstraintStatus::Superseded { ref by_id } if by_id == "C3"
        ));
    }
}
