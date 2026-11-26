//! Multi-turn parent-to-continuation ledger management and Balanced/Gentle turn reconciliation.
//!
//! // ARCH: ARCHITECTURE.md §6.7 — Tenant-and-user-scoped `PromptStateLedger` storage,
//! // bounded eviction via `OryxSyncGuard`, and non-destructive legacy continuation reconciliation.

use uuid::Uuid;

use crate::{
    npae::aggressive::constraints::extract_constraints,
    npae::aggressive::structurer::{render_delta_prompt, scrub_superseded_terms},
    npae::continuation::ledger::strip_directive_or_exclusion_prefix,
    npae::continuation::router::extract_non_constraint_delta_instruction,
    npae::continuation::{
        ConstraintKind, ConstraintOp, ConstraintStatus, ContinuationKind, ContinuationPath,
        ContinuationRouter, LockedAxis, ParentMatcher, PhaseStatus, PromptStateLedger,
    },
    npae::schema::types::{ConstraintView, ContinuationMeta, PhaseView},
    utils::{security::OryxSecuritySanitizer, sync::OryxSyncGuard, text::OryxTextMatcher},
    AppState,
};

/// Maximum number of `(tenant_id, user_id, ledger_id)` entries retained in `state.prompt_ledgers`.
pub(crate) const MAX_PROMPT_LEDGERS: usize = 10_000;

/// Builds the `(tenant_id, user_id, ledger_id)` composite key for `state.prompt_ledgers`.
fn scoped_ledger_key(tenant_id: &str, user_id_str: &str, ledger_id: &str) -> String {
    format!("{tenant_id}:{user_id_str}:{ledger_id}")
}

/// Looks up a tenant-and-user-scoped [`PromptStateLedger`] and its raw parent prompt.
pub(crate) fn lookup_scoped_continuation_ledger(
    state: &AppState,
    tenant_id: &str,
    user_id_str: &str,
    parent_id: Option<&str>,
) -> Option<(PromptStateLedger, String)> {
    let pid = parent_id.map(str::trim).filter(|s| !s.is_empty())?;
    let key = scoped_ledger_key(tenant_id, user_id_str, pid);
    state
        .prompt_ledgers
        .get(&key)
        .map(|entry| entry.value().clone())
}

/// Persists a tenant-and-user-scoped [`PromptStateLedger`] with bounded eviction.
pub(crate) fn store_scoped_continuation_ledger(
    state: &AppState,
    tenant_id: &str,
    user_id_str: &str,
    ledger: PromptStateLedger,
    parent_raw: String,
) {
    let key = scoped_ledger_key(tenant_id, user_id_str, &ledger.ledger_id);
    OryxSyncGuard::insert_into_bounded_dashmap(
        &state.prompt_ledgers,
        MAX_PROMPT_LEDGERS,
        key,
        (ledger, parent_raw),
    );
}

