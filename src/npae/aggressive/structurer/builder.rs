//! Single-Pass `BuildContext` & `StructuredPrompt` Assembly.
//!
//! // ARCH: ARCHITECTURE.md §1.4, §5 GAP-35 & §6.6 — precomputes per-request `BuildContext`
//! // once and assembles `StructuredPrompt` from config-driven templates and Ory blueprints.

use super::prune::extract_task_description;
use super::templates::{
    derive_dynamic_instruction_with_ctx, derive_output_blueprint_sections_with_ctx,
    derive_persona_anchor_with_ctx, derive_persona_guardrails, infer_assumptions_with_ctx,
    infer_execution_phases_with_ctx, infer_success_criteria_with_ctx,
    infer_validation_steps_with_ctx, unpack_context_with_ctx,
};
use crate::npae::aggressive::intent::{IntentClass, IntentProfile};
use crate::npae::aggressive::rag_context::DerivedRagContext;
use crate::npae::grounding::verifier::sanitize_untrusted_fact;
use crate::npae::schema::types::{
    ConstraintsMeta, DeliverableType, ExecutionPhase, HallucinationGuardConfig, LengthBound,
    OutputSpec, PromptConstraints, PromptContext, PromptRole, StructuredPrompt, ValidationStep,
};

/// Builds a `StructuredPrompt` without an Ory `DynamicBlueprint`.
///
/// # Errors
/// Returns `Err` if structure assembly fails.
///
/// # Panics
/// Never.
pub fn build(
    profile: &IntentProfile,
    raw: &str,
    resolved: &crate::npae::aggressive::resolver::ResolvedPrompt,
    config: Option<&crate::npae::aggressive::config::UnifiedConfig>,
    reconstructed: &crate::types::ReconstructedInput,
    derived_rag: Option<&DerivedRagContext>,
) -> std::result::Result<StructuredPrompt, String> {
    build_with_blueprint(
        profile,
        raw,
        resolved,
        config,
        reconstructed,
        derived_rag,
        None,
    )
}

/// Precomputed per-request build context to eliminate redundant lowercasing, rule evaluation,
/// and taxonomy lookups across `build_with_blueprint` helpers.
///
/// // ARCH: ARCHITECTURE.md §6.6 — computes `raw_lower`, `word_count`, `rule_verdict`,
/// // `canonical_domain`, and wealth/business gating once per request.
pub(super) struct BuildContext<'a> {
    pub(super) raw_lower: String,
    pub(super) word_count: usize,
    pub(super) rule_verdict: Option<crate::classifier::intent_rule::RoleRuleVerdict>,
    pub(super) canonical_domain: String,
    pub(super) domain_taxonomy_entry: Option<&'a crate::npae::aggressive::config::DomainTaxonomy>,
    pub(super) domain_in_config_taxonomy: bool,
    pub(super) is_wealth_query: bool,
    pub(super) is_enterprise_business: bool,
}

impl<'a> BuildContext<'a> {
    #[inline]
    pub(super) fn is_long_narrative(&self) -> bool {
        self.word_count > 150
    }

    pub(super) fn new(
        profile: &IntentProfile,
        raw: &str,
        config: Option<&'a crate::npae::aggressive::config::UnifiedConfig>,
    ) -> Self {
        let raw_lower = raw.to_lowercase();
        let word_count = raw.split_whitespace().count();
        let rule_verdict = crate::classifier::intent_rule::resolve_prompt_rules(raw);

        let domain_taxonomy_entry = config.and_then(|cfg| cfg.lookup_domain(&profile.domain));
        let domain_in_config_taxonomy = match config {
            Some(_) => domain_taxonomy_entry.is_some(),
            None => true,
        };
        let canonical_domain = match domain_taxonomy_entry {
            Some(tax) => tax.domain.clone(),
            None => match config {
                Some(cfg) => crate::npae::aggressive::config::normalize_domain(
                    &profile.domain,
                    &cfg.domain_taxonomy,
                )
                .to_owned(),
                None => profile.domain.clone(),
            },
        };

        let is_wealth_query =
            compute_is_wealth_engineering_query(profile, &raw_lower, rule_verdict.as_ref());
        let is_enterprise_business = is_enterprise_business_query(profile);

        Self {
            raw_lower,
            word_count,
            rule_verdict,
            canonical_domain,
            domain_taxonomy_entry,
            domain_in_config_taxonomy,
            is_wealth_query,
            is_enterprise_business,
        }
    }
}