/// Appends preserved active constraints, sanitized resolved slots, non-markdown output format,
/// and active phase guidance onto `reconciled` when not already present.
fn append_preserved_ledger_directives(
    existing: &PromptStateLedger,
    target_phase: Option<u32>,
    reconciled: &mut String,
) {
    let base_low = reconciled.to_lowercase();
    let base_tokens = OryxTextMatcher::tokenize_alphanumeric_words(&base_low);
    let mut extra = Vec::new();

    for inc in existing.active_inclusions() {
        let inc_low = inc.text.to_lowercase();
        let inc_tokens = OryxTextMatcher::tokenize_alphanumeric_words(&inc_low);
        let core_tokens = OryxTextMatcher::tokenize_alphanumeric_words(
            strip_directive_or_exclusion_prefix(&inc_low),
        );
        if !OryxTextMatcher::contains_contiguous_token_span(&base_tokens, &inc_tokens)
            && (core_tokens.is_empty()
                || !OryxTextMatcher::contains_contiguous_token_span(&base_tokens, &core_tokens))
        {
            extra.push(inc.text.clone());
        }
    }

    for exc in existing.active_exclusions() {
        let exc_low = exc.text.to_lowercase();
        let exc_tokens = OryxTextMatcher::tokenize_alphanumeric_words(&exc_low);
        if !OryxTextMatcher::contains_contiguous_token_span(&base_tokens, &exc_tokens) {
            // FIX: Medium #3 — ensure exclusions without an explicit negation prefix are
            // prefixed with `"exclude "` before appending to `reconciled`.
            let starts_with_neg = ["no ", "without ", "never ", "avoid ", "exclude "]
                .iter()
                .any(|p| exc_low.starts_with(p));
            if starts_with_neg {
                extra.push(exc.text.clone());
            } else {
                extra.push(format!("exclude {}", exc.text));
            }
        }
    }

    if !existing.resolved_slots.is_empty() {
        for (k, v) in &existing.resolved_slots {
            if OryxSecuritySanitizer::has_prompt_injection_markers(k)
                || OryxSecuritySanitizer::has_prompt_injection_markers(v)
            {
                continue;
            }
            let clean_k =
                OryxSecuritySanitizer::escape_untrusted_xml_content(&k.replace(['[', ']'], ""));
            let clean_v =
                OryxSecuritySanitizer::escape_untrusted_xml_content(&v.replace(['[', ']'], ""));
            if !clean_k.is_empty()
                && !clean_v.is_empty()
                && !base_low.contains(&clean_v.to_lowercase())
            {
                extra.push(format!("{clean_k}: {clean_v}"));
            }
        }
    }

    // FIX: High #3 — preserve non-markdown output format & active phase guidance.
    if !existing.output_format.trim().is_empty()
        && !existing.output_format.eq_ignore_ascii_case("markdown")
        && !base_low.contains(&existing.output_format.to_lowercase())
    {
        extra.push(format!("Output format: {}", existing.output_format.trim()));
    }

    if let Some(p) = target_phase {
        if let Some(phase) = existing.phases.iter().find(|ph| ph.phase_number == p) {
            extra.push(format!(
                "Focus on Phase {} ({}): {}",
                phase.phase_number, phase.name, phase.description
            ));
        } else {
            extra.push(format!("Focus on Phase {p}"));
        }
    }

    if !extra.is_empty() {
        if !reconciled.is_empty() && !reconciled.ends_with(['.', '!', '?']) {
            reconciled.push('.');
        }
        reconciled.push(' ');
        reconciled.push_str(&extra.join("; "));
        reconciled.push('.');
    }
}

/// Builds a fresh legacy turn (`TopicReset` or `Initial`) with a newly seeded ledger.
fn build_fresh_legacy_ledger_turn(
    req: &crate::api::CompressRequest,
    scenario: Option<&crate::classifier::signal::ScenarioSignal>,
    path: ContinuationPath,
    kind: ContinuationKind,
) -> (String, PromptStateLedger, String, ContinuationMeta) {
    let mut fresh = PromptStateLedger::new(Uuid::new_v4().to_string());
    fresh.advance_turn();
    fresh.objective.clone_from(&req.raw_text);
    seed_legacy_ledger(&mut fresh, &req.raw_text, scenario);
    let meta = build_legacy_continuation_meta(
        &fresh,
        path,
        kind,
        vec![kind],
        req.raw_text.chars().take(160).collect(),
        None,
    );
    (req.raw_text.clone(), fresh, req.raw_text.clone(), meta)
}