/// Builds a structured prompt, incorporating an optional dynamic blueprint from OryEngine.
///
/// // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — curated canonical domain templates
/// // (`infer_execution_phases` & `infer_validation_steps`) take precedence over generic
/// // Ory blueprint phases, and `Content` / `Explain` prompts do not receive forced phases.
///
/// # Arguments
/// * `profile` - Intent profile derived from classification
/// * `raw` - Raw prompt text
/// * `resolved` - Pre-resolved persona and inclusions
/// * `config` - Optional unified config
/// * `reconstructed` - Tokens and constraint locks from Stage -1
/// * `derived_rag` - Optional extracted RAG facts
/// * `blueprint` - Optional dynamic blueprint synthesized by OryArchitect
///
/// # Errors
/// Returns error string on formatting or structure build failure.
///
/// # Panics
/// Never.
pub fn build_with_blueprint(
    profile: &IntentProfile,
    raw: &str,
    resolved: &crate::npae::aggressive::resolver::ResolvedPrompt,
    config: Option<&crate::npae::aggressive::config::UnifiedConfig>,
    reconstructed: &crate::types::ReconstructedInput,
    derived_rag: Option<&DerivedRagContext>,
    blueprint: Option<&crate::npae::ory::types::DynamicBlueprint>,
) -> std::result::Result<StructuredPrompt, String> {
    let ctx = BuildContext::new(profile, raw, config);
    let role_primary = resolved.role.clone();
    let mut inclusions = resolved.inclusions.clone();
    let forbidden = resolved.forbidden.clone();

    // Inject Structural Locks from Stage -1
    for lock in &reconstructed.constraint_locks {
        if !inclusions.contains(&lock.text) {
            inclusions.push(lock.text.clone());
        }
    }

    let user_level_str = match profile.user_knowledge {
        crate::npae::aggressive::intent::KnowledgeLevel::Novice => "novice",
        crate::npae::aggressive::intent::KnowledgeLevel::Intermediate => "intermediate",
        crate::npae::aggressive::intent::KnowledgeLevel::Expert => "expert",
    }
    .to_string();

    let intent_type = match profile.primary_intent {
        IntentClass::Build => {
            if matches!(
                profile.domain.as_str(),
                "software"
                    | "software-engineering"
                    | "devops"
                    | "devops-infra"
                    | "computers"
                    | "ai-ml"
                    | "data-science"
                    | "cybersecurity"
            ) {
                "development/construction"
            } else {
                "creation/design"
            }
        }
        IntentClass::Explain => "educational/instructional",
        IntentClass::Debug => "troubleshooting/optimization",
        IntentClass::Analyze => "analytical/discovery",
        IntentClass::Transform => "refactoring/conversion",
    };

    // Extract a concise task description (not just raw first line)
    let description = extract_task_description(raw, intent_type);

    // Build a non-redundant background — includes goal decomposition, context unpacking, and RAG facts
    let mut background = build_background(profile, &user_level_str, &ctx);
    if let Some(rag) = derived_rag {
        if !rag.facts.is_empty() {
            // FIX: G-07 — Sanitize each retrieved RAG fact before embedding into `Known facts:`
            // so untrusted text cannot smuggle raw XML tags or multiline directives into Crisp mode.
            let sanitized_facts: Vec<String> = rag
                .facts
                .iter()
                .map(|f| sanitize_untrusted_fact(f))
                .collect();
            let facts_str = format!("Known facts: {}", sanitized_facts.join("; "));
            if background.is_empty() {
                background = facts_str;
            } else {
                background = format!("{background}. {facts_str}");
            }
        }
    }

    // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — Always compute curated domain phases and
    // validation steps first. Curated canonical templates (`config/unified.json` `phase_templates`
    // and domain-specific structurer rules) take precedence over generic `DynamicBlueprint` phases,
    // and `Content` or `Explain` prompts must not be forced into multi-phase execution roadmaps.
    // When `config` is explicitly provided without the domain in `config.domain_taxonomy` (e.g.,
    // novel-domain audit / empty taxonomy test where Ory synthesized a `NovelDesign` blueprint),
    // allow the Ory `DynamicBlueprint` phases and validation checklist to flow through.
    let curated_phases = infer_execution_phases_with_ctx(profile, &ctx);
    let curated_steps = infer_validation_steps_with_ctx(profile, &ctx);
    let domain_in_config_taxonomy = ctx.domain_in_config_taxonomy;

    let suppress_phases = (profile.deliverable_type == DeliverableType::Content
        && !profile.has_phases
        && !profile.has_timeline)
        || (profile.primary_intent == IntentClass::Explain && curated_phases.is_empty());

    let execution_phases = if suppress_phases {
        vec![]
    } else if !curated_phases.is_empty() && (domain_in_config_taxonomy || blueprint.is_none()) {
        curated_phases
    } else if let Some(bp) = blueprint {
        bp.phases
            .iter()
            .enumerate()
            .map(|(idx, p)| ExecutionPhase {
                phase_number: (idx + 1) as u8,
                name: p.name.clone(),
                description: p.description.clone(),
                estimated_duration: format!("Complexity {:.1}/10", p.estimated_complexity),
                deliverables: p.expected_deliverables.clone(),
            })
            .collect()
    } else {
        curated_phases
    };

    let is_long_narrative = ctx.is_long_narrative();

    let validation_steps = if is_long_narrative && profile.primary_intent == IntentClass::Explain {
        vec![]
    } else if !curated_steps.is_empty() {
        curated_steps
    } else if let Some(bp) = blueprint {
        if profile.has_validation || !domain_in_config_taxonomy {
            bp.validation_checklist
                .iter()
                .enumerate()
                .map(|(idx, check)| ValidationStep {
                    step_number: (idx + 1) as u8,
                    name: format!("Verification Step {}", idx + 1),
                    criteria: check.clone(),
                    validation_method: "Automated & Human Review".to_string(),
                })
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Infer success criteria (Intent-driven)
    let mut success_criteria = infer_success_criteria_with_ctx(profile, &ctx);
    if is_long_narrative && profile.primary_intent == IntentClass::Explain {
        success_criteria.truncate(1);
    }

    // Infer implicit assumptions based on context/domain
    let assumptions = if is_long_narrative
        && profile.primary_intent == IntentClass::Explain
        && profile.domain == "real-estate"
        && profile.perspective == crate::npae::aggressive::intent::ActorPerspective::Consumer
    {
        vec![
            "State clear example assumptions for budget, credit tier, down payment %, and attorney vs. escrow state closing customs rather than relying on broad national averages".to_string(),
        ]
    } else {
        infer_assumptions_with_ctx(profile, &ctx)
    };

    // Build dynamic instruction
    // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — only append Ory `bp.rationale` when the domain is
    // genuinely novel/unconfigured (`!domain_in_config_taxonomy`, e.g. `tests/ory_test.rs` with
    // empty `domain_taxonomy`), preventing internal `DomainTransfer` telemetry from leaking into
    // canonical configured domains.
    let mut dynamic_instruction = derive_dynamic_instruction_with_ctx(profile, &ctx);
    if !domain_in_config_taxonomy {
        if let Some(bp) = blueprint {
            if !bp.rationale.is_empty() {
                if dynamic_instruction.is_empty() {
                    dynamic_instruction = format!("Architecture Strategy: {}", bp.rationale);
                } else {
                    dynamic_instruction = format!(
                        "{}\nArchitecture Strategy: {}",
                        dynamic_instruction, bp.rationale
                    );
                }
            }
        }
    }

    // Build constraints meta from detected signals
    let constraints_meta = build_constraints_meta(profile);

    // Derive appropriate tone from domain + knowledge level
    let tone = derive_tone(profile);

    // Derive high-resolution persona anchor
    let persona_anchor = if let Some(ref v) = ctx.rule_verdict {
        v.persona_anchor()
    } else {
        derive_persona_anchor_with_ctx(
            &profile.domain,
            &role_primary,
            profile.perspective,
            ctx.domain_taxonomy_entry,
        )
    };
    let secondary = ctx
        .rule_verdict
        .as_ref()
        .and_then(|v| v.display_secondary())
        .or_else(|| {
            profile
                .vertical
                .as_deref()
                .map(crate::npae::aggressive::role::to_title_case)
        })
        .or_else(|| {
            if profile.secondary_domains.is_empty() {
                None
            } else {
                Some(
                    profile
                        .secondary_domains
                        .iter()
                        .map(|d| crate::npae::aggressive::role::to_title_case(d))
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
        });

    let mut expertise_domains = vec![profile.domain.clone()];
    for sec in &profile.secondary_domains {
        if !expertise_domains.contains(sec) {
            expertise_domains.push(sec.clone());
        }
    }

    // GAP-13: Derive length bounds from input complexity and intent
    let word_count = ctx.word_count;
    let length_bound = match (&profile.primary_intent, word_count) {
        (crate::npae::aggressive::intent::IntentClass::Explain, 0..=15) => {
            LengthBound { min: 100, max: 500 }
        }
        (crate::npae::aggressive::intent::IntentClass::Explain, _) => LengthBound {
            min: 200,
            max: 1000,
        },
        (crate::npae::aggressive::intent::IntentClass::Build, 0..=30) => LengthBound {
            min: 300,
            max: 1500,
        },
        (crate::npae::aggressive::intent::IntentClass::Build, _) => LengthBound {
            min: 500,
            max: 3000,
        },
        (crate::npae::aggressive::intent::IntentClass::Debug, _) => {
            LengthBound { min: 150, max: 800 }
        }
        (crate::npae::aggressive::intent::IntentClass::Analyze, _) => LengthBound {
            min: 300,
            max: 2000,
        },
        (crate::npae::aggressive::intent::IntentClass::Transform, _) => LengthBound {
            min: 100,
            max: 1000,
        },
    };

    let legal_as_constraint = profile.legal_as_constraint
        || ctx
            .rule_verdict
            .as_ref()
            .is_some_and(|v| v.legal_as_constraint);

    Ok(StructuredPrompt {
        role: PromptRole {
            primary: role_primary,
            secondary,
            expertise_domains,
            persona_constraints: derive_persona_guardrails(profile, &ctx, legal_as_constraint),
            persona_anchor,
        },
        objective: None,
        context: PromptContext {
            domain: profile.domain.clone(),
            description,
            background,
            user_knowledge_level: user_level_str,
            audience: "general".to_string(),
            temporal_scope: profile.temporal_scope.clone(),
            intent_vector: vec![profile.confidence],
            assumptions,
        },
        constraints: PromptConstraints {
            output_format: profile.output_preference.clone(),
            length_bound: length_bound.clone(),
            forbidden_topics: forbidden,
            required_inclusions: inclusions.clone(),
            tone,
        },
        hallucination_guard: HallucinationGuardConfig {
            self_critique_enabled: true,
            confidence_threshold: 0.75,
            contradiction_check: true,
            claim_verification_rules: vec!["no_invented_apis".into(), "cite_uncertainty".into()],
            uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
        },
        // ARCH: ARCHITECTURE.md §5 & G-08 — Populate structured `OutputSpec` contract from
        // derived format, length bounds, execution phases (or intent-specific response blueprint
        // sections when phases are omitted), and mandatory inclusions.
        output_spec: Some(OutputSpec {
            format: profile.output_preference.clone(),
            sections: if execution_phases.is_empty() {
                derive_output_blueprint_sections_with_ctx(profile, &ctx)
            } else {
                execution_phases.iter().map(|p| p.name.clone()).collect()
            },
            word_count: length_bound,
            mandatory_elements: inclusions,
        }),
        execution_phases,
        validation_steps,
        success_criteria,
        constraints_meta,
        dynamic_instruction,
    })
}

/// Build background that adds NEW information — includes context unpacking for vagueness resolution
fn build_background(profile: &IntentProfile, user_level: &str, ctx: &BuildContext<'_>) -> String {
    let mut parts = Vec::new();
    let is_long_narrative = ctx.is_long_narrative();

    parts.push(format!(
        "Domain: {} | Audience: {}",
        profile.domain, user_level
    ));

    if let Some(ref baseline) = profile.baseline_knowledge {
        parts.push(format!("User Baseline: {}", baseline));
    }

    if !is_long_narrative {
        // Vagueness Resolution / Context Unpacking
        if let Some(roadmap) = unpack_context_with_ctx(profile, ctx) {
            parts.push(format!("Roadmap: {}", roadmap));
        }

        if profile.has_timeline {
            parts.push(format!("Timeline: {}", profile.temporal_scope));
        }
        if profile.has_phases {
            parts.push("Phased execution requested".to_string());
        }
        if profile.has_validation {
            parts.push("Validation/QA gates required".to_string());
        }
        if let Some(ref biz) = profile.detected_biz_type {
            parts.push(format!("Business type: {}", biz));
        }
    }

    parts.join(". ")
}

/// Returns `true` when `text_lower` contains `needle` on word boundaries, preventing substring
/// false positives such as `"learn"` matching `"earn"` or `"appraisal"` matching `"app"`.
pub(super) fn has_whole_word_or_phrase(text_lower: &str, needle: &str) -> bool {
    crate::utils::text::OryxTextMatcher::contains_whole_word_or_phrase(text_lower, needle)
}

/// Returns `true` when the prompt is genuinely a wealth-engineering / net-worth compounding query,
/// strictly gated by the deterministic rule layer or `(domain in finance/business && perspective != Consumer)`
/// so consumer homebuying, education (`"learn"`), or travel prompts with budgets (`"$500k"`) are never hijacked.
///
/// // ARCH: ARCHITECTURE.md §1.4 & §6.6 — domain-conditioned feature gating using precomputed `raw_lower`
/// // and `rule_verdict` so `resolve_prompt_rules` is never re-invoked per helper.
fn compute_is_wealth_engineering_query(
    profile: &IntentProfile,
    raw_lower: &str,
    rule_verdict: Option<&crate::classifier::intent_rule::RoleRuleVerdict>,
) -> bool {
    if profile.is_wealth_build()
        || rule_verdict
            .is_some_and(|v| v.intent == crate::classifier::intent_rule::GoalIntent::WealthBuild)
    {
        return true;
    }

    if !matches!(profile.domain.as_str(), "finance" | "business")
        || profile.perspective == crate::npae::aggressive::intent::ActorPerspective::Consumer
    {
        return false;
    }

    let wealth_phrases = [
        "wealth",
        "million",
        "billion",
        "financial freedom",
        "passive income",
        "retire early",
    ];
    wealth_phrases
        .iter()
        .any(|p| has_whole_word_or_phrase(raw_lower, p))
        || raw_lower.contains("make $")
}

#[cfg(test)]
pub(super) fn is_wealth_engineering_query(profile: &IntentProfile, raw: &str) -> bool {
    let raw_lower = raw.to_lowercase();
    let rule_verdict = crate::classifier::intent_rule::resolve_prompt_rules(raw);
    compute_is_wealth_engineering_query(profile, &raw_lower, rule_verdict.as_ref())
}

/// Returns `true` when the prompt is an enterprise / venture business query (`Practitioner` or `Investor`
/// in `"business"` / `"business-strategy"`), preventing cross-domain hijacks on non-business prompts.
///
/// // ARCH: ARCHITECTURE.md §1.4 & §6 — domain-conditioned feature gating for enterprise business templates.
pub(super) fn is_enterprise_business_query(profile: &IntentProfile) -> bool {
    matches!(profile.domain.as_str(), "business" | "business-strategy")
        && profile.perspective != crate::npae::aggressive::intent::ActorPerspective::Consumer
}

/// Build constraints meta from detected signals in profile
fn build_constraints_meta(profile: &IntentProfile) -> Option<ConstraintsMeta> {
    let meta = ConstraintsMeta {
        geography: profile.detected_geography.clone(),
        risk_tolerance: profile.detected_risk.clone(),
        business_type: profile.detected_biz_type.clone(),
        revenue_expectations: None, // future: extract from raw input
        team_composition: profile.detected_team.clone(),
    };

    // Only include if at least one field is populated
    if meta.geography.is_some()
        || meta.risk_tolerance.is_some()
        || meta.business_type.is_some()
        || meta.team_composition.is_some()
    {
        Some(meta)
    } else {
        None
    }
}

/// Derive appropriate tone from domain and knowledge level
fn derive_tone(profile: &IntentProfile) -> String {
    match (profile.domain.as_str(), &profile.user_knowledge) {
        ("business" | "business-strategy", _) => "strategic-actionable-direct".to_string(),
        ("creative" | "creative-writing", _) => "creative-expressive-engaging".to_string(),
        ("education", crate::npae::aggressive::intent::KnowledgeLevel::Novice) => {
            "clear-supportive-step-by-step".to_string()
        }
        ("legal", _) => "precise-formal-referenced".to_string(),
        ("marketing", _) => "persuasive-data-driven-concise".to_string(),
        (_, crate::npae::aggressive::intent::KnowledgeLevel::Expert) => {
            "technical-precise-concise".to_string()
        }
        (_, crate::npae::aggressive::intent::KnowledgeLevel::Novice) => {
            "clear-approachable-detailed".to_string()
        }
        _ => "professional-thorough-structured".to_string(),
    }
}