/// Applies non-reset delta continuation state updates onto an existing parent ledger.
fn apply_delta_turn_to_ledger(
    mut existing: PromptStateLedger,
    parent_raw: String,
    delta: &crate::npae::continuation::diff::DeltaExtraction,
    req: &crate::api::CompressRequest,
) -> (String, PromptStateLedger, String, ContinuationMeta) {
    existing.advance_turn();

    let mut resolved_any = false;
    if let Some(ref slots) = req.resolved_slots {
        for (k, v) in slots {
            if existing.resolve_slot(k, v) {
                resolved_any = true;
            }
        }
    }
    if let Some(ref revoked) = req.revoked_constraints {
        if !revoked.is_empty() {
            let ops: Vec<ConstraintOp> = revoked
                .iter()
                .map(|t| ConstraintOp::Revoke { target: t.clone() })
                .collect();
            existing.apply_constraint_ops(&ops);
        }
    }

    let mut route = ContinuationRouter::classify(
        delta,
        &existing.open_slots,
        !existing.phases.is_empty(),
        req.repair_context.is_some(),
    );
    if resolved_any && !route.all_kinds.contains(&ContinuationKind::SlotFill) {
        route.all_kinds.push(ContinuationKind::SlotFill);
    }
    existing.apply_constraint_ops(&route.constraint_ops);
    if let Some(ref fmt) = route.output_format_override {
        existing.output_format.clone_from(fmt);
    }
    let target_phase = req.active_phase.or(route.target_phase);
    if let Some(p) = target_phase {
        if existing.set_active_phase(p) {
            for phase in &mut existing.phases {
                if phase.phase_number != p && phase.status == PhaseStatus::Pending {
                    phase.status = PhaseStatus::Deferred;
                }
            }
        }
    }

    let mut scrub_list: Vec<String> = existing
        .superseded_targets()
        .into_iter()
        .map(str::to_string)
        .collect();
    for entry in &existing.constraints {
        if entry.status == ConstraintStatus::Revoked
            && !scrub_list
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&entry.text))
        {
            scrub_list.push(entry.text.clone());
        }
    }
    let scrub_refs: Vec<&str> = scrub_list.iter().map(String::as_str).collect();
    let raw_base = if delta.path == ContinuationPath::EditContinuation {
        req.raw_text.as_str()
    } else {
        existing.objective.as_str()
    };
    let mut reconciled = scrub_superseded_terms(raw_base, &scrub_refs);
    if delta.path == ContinuationPath::EditContinuation {
        existing.objective.clone_from(&reconciled);
    }

    append_preserved_ledger_directives(&existing, target_phase, &mut reconciled);

    if delta.path == ContinuationPath::DeltaFollowUp {
        if let Some(follow_up) =
            extract_non_constraint_delta_instruction(&delta.delta_text, &existing)
        {
            let scrubbed_follow_up = scrub_superseded_terms(&follow_up, &scrub_refs);
            let trimmed_follow_up = scrubbed_follow_up.trim();
            if !trimmed_follow_up.is_empty()
                && !reconciled
                    .to_lowercase()
                    .contains(&trimmed_follow_up.to_lowercase())
            {
                reconciled.push_str("\nFollow-up instruction: ");
                reconciled.push_str(trimmed_follow_up);
            }
        }
    }

    let delta_prompt = render_delta_prompt(&existing, &route, &[], &req.raw_text);
    let meta = build_legacy_continuation_meta(
        &existing,
        delta.path,
        route.primary_kind,
        route.all_kinds,
        route.delta_summary,
        Some(delta_prompt),
    );
    let parent_raw_to_store = if delta.path == ContinuationPath::EditContinuation {
        req.raw_text.clone()
    } else {
        parent_raw
    };
    (reconciled, existing, parent_raw_to_store, meta)
}

/// Reconciles parent-ledger state for Balanced/Gentle/Auto pipeline continuations.
///
/// // ARCH: ARCHITECTURE.md §6.7 — preserves active constraints, resolved slots, output format,
/// // active phase guidance, and non-constraint Path A follow-up instructions into `reconciled`.
pub(crate) fn reconcile_legacy_continuation_turn(
    state: &AppState,
    user_id_str: &str,
    tenant_id: &str,
    req: &crate::api::CompressRequest,
    scenario: Option<&crate::classifier::signal::ScenarioSignal>,
) -> (String, PromptStateLedger, String, ContinuationMeta) {
    let found_parent =
        lookup_scoped_continuation_ledger(state, tenant_id, user_id_str, req.parent_id.as_deref());

    if let Some((existing, parent_raw)) = found_parent {
        let delta = ParentMatcher::extract_delta(&parent_raw, &req.raw_text, true);
        if delta.path != ContinuationPath::TopicReset {
            return apply_delta_turn_to_ledger(existing, parent_raw, &delta, req);
        }

        return build_fresh_legacy_ledger_turn(
            req,
            scenario,
            ContinuationPath::TopicReset,
            ContinuationKind::TopicReset,
        );
    }

    build_fresh_legacy_ledger_turn(
        req,
        scenario,
        ContinuationPath::Initial,
        ContinuationKind::Initial,
    )
}

/// Seeds constraints and initial 3-axis classification into a freshly created legacy ledger.
fn seed_legacy_ledger(
    ledger: &mut PromptStateLedger,
    raw_text: &str,
    scenario: Option<&crate::classifier::signal::ScenarioSignal>,
) {
    let (inclusions, exclusions) = extract_constraints(raw_text, &[], &[]);
    let mut ops = Vec::new();
    for inc in inclusions {
        ops.push(ConstraintOp::AddInclusion { text: inc });
    }
    for exc in exclusions {
        ops.push(ConstraintOp::AddExclusion { text: exc });
    }
    ledger.apply_constraint_ops(&ops);
    if let Some(sig) = scenario {
        ledger.locked_axis = Some(LockedAxis {
            domain: sig.domain.clone(),
            perspective: "practitioner".to_string(),
            intent: sig.intent_class.clone(),
            confidence: sig.confidence.clamp(0.0, 1.0),
        });
    }
}

/// Builds [`ContinuationMeta`] from a [`PromptStateLedger`] and route metadata.
pub(crate) fn build_legacy_continuation_meta(
    ledger: &PromptStateLedger,
    path: ContinuationPath,
    primary_kind: ContinuationKind,
    all_kinds: Vec<ContinuationKind>,
    delta_summary: String,
    delta_prompt: Option<String>,
) -> ContinuationMeta {
    let active_constraints = ledger
        .constraints
        .iter()
        .filter(|c| c.status == ConstraintStatus::Active)
        .map(|c| ConstraintView {
            id: c.id.clone(),
            kind: match c.kind {
                ConstraintKind::Inclusion => "inclusion",
                ConstraintKind::Exclusion => "exclusion",
                ConstraintKind::Format => "format",
                ConstraintKind::LengthBound => "length_bound",
                ConstraintKind::Tone => "tone",
            }
            .to_string(),
            text: c.text.clone(),
            status: "active".to_string(),
        })
        .collect();
    let superseded_constraints = ledger
        .superseded_targets()
        .into_iter()
        .map(str::to_string)
        .collect();
    let phases = ledger
        .phases
        .iter()
        .map(|p| PhaseView {
            phase_number: p.phase_number,
            name: p.name.clone(),
            status: match p.status {
                PhaseStatus::Pending => "pending",
                PhaseStatus::Active => "active",
                PhaseStatus::Done => "done",
                PhaseStatus::Deferred => "deferred",
            }
            .to_string(),
        })
        .collect();
    let active_phase = ledger
        .phases
        .iter()
        .find(|p| p.status == PhaseStatus::Active)
        .map(|p| p.phase_number);

    ContinuationMeta {
        ledger_id: ledger.ledger_id.clone(),
        turn_number: ledger.turn_count.max(1),
        path,
        primary_kind,
        all_kinds,
        delta_summary,
        active_constraints,
        superseded_constraints,
        phases,
        active_phase,
        open_slots: ledger.open_slots.clone(),
        delta_prompt,
        resolved_slots: ledger.resolved_slots.clone(),
    }
}
