//! Config-Driven Domain Template Resolution & Fallback Tables.
//!
//! // ARCH: ARCHITECTURE.md §1.4 & §6.6 — resolves perspective-keyed templates
//! // (`"{perspective}:{intent}"` -> `"{perspective}"` -> `"{intent}"` -> `"default"`)
//! // from `DomainTaxonomy` in `config/unified.json` first, falling back to built-in tables
//! // when unconfigured or when `config` is `None`.

use std::collections::HashMap;

use super::builder::{has_whole_word_or_phrase, BuildContext};
use crate::npae::aggressive::intent::{ActorPerspective, IntentClass, IntentProfile};
use crate::npae::schema::types::{ExecutionPhase, ValidationStep};

/// Returns the canonical lowercase key for an [`IntentClass`].
#[inline]
pub(super) fn intent_key(intent: &IntentClass) -> &'static str {
    match intent {
        IntentClass::Build => "build",
        IntentClass::Analyze => "analyze",
        IntentClass::Explain => "explain",
        IntentClass::Debug => "debug",
        IntentClass::Transform => "transform",
    }
}

/// Resolves a perspective-and-intent-keyed template from a `HashMap` in priority order:
/// `"{perspective}:{intent}"` -> `"{perspective}"` -> `"{intent}"` -> `"default"`.
pub(super) fn resolve_template_key<'a, T>(
    map: &'a HashMap<String, T>,
    perspective: ActorPerspective,
    intent: &IntentClass,
) -> Option<&'a T> {
    let ikey = intent_key(intent);
    let pkey = perspective.as_key();
    let composite = format!("{pkey}:{ikey}");
    map.get(&composite)
        .or_else(|| map.get(pkey))
        .or_else(|| map.get(ikey))
        .or_else(|| map.get("default"))
}

/// Derives persona guardrail keys from `DomainTaxonomy::persona_guardrails` first,
/// falling back to built-in domain guardrail rules and `legal_as_constraint`.
pub(super) fn derive_persona_guardrails(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
    legal_as_constraint: bool,
) -> Vec<String> {
    let mut constraints = Vec::new();
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref cfg_guardrails) = tax.persona_guardrails {
            for g in cfg_guardrails {
                if !constraints.iter().any(|existing| existing == g) {
                    constraints.push(g.clone());
                }
            }
        }
    }
    if constraints.is_empty() {
        for dom in std::iter::once(&profile.domain).chain(profile.secondary_domains.iter()) {
            let dom_c: &[&str] = match dom.as_str() {
                "software-engineering" | "devops-infra" | "software" | "devops" => {
                    &["production_grade_only", "no_placeholder_code"]
                }
                "medical" | "pharma" | "health" => &[
                    "evidence_based_only",
                    "cite_sources_required",
                    "no_medical_advice_disclaimer",
                ],
                "legal" => &["jurisdiction_aware", "cite_statutes"],
                "creative" => &["maintain_narrative_voice", "show_dont_tell"],
                _ => &["accurate_and_thorough"],
            };
            for &c in dom_c {
                if !constraints.iter().any(|existing| existing == c) {
                    constraints.push(c.to_string());
                }
            }
        }
    }
    if legal_as_constraint {
        if !constraints.iter().any(|c| c == "jurisdiction_aware") {
            constraints.push("jurisdiction_aware".into());
        }
        if !constraints.iter().any(|c| c == "regulatory_compliance") {
            constraints.push("regulatory_compliance".into());
        }
    }
    constraints
}

/// Derives output blueprint sections from `DomainTaxonomy::output_blueprints` first,
/// falling back to built-in intent/domain sections.
pub(super) fn derive_output_blueprint_sections_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Vec<String> {
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref blueprints) = tax.output_blueprints {
            if let Some(sections) =
                resolve_template_key(blueprints, profile.perspective, &profile.primary_intent)
            {
                if !sections.is_empty() {
                    return sections.clone();
                }
            }
        }
    }
    derive_output_blueprint_sections(&profile.primary_intent, &profile.domain)
}

/// Derives intent-specific response blueprint sections when no multi-phase execution roadmap
/// is emitted, guiding downstream LLMs to produce structured, high-signal answers.
fn derive_output_blueprint_sections(intent: &IntentClass, domain: &str) -> Vec<String> {
    match intent {
        IntentClass::Build => match domain {
            "software"
            | "software-engineering"
            | "devops"
            | "devops-infra"
            | "computers"
            | "data-science"
            | "ai-ml" => vec![
                "Architecture & Trade-offs".into(),
                "Complete Implementation".into(),
                "Edge Cases & Failure Modes".into(),
                "Verification & Tests".into(),
            ],
            _ => vec![
                "Core Design".into(),
                "Actionable Deliverable".into(),
                "Key Constraints & Risks".into(),
                "Verification Checklist".into(),
            ],
        },
        IntentClass::Debug => vec![
            "Symptom Triage".into(),
            "Ranked Root-Cause Hypotheses".into(),
            "Minimal Verified Fix".into(),
            "Regression Guardrails".into(),
        ],
        IntentClass::Analyze => vec![
            "Executive Verdict".into(),
            "Comparison / Trade-off Matrix".into(),
            "Quantified Risks & Edge Cases".into(),
            "Prioritized Action Plan".into(),
        ],
        IntentClass::Explain => vec![
            "Core Intuition".into(),
            "Mental Model".into(),
            "Step-by-Step Mechanism".into(),
            "Worked Example".into(),
            "Common Misconceptions".into(),
        ],
        IntentClass::Transform => vec![
            "Preservation Invariants".into(),
            "Transformed Output".into(),
            "Change Summary".into(),
        ],
    }
}

fn naturalize_subject(
    subject: &str,
    intent: &crate::npae::aggressive::intent::IntentClass,
) -> String {
    let lower = subject.to_lowercase();
    let is_garbage = lower.contains("no ")
        || lower == "no"
        || lower.contains("not ")
        || lower == "not"
        || lower.contains("without ")
        || lower == "without"
        || lower.contains("under ")
        || lower.contains('{')
        || lower.contains('}')
        || lower
            .chars()
            .all(|c| c.is_numeric() || !c.is_alphanumeric());

    if is_garbage {
        return match intent {
            crate::npae::aggressive::intent::IntentClass::Build => "system".to_string(),
            crate::npae::aggressive::intent::IntentClass::Analyze => "framework".to_string(),
            crate::npae::aggressive::intent::IntentClass::Explain => "concept".to_string(),
            _ => "solution".to_string(),
        };
    }

    let abstract_terms = [
        "industry",
        "domain",
        "sector",
        "field",
        "tech",
        "technology",
        "business",
        "market",
        "point",
        "topic",
        "thing",
        "stuff",
        "it",
        "this",
        "that",
    ];

    // FIX: Preserve concrete 1-2 word entities (e.g. "Redis", "OAuth2", "Rust", "Kafka", "RAG")
    // instead of replacing them with generic "system"/"concept". Only neutralize generic abstract
    // placeholders or single-character fragments.
    let trimmed_low = lower.trim();
    let is_abstract =
        trimmed_low.len() < 2 || abstract_terms.iter().any(|&term| trimmed_low == term);

    if !is_abstract {
        return subject.to_string();
    }

    match intent {
        crate::npae::aggressive::intent::IntentClass::Build => "system".to_string(),
        crate::npae::aggressive::intent::IntentClass::Analyze => "framework".to_string(),
        crate::npae::aggressive::intent::IntentClass::Explain => "concept".to_string(),
        _ => "solution".to_string(),
    }
}

/// Returns `true` when a technical `Build` prompt asks for a direct code/config artifact
/// (function, script, query, Dockerfile, rate limiter, parser, etc.) rather than a multi-week
/// project roadmap.
fn is_direct_artifact_build(raw_lower: &str, domain: &str) -> bool {
    if !matches!(
        domain,
        "software"
            | "software-engineering"
            | "devops"
            | "devops-infra"
            | "computers"
            | "data-science"
            | "ai-ml"
    ) {
        return false;
    }
    let artifact_markers = [
        "write a ",
        "write an ",
        "implement a ",
        "implement an ",
        "code a ",
        "script",
        "function",
        "class",
        "struct",
        "module",
        "endpoint",
        "sql query",
        "regex",
        "dockerfile",
        "yaml",
        "rate limiter",
        "parser",
        "algorithm",
        "cli tool",
        "webhook",
        "middleware",
        "unit test",
        "snippet",
    ];
    artifact_markers.iter().any(|m| raw_lower.contains(m))
}

/// Infer execution phases from temporal scope, config templates, and raw prompt.
#[cfg(test)]
pub(super) fn infer_execution_phases(
    profile: &IntentProfile,
    raw: &str,
    config: Option<&crate::npae::aggressive::config::UnifiedConfig>,
) -> Vec<ExecutionPhase> {
    let ctx = BuildContext::new(profile, raw, config);
    infer_execution_phases_with_ctx(profile, &ctx)
}

/// Infer execution phases using precomputed [`BuildContext`] (single-pass lowercasing and taxonomy lookup).
pub(super) fn infer_execution_phases_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Vec<ExecutionPhase> {
    let raw_lower = ctx.raw_lower.as_str();
    // FIX: G-11 — Use whole-word boundary matching for `"plan"` / `"roadmap"` so words like
    // `"plants"`, `"planet"`, `"explanation"`, or `"implant"` never hijack `Explain` prompts into
    // multi-phase execution roadmaps.
    let is_planning_prompt = raw_lower.contains("step-by-step")
        || has_whole_word_or_phrase(raw_lower, "plan")
        || has_whole_word_or_phrase(raw_lower, "roadmap")
        || raw_lower.contains("how do i ")
        || raw_lower.contains("become a ");
    if !profile.has_phases && !profile.has_timeline && !is_planning_prompt {
        // No temporal signals — check if this is a build/planning task
        if profile.primary_intent != crate::npae::aggressive::intent::IntentClass::Build
            && profile.primary_intent != crate::npae::aggressive::intent::IntentClass::Transform
        {
            return vec![];
        }
        // Split Artifact-Build vs. Roadmap-Build: direct code/config artifact requests should use
        // `# OUTPUT BLUEPRINT` implementation sections instead of multi-week waterfall timelines.
        if profile.primary_intent == crate::npae::aggressive::intent::IntentClass::Build
            && is_direct_artifact_build(raw_lower, &profile.domain)
        {
            return vec![];
        }
    }

    // 1. Generic dynamic lookup from config.domain_taxonomy.phase_templates
    let intent_key = match profile.primary_intent {
        crate::npae::aggressive::intent::IntentClass::Build => "build",
        crate::npae::aggressive::intent::IntentClass::Analyze => "analyze",
        crate::npae::aggressive::intent::IntentClass::Explain => "explain",
        crate::npae::aggressive::intent::IntentClass::Debug => "debug",
        crate::npae::aggressive::intent::IntentClass::Transform => "transform",
    };

    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref templates) = tax.phase_templates {
            let composite_key = format!("{}:{}", profile.perspective.as_key(), intent_key);
            if let Some(phases) = templates.get(&composite_key) {
                if !phases.is_empty() {
                    return phases.clone();
                }
            }
            if let Some(phases) = templates.get(profile.perspective.as_key()) {
                if !phases.is_empty() {
                    return phases.clone();
                }
            }
            if profile.perspective == crate::npae::aggressive::intent::ActorPerspective::Consumer
                && (is_planning_prompt || profile.has_phases)
            {
                if let Some(phases) = templates.get("consumer:build") {
                    if !phases.is_empty() {
                        return phases.clone();
                    }
                }
            }
            if profile.perspective
                == crate::npae::aggressive::intent::ActorPerspective::Practitioner
                && (is_planning_prompt || profile.has_phases)
            {
                if let Some(phases) = templates.get("practitioner:build") {
                    if !phases.is_empty() {
                        return phases.clone();
                    }
                }
            }
            if let Some(phases) = templates.get(intent_key) {
                if !phases.is_empty() {
                    return phases.clone();
                }
            }
            if matches!(
                profile.primary_intent,
                IntentClass::Build | IntentClass::Transform
            ) {
                if let Some(phases) = templates.get("default") {
                    if !phases.is_empty() {
                        return phases.clone();
                    }
                }
            }
        }
    }

    let lower = raw_lower;
    let mut phases = Vec::new();

    // Resolve domain alias → canonical name for consistent matching.
    let resolved_domain = ctx.canonical_domain.as_str();

    match resolved_domain {
        "finance" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Financial & Asset Audit (30 Days)".into(),
                description: "Evaluate current active/passive income sources, baseline burn rate, balance sheet assets/debt, and human capital monetization inventory.".into(),
                estimated_duration: "30 days".into(),
                deliverables: vec!["Financial Baseline & Burn Rate Audit".into(), "Human Capital & Monetization Matrix".into(), "Target Definition & Wealth Gap Calculation".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Strategy & Vehicle Selection (90 Days)".into(),
                description: "Select and launch primary high-leverage monetization vehicles (career advancement, high-ticket services, business equity, or dividend/index investing).".into(),
                estimated_duration: "90 days".into(),
                deliverables: vec!["Vehicle Feasibility Matrix".into(), "Income Growth Blueprint".into(), "First Revenue Milestone Report".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Execution & Reinvestment (6-12 Months)".into(),
                description: "Launch revenue-generating initiatives, secure initial cash flows, and configure automated compounding reinvestment.".into(),
                estimated_duration: "6-12 months".into(),
                deliverables: vec!["Execution Action Checklist".into(), "Cash Flow & Surplus Automation Blueprint".into(), "Diversified Asset Allocation Strategy".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 4,
                name: "Equity & Asset Expansion (3-5 Years)".into(),
                description: "Scale business equity, expand productive assets, and optimize capital compounding across market cycles.".into(),
                estimated_duration: "3-5 years".into(),
                deliverables: vec!["Equity & Portfolio Growth Model".into(), "Scenario Stress-Test Report (Conservative/Base/Aggressive)".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 5,
                name: "Wealth Compounding & Preservation (5-10 Years)".into(),
                description: "Maximize compound growth, achieve targeted net worth milestone, and implement estate, tax, and capital preservation structures.".into(),
                estimated_duration: "5-10 years".into(),
                deliverables: vec!["Target Achievement & Wealth Preservation Blueprint".into(), "Financial Freedom Audit".into()],
            });
        }
        "computers" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "System Architecture & Specs".into(),
                description: "Define hardware/software compute requirements, throughput constraints, and interface specifications.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["System Architecture Specification".into(), "Compute & Memory Budget".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Core System Implementation".into(),
                description: "Develop core modules, memory management, concurrency models, and driver/OS integrations.".into(),
                estimated_duration: "1-3 weeks".into(),
                deliverables: vec!["Working System Implementation".into(), "Benchmark Test Suite".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Optimization & Deployment".into(),
                description: "Profile bottlenecks, optimize cache/memory locality, and configure production telemetry.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Performance Audit Report".into(), "Production Configuration".into()],
            });
        }
        "science" | "scientific-research" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Hypothesis & Experimental Design".into(),
                description: "Formulate falsifiable hypotheses, define control variables, and design experimental protocols.".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Formal Hypothesis Document".into(), "Experimental Protocol Blueprint".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Data Acquisition & Execution".into(),
                description: "Execute trials, collect empirical sensor/laboratory data, and ensure methodological reproducibility.".into(),
                estimated_duration: "2-6 weeks".into(),
                deliverables: vec!["Raw Empirical Dataset".into(), "Execution Telemetry Log".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Statistical Analysis & Validation".into(),
                description: "Apply statistical significance testing, error analysis, and peer-review synthesis.".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Statistical Validation Report".into(), "Publication-Ready Synthesis".into()],
            });
        }
        "health" | "medical" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Health Assessment & Baseline Audit".into(),
                description: "Evaluate patient history, baseline biomarkers, physiological symptoms, and lifestyle factors.".into(),
                estimated_duration: "1-3 days".into(),
                deliverables: vec!["Comprehensive Assessment Profile".into(), "Biomarker Baseline Report".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Intervention & Protocol Design".into(),
                description: "Develop evidence-based therapeutic interventions, dosage protocols, and lifestyle adjustments.".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Therapeutic Protocol Specification".into(), "Risk & Safety Mitigation Plan".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Monitoring & Long-Term Adaptation".into(),
                description: "Execute health protocol, track biomarker responses, and optimize long-term outcomes.".into(),
                estimated_duration: "2-8 weeks".into(),
                deliverables: vec!["Progress Tracking Log".into(), "Outcome Analysis Report".into()],
            });
        }
        "business" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Research & Validation".into(),
                description: "Market research, competitor analysis, target audience identification"
                    .into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Market analysis report".into(), "Competitor matrix".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Strategy & Planning".into(),
                description: "Business model design, revenue strategy, operational planning".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Business model canvas".into(), "Revenue model".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Execution & Launch".into(),
                description: "Implementation, go-to-market, initial customer acquisition".into(),
                estimated_duration: "2-4 weeks".into(),
                deliverables: vec!["Launch plan".into(), "First 30-day action items".into()],
            });
        }
        "real-estate" | "real estate" => match profile.perspective {
            crate::npae::aggressive::intent::ActorPerspective::Consumer => {
                phases.push(ExecutionPhase {
                    phase_number: 1,
                    name: "Financial Readiness & Mortgage Pre-Approval".into(),
                    description: "Audit credit profile, debt-to-income (DTI) ratio, down-payment and closing-cost reserves, and secure mortgage pre-approval (evaluating fixed-rate vs. ARM structures).".into(),
                    estimated_duration: "1-2 weeks".into(),
                    deliverables: vec![
                        "Affordability & DTI Budget Ceiling".into(),
                        "Mortgage Pre-Approval Checklist".into(),
                    ],
                });
                phases.push(ExecutionPhase {
                    phase_number: 2,
                    name: "Property Search, Neighborhood Comps & Offer Strategy".into(),
                    description: "Evaluate neighborhood comparables (CMA), property condition, HOA/zoning disclosures, and structure a contingency-protected purchase offer.".into(),
                    estimated_duration: "2-6 weeks".into(),
                    deliverables: vec![
                        "Comparative Market Analysis (CMA)".into(),
                        "Purchase Offer & Contingency Strategy".into(),
                    ],
                });
                phases.push(ExecutionPhase {
                    phase_number: 3,
                    name: "Inspection, Appraisal, Underwriting & Closing".into(),
                    description: "Execute physical home inspection, negotiate repair credits, verify lender appraisal and title/escrow clearance, and complete closing settlement.".into(),
                    estimated_duration: "30-45 days".into(),
                    deliverables: vec![
                        "Inspection & Due Diligence Checklist".into(),
                        "Closing Disclosure & Settlement Plan".into(),
                    ],
                });
            }
            crate::npae::aggressive::intent::ActorPerspective::Investor => {
                phases.push(ExecutionPhase {
                    phase_number: 1,
                    name: "Market Comparables & Location Audit".into(),
                    description: "Evaluate local market comparables (CMA), submarket supply/demand dynamics, zoning disclosures, and property condition.".into(),
                    estimated_duration: "2-3 days".into(),
                    deliverables: vec![
                        "Comparative Market Analysis (CMA)".into(),
                        "Zoning & Due Diligence Audit".into(),
                    ],
                });
                phases.push(ExecutionPhase {
                    phase_number: 2,
                    name: "Financial Underwriting & ROI Stress-Test".into(),
                    description: "Model Net Operating Income (NOI), capitalization rate, cash-on-cash return, and debt-service coverage across downside scenarios.".into(),
                    estimated_duration: "2-3 days".into(),
                    deliverables: vec![
                        "NOI, Cap Rate & ROI Projection".into(),
                        "Financing & Capital Stack Plan".into(),
                    ],
                });
            }
            crate::npae::aggressive::intent::ActorPerspective::Practitioner => {
                phases.push(ExecutionPhase {
                    phase_number: 1,
                    name: "Licensing & Pre-Registration".into(),
                    description: "Complete pre-licensing education coursework, pass state/regional real estate exam, and secure broker sponsorship.".into(),
                    estimated_duration: "1-3 months".into(),
                    deliverables: vec![
                        "Real Estate License".into(),
                        "Broker Sponsorship Agreement".into(),
                    ],
                });
                phases.push(ExecutionPhase {
                    phase_number: 2,
                    name: "Business Setup & GTM Strategy".into(),
                    description: "Form business entity (LLC), set up MLS & CRM systems, build personal brand, and establish marketing funnel.".into(),
                    estimated_duration: "2-4 weeks".into(),
                    deliverables: vec![
                        "Business Entity (LLC)".into(),
                        "Lead Generation & CRM Setup".into(),
                    ],
                });
                phases.push(ExecutionPhase {
                    phase_number: 3,
                    name: "Client Acquisition & Expansion".into(),
                    description: "Execute prospecting campaigns, handle buyer/seller representations, secure initial listings, and scale brokerage operations.".into(),
                    estimated_duration: "Ongoing".into(),
                    deliverables: vec![
                        "First Closed Transactions".into(),
                        "Client Referral Pipeline".into(),
                    ],
                });
            }
        },
        "career-growth" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Gap Analysis & Baseline Audit".into(),
                description: "Compare current skills, certifications, and experience against target role requirements and industry benchmarks.".into(),
                estimated_duration: "1-2 weeks".into(),
                deliverables: vec!["Skills Gap Matrix".into(), "Target Role Requirement Specification".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Skill Acquisition & Certification".into(),
                description: "Execute targeted learning paths, acquire necessary industry certifications, and build project-based evidence of competency.".into(),
                estimated_duration: "3-6 months".into(),
                deliverables: vec!["Certification/Credential Proof".into(), "Technical Portfolio/Case Studies".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Positioning & Market Alignment".into(),
                description: "Optimize professional branding, build strategic industry network, and align resume/LinkedIn with target role keywords.".into(),
                estimated_duration: "2-4 weeks".into(),
                deliverables: vec!["Optimized Professional Profile".into(), "Industry Networking Map".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 4,
                name: "Execution & Transition".into(),
                description: "Implement high-conversion application strategy, execute interview frameworks, and secure target role.".into(),
                estimated_duration: "1-3 months".into(),
                deliverables: vec!["Interview Performance Log".into(), "Secured Offer/Role Transition".into()],
            });
        }
        "software" | "devops" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Design & Architecture".into(),
                description: "System design, technology selection, architecture decisions".into(),
                estimated_duration: "3-5 days".into(),
                deliverables: vec!["Architecture document".into(), "Tech stack decision".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Implementation".into(),
                description: "Core development, testing, iteration".into(),
                estimated_duration: "1-3 weeks".into(),
                deliverables: vec!["Working codebase".into(), "Test suite".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Deployment & Validation".into(),
                description: "Staging deployment, performance testing, production release".into(),
                estimated_duration: "2-3 days".into(),
                deliverables: vec!["Deployed service".into(), "Monitoring setup".into()],
            });
        }
        "nutrition" | "health-fitness" => {
            if lower.contains("week") || lower.contains("day") || lower.contains("plan") {
                phases.push(ExecutionPhase {
                    phase_number: 1,
                    name: "Assessment".into(),
                    description: "Current state analysis, goal setting, constraint identification"
                        .into(),
                    estimated_duration: "Day 1".into(),
                    deliverables: vec!["Baseline assessment".into(), "Goal targets".into()],
                });
                phases.push(ExecutionPhase {
                    phase_number: 2,
                    name: "Plan Design".into(),
                    description: "Detailed plan creation with daily/weekly breakdowns".into(),
                    estimated_duration: "Day 1-2".into(),
                    deliverables: vec!["Detailed plan".into(), "Macro targets".into()],
                });
                phases.push(ExecutionPhase {
                    phase_number: 3,
                    name: "Execution & Adjustment".into(),
                    description: "Follow plan, monitor progress, adjust based on feedback".into(),
                    estimated_duration: "Ongoing".into(),
                    deliverables: vec!["Progress tracking".into(), "Adjustment notes".into()],
                });
            }
        }
        "workplace-productivity" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Data Collection & Audit".into(),
                description:
                    "Gather quantitative (output, hours) and qualitative (surveys, sentiment) data"
                        .into(),
                estimated_duration: "1 week".into(),
                deliverables: vec![
                    "Data collection framework".into(),
                    "Initial sentiment report".into(),
                ],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Comparative Analysis".into(),
                description:
                    "Compare remote vs in-office benchmarks, analyze 'perception vs reality' gap"
                        .into(),
                estimated_duration: "1 week".into(),
                deliverables: vec![
                    "Productivity gap analysis".into(),
                    "Variable correlation matrix".into(),
                ],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Strategic Recommendation".into(),
                description:
                    "Propose hybrid/remote optimizations, cultural adjustments, and KPI realignment"
                        .into(),
                estimated_duration: "1 week".into(),
                deliverables: vec![
                    "Strategic optimization roadmap".into(),
                    "Policy adjustment guide".into(),
                ],
            });
        }
        "education" => {
            phases.push(ExecutionPhase {
                phase_number: 1,
                name: "Knowledge Mapping".into(),
                description: "Identify core concepts, current knowledge gaps, and learning objectives".into(),
                estimated_duration: "Immediate".into(),
                deliverables: vec!["Concept map".into(), "Gap analysis".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 2,
                name: "Skill Acquisition".into(),
                description: "Implement high-leverage learning techniques (e.g., active recall, interleaving)".into(),
                estimated_duration: "Daily".into(),
                deliverables: vec!["Study protocol".into(), "Practice modules".into()],
            });
            phases.push(ExecutionPhase {
                phase_number: 3,
                name: "Application & Mastery".into(),
                description: "Verify understanding through teaching, problem-solving, and real-world application".into(),
                estimated_duration: "Ongoing".into(),
                deliverables: vec!["Mastery check".into(), "Feedback loop".into()],
            });
        }
        _ => {
            // INTENT-DRIVEN TEMPLATES for unknown domains (All-Type Prompt Support)
            let subject = profile
                .dynamic_subject
                .clone()
                .unwrap_or_else(|| "Project".into());

            match profile.primary_intent {
                crate::npae::aggressive::intent::IntentClass::Analyze => {
                    phases.push(ExecutionPhase {
                        phase_number: 1,
                        name: format!("{} Discovery", subject),
                        description: format!(
                            "Identify key variables, stakeholders, and data sources for {}.",
                            subject
                        )
                        .into(),
                        estimated_duration: "Phase 1".into(),
                        deliverables: vec![format!(
                            "{} assessment report",
                            naturalize_subject(&subject, &profile.primary_intent)
                        )
                        .into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 2,
                        name: format!("{} Deep Dive", subject),
                        description: format!(
                            "Perform detailed analytical investigation into {} dynamics.",
                            subject
                        )
                        .into(),
                        estimated_duration: "Phase 2".into(),
                        deliverables: vec![format!(
                            "{} analytical model",
                            naturalize_subject(&subject, &profile.primary_intent)
                        )
                        .into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 3,
                        name: "Insights & Strategy".into(),
                        description: format!(
                            "Synthesize findings into actionable recommendations for {}.",
                            subject
                        )
                        .into(),
                        estimated_duration: "Phase 3".into(),
                        deliverables: vec!["Strategic recommendation brief".into()],
                    });
                }
                crate::npae::aggressive::intent::IntentClass::Build => {
                    let is_software_like =
                        matches!(resolved_domain, "software" | "devops" | "ai-ml");

                    if is_software_like {
                        phases.push(ExecutionPhase {
                            phase_number: 1,
                            name: "Architecture & Design".into(),
                            description: format!(
                                "Define structural requirements and design for {}.",
                                subject
                            )
                            .into(),
                            estimated_duration: "Design Phase".into(),
                            deliverables: vec![format!(
                                "{} blueprint",
                                naturalize_subject(&subject, &profile.primary_intent)
                            )
                            .into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 2,
                            name: "Core Implementation".into(),
                            description: format!(
                                "Construct the functional components of {}.",
                                subject
                            )
                            .into(),
                            estimated_duration: "Build Phase".into(),
                            deliverables: vec![format!(
                                "Functional {} prototype",
                                naturalize_subject(&subject, &profile.primary_intent)
                            )
                            .into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 3,
                            name: "Validation & Polish".into(),
                            description: format!(
                                "Test, refine, and finalize {} for production.",
                                subject
                            )
                            .into(),
                            estimated_duration: "Final Phase".into(),
                            deliverables: vec![format!(
                                "Optimized {}",
                                naturalize_subject(&subject, &profile.primary_intent)
                            )
                            .into()],
                        });
                    } else {
                        phases.push(ExecutionPhase {
                            phase_number: 1,
                            name: "Planning & Strategy".into(),
                            description: format!("Define goals, foundational requirements, and operational roadmap for {}.", subject).into(),
                            estimated_duration: "Phase 1".into(),
                            deliverables: vec![format!("{} strategy roadmap", naturalize_subject(&subject, &profile.primary_intent)).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 2,
                            name: "Execution & Setup".into(),
                            description: format!("Establish core operational assets, compliance, and systems for {}.", subject).into(),
                            estimated_duration: "Phase 2".into(),
                            deliverables: vec![format!("Functional {} setup", naturalize_subject(&subject, &profile.primary_intent)).into()],
                        });
                        phases.push(ExecutionPhase {
                            phase_number: 3,
                            name: "Launch & Optimization".into(),
                            description: format!("Deploy, monitor initial outcomes, and optimize performance for {}.", subject).into(),
                            estimated_duration: "Phase 3".into(),
                            deliverables: vec![format!("Launched {}", naturalize_subject(&subject, &profile.primary_intent)).into()],
                        });
                    }
                }
                _ => {
                    // Minimal fallback for other intents
                    phases.push(ExecutionPhase {
                        phase_number: 1,
                        name: "Preparation".into(),
                        description: format!("Set up environment and context for {}.", subject)
                            .into(),
                        estimated_duration: "Start".into(),
                        deliverables: vec!["Initial setup".into()],
                    });
                    phases.push(ExecutionPhase {
                        phase_number: 2,
                        name: "Execution".into(),
                        description: format!("Execute core {} tasks.", subject).into(),
                        estimated_duration: "Execution".into(),
                        deliverables: vec![format!(
                            "{} core output",
                            naturalize_subject(&subject, &profile.primary_intent)
                        )
                        .into()],
                    });
                }
            }
        }
    }

    phases
}

/// Infer validation steps if validation signals detected
///
/// // ARCH: ARCHITECTURE.md §1.4 & §5 GAP-35 — provides domain-specific validation checkpoints
/// // across canonical and legacy domain names.
pub(super) fn infer_validation_steps_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Vec<ValidationStep> {
    if !profile.has_validation {
        return vec![];
    }
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref templates) = tax.validation_templates {
            if let Some(steps) =
                resolve_template_key(templates, profile.perspective, &profile.primary_intent)
            {
                if !steps.is_empty() {
                    return steps.clone();
                }
            }
        }
    }
    infer_validation_steps_fallback(profile)
}

fn infer_validation_steps_fallback(profile: &IntentProfile) -> Vec<ValidationStep> {
    if !profile.has_validation {
        return vec![];
    }

    match profile.domain.as_str() {
        "software" | "software-engineering" | "devops" | "devops-infra" | "computers" => vec![
            ValidationStep {
                step_number: 1,
                name: "Unit Tests".into(),
                criteria: "All unit tests pass".into(),
                validation_method: "Automated test suite".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Integration Test".into(),
                criteria: "End-to-end flow verified".into(),
                validation_method: "Integration test suite".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Performance Check".into(),
                criteria: "Meets latency/throughput targets".into(),
                validation_method: "Load testing".into(),
            },
        ],
        "business" | "business-strategy" => vec![
            ValidationStep {
                step_number: 1,
                name: "Market Fit".into(),
                criteria: "Target market size validated".into(),
                validation_method: "TAM/SAM/SOM analysis".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Financial Viability".into(),
                criteria: "Unit economics positive".into(),
                validation_method: "Financial model review".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Competitive Position".into(),
                criteria: "Clear differentiation identified".into(),
                validation_method: "Competitor analysis".into(),
            },
        ],
        "finance" => vec![
            ValidationStep {
                step_number: 1,
                name: "Mathematical & CAGR Audit".into(),
                criteria: "Compound returns, savings rate, and cashflows reconcile mathematically"
                    .into(),
                validation_method: "Quantitative spreadsheet verification".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Downside & Drawdown Stress Test".into(),
                criteria: "Portfolio survives conservative market drawdown and liquidity shocks"
                    .into(),
                validation_method: "Scenario sensitivity modeling".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Tax & Regulatory Alignment".into(),
                criteria: "Asset location and withdrawal assumptions account for tax drag".into(),
                validation_method: "Tax efficiency review".into(),
            },
        ],
        "legal" => vec![
            ValidationStep {
                step_number: 1,
                name: "Jurisdictional & Statutory Check".into(),
                criteria: "All cited clauses and statutes match the governing jurisdiction".into(),
                validation_method: "Statutory cross-reference audit".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Liability & Indemnification Review".into(),
                criteria: "Risk allocation, caps, and remedies are unambiguous and enforceable"
                    .into(),
                validation_method: "Contractual risk matrix review".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Regulatory Compliance Gate".into(),
                criteria:
                    "Satisfies applicable privacy, IP, or industry regulations (e.g. GDPR/HIPAA)"
                        .into(),
                validation_method: "Compliance checklist verification".into(),
            },
        ],
        "medical" | "health" | "pharma" => vec![
            ValidationStep {
                step_number: 1,
                name: "Clinical Evidence Verification".into(),
                criteria: "Recommendations align with peer-reviewed clinical guidelines".into(),
                validation_method: "Clinical literature cross-check".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Contraindication & Safety Screening".into(),
                criteria: "Dosages, interactions, and patient risk factors are screened".into(),
                validation_method: "Safety & contraindication audit".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Regulatory & Ethical Compliance".into(),
                criteria: "Includes clinical disclaimer and adheres to FDA/EMA/HIPAA standards"
                    .into(),
                validation_method: "Regulatory review gate".into(),
            },
        ],
        "nutrition" | "sports-nutrition" => vec![
            ValidationStep {
                step_number: 1,
                name: "Caloric & Macronutrient Balance".into(),
                criteria:
                    "Daily protein, carbohydrate, fat, and caloric totals match target expenditure"
                        .into(),
                validation_method: "Macro & energy balance audit".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Allergen & Restriction Compliance".into(),
                criteria: "Zero forbidden ingredients or allergens appear in meal selections"
                    .into(),
                validation_method: "Strict exclusion checklist".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Micronutrient & Hydration Sufficiency".into(),
                criteria: "Electrolyte, fiber, and vitamin requirements are met sustainably".into(),
                validation_method: "Nutritional completeness review".into(),
            },
        ],
        "data-science" | "ai-ml" => vec![
            ValidationStep {
                step_number: 1,
                name: "Data Leakage & Split Integrity".into(),
                criteria:
                    "Train/validation/test splits are strictly isolated with zero target leakage"
                        .into(),
                validation_method: "Pipeline data audit".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Metric & Baseline Evaluation".into(),
                criteria:
                    "Model beats baseline on target evaluation metrics (e.g. F1, ROC-AUC, RMSE)"
                        .into(),
                validation_method: "Holdout benchmark evaluation".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Calibration, Drift & Safety Check".into(),
                criteria:
                    "Predictions are calibrated and robust against distribution shift or bias"
                        .into(),
                validation_method: "Robustness & fairness suite".into(),
            },
        ],
        "cybersecurity" => vec![
            ValidationStep {
                step_number: 1,
                name: "Threat Model & Attack Surface Coverage".into(),
                criteria: "All trust boundaries and OWASP/MITRE threat vectors are enumerated"
                    .into(),
                validation_method: "STRIDE / MITRE ATT&CK review".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Vulnerability & Control Verification".into(),
                criteria: "Authentication, encryption, and network controls block exploit paths"
                    .into(),
                validation_method: "Penetration test & scanner verification".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Incident Detection & Audit Logging".into(),
                criteria: "SIEM alerts and immutable audit logs capture anomalous activity".into(),
                validation_method: "SOC detection rule test".into(),
            },
        ],
        "education" => vec![
            ValidationStep {
                step_number: 1,
                name: "Learning Objective Alignment".into(),
                criteria: "Every module maps directly to measurable Bloom's taxonomy outcomes"
                    .into(),
                validation_method: "Curriculum alignment rubric".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Cognitive Load & Pacing Check".into(),
                criteria: "Concepts progress from foundational to advanced with active recall"
                    .into(),
                validation_method: "Pedagogical pacing review".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Assessment & Mastery Verification".into(),
                criteria: "Formative and summative assessments accurately test concept retention"
                    .into(),
                validation_method: "Assessment rubric validation".into(),
            },
        ],
        "travel" => vec![
            ValidationStep {
                step_number: 1,
                name: "Route & Transit Feasibility".into(),
                criteria: "Daily transit times and geographic sequencing are realistic".into(),
                validation_method: "Route & schedule audit".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Budget & Constraint Reconciliation".into(),
                criteria:
                    "Lodging, flights, activities, and dining stay within the stated budget ceiling"
                        .into(),
                validation_method: "Itemized cost tally".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Dietary & Group Accommodation Check".into(),
                criteria:
                    "All dietary restrictions, accessibility, and traveler group needs are met"
                        .into(),
                validation_method: "Traveler constraint checklist".into(),
            },
        ],
        "real-estate" => match profile.perspective {
            crate::npae::aggressive::intent::ActorPerspective::Consumer => vec![
                ValidationStep {
                    step_number: 1,
                    name: "Affordability & DTI Stress Test".into(),
                    criteria:
                        "Monthly PITI + HOA payment stays within DTI ceiling and budget reserves"
                            .into(),
                    validation_method: "Mortgage affordability & DTI audit".into(),
                },
                ValidationStep {
                    step_number: 2,
                    name: "Neighborhood Comps & Appraisal Check".into(),
                    criteria:
                        "Offer price aligns with recent comparable sales (CMA) and appraisal value"
                            .into(),
                    validation_method: "CMA & lender appraisal verification".into(),
                },
                ValidationStep {
                    step_number: 3,
                    name: "Inspection, Title & Closing Verification".into(),
                    criteria:
                        "Physical inspection contingencies, title clearance, and closing disclosures are verified"
                            .into(),
                    validation_method: "Inspection & settlement checklist".into(),
                },
            ],
            _ => vec![
                ValidationStep {
                    step_number: 1,
                    name: "Comparable Market & Valuation Check".into(),
                    criteria: "Pricing, cap rate, and cash-on-cash return align with local comps"
                        .into(),
                    validation_method: "CMA & underwriting audit".into(),
                },
                ValidationStep {
                    step_number: 2,
                    name: "Zoning, Title & Licensing Compliance".into(),
                    criteria:
                        "Property use, disclosures, and brokerage steps satisfy local regulations"
                            .into(),
                    validation_method: "Regulatory & title checklist".into(),
                },
                ValidationStep {
                    step_number: 3,
                    name: "Financing & Cashflow Stress Test".into(),
                    criteria:
                        "Debt service coverage survives vacancy and interest rate sensitivity"
                            .into(),
                    validation_method: "Underwriting sensitivity model".into(),
                },
            ],
        },
        "marketing" | "ecommerce" => vec![
            ValidationStep {
                step_number: 1,
                name: "Audience & Positioning Fit".into(),
                criteria: "Value proposition and messaging match target segment pain points".into(),
                validation_method: "Segment & copy review".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Funnel & Conversion Economics".into(),
                criteria: "CAC, ROAS, and checkout conversion targets are Positive-ROI".into(),
                validation_method: "Funnel unit economics audit".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Attribution & A/B Test Readiness".into(),
                criteria:
                    "Tracking pixels, KPIs, and experiment hypotheses are instrumentation-ready"
                        .into(),
                validation_method: "Analytics & experiment checklist".into(),
            },
        ],
        "science" | "scientific-research" => vec![
            ValidationStep {
                step_number: 1,
                name: "Hypothesis Falsifiability & Controls".into(),
                criteria:
                    "Null hypothesis, independent variables, and negative controls are defined"
                        .into(),
                validation_method: "Experimental design review".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Statistical Power & Significance".into(),
                criteria: "Sample size, confidence intervals, and p-value thresholds are justified"
                    .into(),
                validation_method: "Biostatistics / power analysis".into(),
            },
            ValidationStep {
                step_number: 3,
                name: "Reproducibility & Peer-Review Rigor".into(),
                criteria: "Methodology and apparatus parameters allow independent replication"
                    .into(),
                validation_method: "Protocol reproducibility checklist".into(),
            },
        ],
        _ => vec![
            ValidationStep {
                step_number: 1,
                name: "Completeness".into(),
                criteria: "All deliverables produced".into(),
                validation_method: "Checklist review".into(),
            },
            ValidationStep {
                step_number: 2,
                name: "Quality".into(),
                criteria: "Meets stated requirements".into(),
                validation_method: "Peer review".into(),
            },
        ],
    }
}

/// Infer success criteria from profile and raw prompt — intent and domain driven.
#[cfg(test)]
pub(super) fn infer_success_criteria(profile: &IntentProfile, raw: &str) -> Vec<String> {
    let ctx = BuildContext::new(profile, raw, None);
    infer_success_criteria_with_ctx(profile, &ctx)
}

/// Infer success criteria using precomputed [`BuildContext`].
pub(super) fn infer_success_criteria_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Vec<String> {
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref templates) = tax.success_criteria_templates {
            if let Some(items) =
                resolve_template_key(templates, profile.perspective, &profile.primary_intent)
            {
                if !items.is_empty() {
                    let mut criteria = items.clone();
                    if profile.has_phases {
                        criteria.push("All execution phases have clear deliverables".into());
                    }
                    if profile.has_validation {
                        criteria.push("All validation checkpoints pass".into());
                    }
                    return criteria;
                }
            }
        }
    }
    let mut criteria = Vec::new();
    let is_financial = ctx.is_wealth_query;
    let is_business = ctx.is_enterprise_business;

    if is_financial && !profile.domain.starts_with("workplace") {
        criteria.push("Target is clearly defined with explicit amount ($), currency, time horizon, and target type (Distinguish: Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth)".into());
        criteria.push("Baseline financial and human capital inventory is fully quantified with transparent assumptions".into());
        criteria.push("Wealth gap, required annual savings, and required CAGR are calculated with mathematical rigor".into());
        criteria.push("Income engines and leverage vectors (skills, technology, capital, equity) are evaluated and ranked by ROI and scalability".into());
        criteria.push("Capital allocation framework balances liquidity reserves, asset compounding, and business reinvestment".into());
        criteria.push("Conservative, Base Case, and Aggressive scenario models are articulated with distinct assumptions".into());
        criteria.push("Downside risks (income, market, debt, tax, concentration) have concrete mitigation strategies".into());
        criteria.push("Execution timeline outlines 30-day, 90-day, 12-month, 3-year, and 5-10-year milestones with measurable KPIs".into());
        criteria.push("Includes explicit Reality-Check verdict assessing mathematical feasibility against stated constraints".into());
        return criteria;
    }

    if is_business {
        criteria.push("Business model defines clear revenue streams, pricing tiers, and positive unit economics (LTV/CAC, gross margin)".into());
        criteria.push("Scalability vectors (distribution channels, technology leverage, team delegation) are explicitly mapped".into());
        criteria.push("Valuation roadmap differentiates annual revenue/EBITDA from enterprise valuation multiples".into());
        criteria.push(
            "Capital requirements and cash flow runway are modeled across growth phases".into(),
        );
        criteria
            .push("Includes measurable revenue, customer acquisition, and retention KPIs".into());
        return criteria;
    }

    // Domain-Aware Success Criteria for other domains
    match (profile.primary_intent.clone(), profile.domain.as_str()) {
        (IntentClass::Build, "career-growth") => {
            criteria.push("Comprehensive skills gap analysis completed and validated against industry benchmarks".into());
            criteria.push("Target certifications or credentials acquired and verified".into());
            criteria.push(
                "Professional portfolio demonstrates competency in target role core requirements"
                    .into(),
            );
            criteria.push("Market positioning aligned with target role expectations".into());
        }
        (
            IntentClass::Build,
            "software" | "software-engineering" | "devops" | "devops-infra" | "computers",
        ) => {
            criteria.push("Functional, bug-free implementation with optimized performance".into());
            criteria.push(
                "Adheres to industry-standard architectural patterns and best practices".into(),
            );
            criteria.push("Includes necessary technical documentation and test coverage".into());
        }
        (IntentClass::Build, "ai-ml" | "data-science") => {
            criteria.push("Model performance meets or exceeds stated benchmarks".into());
            criteria.push(
                "Alignment, safety, and ethical considerations are explicitly addressed".into(),
            );
            criteria.push("Data handling and inference pipelines are robust and scalable".into());
        }
        (IntentClass::Build, "health" | "medical" | "pharma") => {
            criteria
                .push("Clinically accurate protocols grounded in peer-reviewed evidence".into());
            criteria.push("Strict adherence to regulatory (FDA/EMA) and ethical guidelines".into());
            criteria.push("Risk-benefit analysis is comprehensive and clearly stated".into());
        }
        (IntentClass::Build, "nutrition" | "sports-nutrition" | "health-fitness") => {
            criteria.push("Plan is physiologically sound and tailored to specific goals".into());
            criteria
                .push("Macros and micronutrients are balanced according to activity level".into());
            criteria.push("Includes clear instructions for tracking and adjustment".into());
        }
        (IntentClass::Build, "legal") => {
            criteria.push("Legal clauses and analysis strictly conform to governing jurisdiction and statutory precedent".into());
            criteria.push("Liability exposure, indemnification, and compliance obligations are unambiguously specified".into());
            criteria.push(
                "Includes actionable risk-mitigation language and regulatory verification".into(),
            );
        }
        (IntentClass::Build, "cybersecurity") => {
            criteria.push("Threat model enumerates all trust boundaries and attack vectors (OWASP/MITRE ATT&CK)".into());
            criteria.push(
                "Defensive controls, encryption, and authentication mechanisms are verifiable"
                    .into(),
            );
            criteria.push("Includes incident detection telemetry and response playbooks".into());
        }
        (IntentClass::Build, "real-estate") => match profile.perspective {
            crate::npae::aggressive::intent::ActorPerspective::Consumer => {
                criteria.push("Homebuying budget, DTI ratio, down payment, and closing-cost reserves are verified against mortgage pre-approval limits".into());
                criteria.push("Property evaluation and offer pricing are grounded in neighborhood Comparative Market Analysis (CMA) and appraisal comps".into());
                criteria.push("Includes actionable inspection contingencies, title/escrow due diligence, and closing settlement milestones".into());
            }
            _ => {
                criteria.push("Property valuation and cashflow underwriting are grounded in comparable market data".into());
                criteria.push(
                    "Licensing, zoning, and transaction steps adhere to local real estate regulations"
                        .into(),
                );
                criteria.push(
                    "Includes actionable client acquisition, financing, and closing milestones"
                        .into(),
                );
            }
        },
        (IntentClass::Build, "travel") => {
            criteria.push(
                "Itinerary schedule is geographically logical and optimizes transit times".into(),
            );
            criteria.push("All activities, accommodations, and dining options adhere to specified budget limits".into());
            criteria.push(
                "Dietary preferences and family group constraints are fully satisfied".into(),
            );
            criteria.push(
                "Includes actionable booking recommendations and cost-optimization alternatives"
                    .into(),
            );
        }
        (_, "workplace-productivity") => {
            criteria
                .push("Differentiates between objective output and subjective sentiment".into());
            criteria.push("Identifies hidden bottlenecks in distributed collaboration".into());
            criteria.push(
                "Provides actionable recommendations for burnout prevention and engagement".into(),
            );
            criteria.push("Aligns productivity metrics with organizational culture goals".into());
        }
        (_, "education") => {
            criteria.push("Learning objectives are clearly defined and measurable".into());
            criteria.push("Techniques used are evidence-based and optimized for retention".into());
            criteria.push(
                "Roadmap addresses both theoretical understanding and practical application".into(),
            );
            criteria.push("Progress can be tracked through objective mastery checks".into());
        }
        (crate::npae::aggressive::intent::IntentClass::Build, _) => {
            let subject = profile
                .dynamic_subject
                .clone()
                .unwrap_or_else(|| "deliverable".into());
            criteria.push(
                format!(
                    "Functional, high-quality {} is produced with modular design.",
                    naturalize_subject(&subject, &profile.primary_intent)
                )
                .into(),
            );
            criteria.push("Strict adherence to specified requirements and best practices.".into());
        }

        // EXPLAIN Intent (Domain-Agnostic but pedagogical)
        (crate::npae::aggressive::intent::IntentClass::Explain, _) => {
            let subject = if profile.domain == "real-estate"
                && matches!(
                    profile.perspective,
                    crate::npae::aggressive::intent::ActorPerspective::Consumer
                ) {
                "residential home-buying and mortgage process".to_string()
            } else {
                profile
                    .dynamic_subject
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| {
                        if profile.domain != "general" && !profile.domain.is_empty() {
                            profile.domain.replace('-', " ")
                        } else {
                            "concept".to_string()
                        }
                    })
            };
            criteria.push(
                format!(
                    "Conceptually clear explanation of {} at the target audience level.",
                    subject
                )
                .into(),
            );
            criteria.push("Balances technical depth with intuitive clarity.".into());
            criteria
                .push("Covers all major dimensions (capabilities, risks, and implications)".into());
        }

        // DEBUG Intent
        (crate::npae::aggressive::intent::IntentClass::Debug, _) => {
            criteria.push("Root cause of the issue is correctly identified and explained".into());
            criteria.push("Permanent fix is implemented and verified for correctness".into());
            criteria.push("No regressions or side-effects are introduced".into());
        }

        // ANALYZE Intent
        (crate::npae::aggressive::intent::IntentClass::Analyze, _) => {
            let subject = profile
                .dynamic_subject
                .clone()
                .unwrap_or_else(|| "data".into());
            criteria.push(
                format!(
                    "Deep insights and actionable patterns are extracted for {}.",
                    subject
                )
                .into(),
            );
            criteria.push("Reasoning is clear, logical, and supported by evidence.".into());
            criteria.push("Conclusions are prioritized by impact and feasibility".into());
        }

        // TRANSFORM Intent
        (crate::npae::aggressive::intent::IntentClass::Transform, _) => {
            criteria.push("Successful transformation with 100% data/logic integrity".into());
            criteria.push("Output format strictly adheres to the requested specification".into());
            criteria.push("Redundancy is eliminated while preserving essential context".into());
        }
    }

    if profile.has_phases {
        criteria.push("All execution phases have clear deliverables".into());
    }
    if profile.has_validation {
        criteria.push("All validation checkpoints pass".into());
    }

    criteria
}

/// Vagueness Resolution: Unpacks generic requests into structured roadmaps using precomputed [`BuildContext`].
pub(super) fn unpack_context_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Option<String> {
    let subject = profile
        .dynamic_subject
        .clone()
        .unwrap_or_else(|| "the core topic".into());
    let naturalized = naturalize_subject(&subject, &profile.primary_intent);
    let is_financial = ctx.is_wealth_query;
    let is_business = ctx.is_enterprise_business;

    if is_financial && !profile.domain.starts_with("workplace") {
        return Some(format!(
            "Target Definition (Disambiguate: Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth; quantify target amount, currency, and horizon). Current-State Baseline (Audit income, burn rate, assets, liabilities, and human capital monetization). Wealth Gap & Quantitative Modeling (Calculate Net Worth Gap, Required Annual Savings, Required Gross/Net Income, and Required CAGR). Income Engine & Leverage (Rank monetization vehicles: career, high-ticket services, equity, tech leverage by ROI). Capital Allocation & Risk Framework (Liquidity buffers, productive asset compounding, and downside mitigation across Conservative, Base, and Aggressive scenarios). Reality-Check (Quantify feasibility against constraints and identify required variable adjustments for {}).",
            naturalized
        ));
    }
    if is_business {
        return Some(format!(
            "Define target enterprise valuation and revenue milestones for {}. Validate unit economics (LTV/CAC, gross margin), business model canvas, go-to-market scalability, and capitalization roadmap across multi-year horizon.",
            naturalized
        ));
    }
    match profile.domain.as_str() {
        "computers" => Some(format!("Cover computing architecture for {}, low-level hardware-software interaction, memory constraints, and runtime efficiency.", naturalized)),
        "science" | "scientific-research" => Some(format!("Cover empirical foundations of {}, theoretical framework, experimental methodology, and potential impact.", naturalized)),
        "health" | "medical" => Some(format!("Cover physiological mechanisms of {}, clinical presentation, evidence-based interventions, and long-term outcomes.", naturalized)),
        "finance" => Some(format!("Cover capital dynamics of {}, market mechanisms, risk-adjusted returns, and actionable execution strategies.", naturalized)),
        "ai-ml" => Some(format!("Cover definition of {}, how it differs from current AI, how it might work, key challenges, risks, and real-world implications.", naturalized)),
        "workplace-productivity" => Some(format!("Analyze objective output vs subjective perception for {}, remote/hybrid dynamics, and cultural impact.", naturalized)),
        "education" => Some(format!("Cover pedagogical foundations of {}, cognitive load optimization, retention strategies, and application milestones.", naturalized)),
        "software" | "software-engineering" | "devops" | "devops-infra" => Some(format!("Cover system architecture for {}, deployment strategy, scalability bottlenecks, and security considerations.", naturalized)),
        "real-estate" => match profile.perspective {
            crate::npae::aggressive::intent::ActorPerspective::Consumer => Some(
                "End-to-end residential acquisition workflow covering financial underwriting, property due diligence, contingency protection, and settlement closing.".to_string(),
            ),
            _ => Some(format!(
                "Cover market valuation for {}, licensing/regulatory compliance, deal underwriting, and transaction execution milestones.",
                naturalized
            )),
        },
        _ => Some(format!("Break down {} into fundamental components, current state, key challenges, and future implications.", naturalized)),
    }
}

/// Infer missing assumptions to anchor the LLM using precomputed [`BuildContext`].
pub(super) fn infer_assumptions_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> Vec<String> {
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref templates) = tax.assumption_templates {
            if let Some(items) =
                resolve_template_key(templates, profile.perspective, &profile.primary_intent)
            {
                if !items.is_empty() {
                    return items.clone();
                }
            }
        }
    }
    let mut assumptions = Vec::new();
    let lower = ctx.raw_lower.as_str();
    let is_financial = ctx.is_wealth_query;
    let is_business = ctx.is_enterprise_business;

    if is_financial && !profile.domain.starts_with("workplace") {
        assumptions.push(
            "Assume Income ≠ Revenue ≠ Profit ≠ Savings ≠ Investable Capital ≠ Net Worth".into(),
        );
        assumptions.push("Assume unstated financial baselines (income, expenses, assets, liabilities) must be explicitly framed with transparent assumptions rather than arbitrary invention".into());
        assumptions.push("Assume mathematical consistency: calculate required CAGR and savings rate; flag any feasibility mismatch rather than fabricating unrealistic returns".into());
        assumptions.push("Assume primary income engine and skill/business leverage must precede passive portfolio compounding for zero-to-wealth trajectories".into());
        assumptions.push("Assume tax efficiency (capital gains vs ordinary income) and inflation-adjusted real returns must be accounted for across all scenario projections".into());
    } else if is_business {
        assumptions.push("Assume business enterprise valuation is separate from personal liquid net worth and depends on revenue multiples and profit margins".into());
        assumptions.push("Assume focus on sustainable unit economics (LTV/CAC > 3), gross margin health, and capital efficiency".into());
        assumptions.push(
            "Assume scalable operational infrastructure, defensible moat, and equity retention"
                .into(),
        );
    } else {
        // Domain assumptions
        match profile.domain.as_str() {
            "computers" => {
                assumptions.push(
                    "Assume standard system architecture and memory hierarchy unless specified"
                        .into(),
                );
                assumptions.push(
                    "Assume high efficiency, reliability, and correctness requirements".into(),
                );
            }
            "science" | "scientific-research" => {
                assumptions.push(
                    "Assume adherence to the scientific method and empirical reproducibility"
                        .into(),
                );
                assumptions.push("Assume peer-reviewed standards for statistical validity".into());
            }
            "health" | "medical" | "pharma" => {
                assumptions.push(
                    "Assume evidence-based clinical practices and safety-first protocols".into(),
                );
                assumptions.push(
                    "Assume compliance with relevant healthcare regulations and ethics".into(),
                );
            }
            "software" | "software-engineering" | "devops" | "devops-infra" => {
                if !lower.contains("legacy") && !lower.contains("old") {
                    assumptions.push(
                        "Assume modern, idiomatic technology stack and best practices".into(),
                    );
                }
                assumptions.push(
                    "Assume production-grade requirements (security, logging, error handling)"
                        .into(),
                );
            }
            "data-science" | "ai-ml" => {
                assumptions
                    .push("Assume data is imperfect and requires preprocessing/cleaning".into());
                assumptions.push(
                    "Assume model scalability and ethical considerations are paramount".into(),
                );
            }
            "real-estate" => match profile.perspective {
                crate::npae::aggressive::intent::ActorPerspective::Consumer => {
                    assumptions.push(
                        "Assume baseline buyer variables (target purchase budget, credit score tier, down payment %, and state-specific closing customs: attorney state vs. escrow/title state) must be explicitly stated or parameterized rather than silently defaulting to broad national averages"
                            .into(),
                    );
                    assumptions.push(
                        "Assume full contingency protection (inspection, appraisal, and financing contingencies) and itemized cash-to-close accounting (down payment + 2%–5% closing costs + 3–6 months emergency reserves)"
                            .into(),
                    );
                }
                _ => {
                    assumptions.push(
                        "Assume local MLS rules, state real-estate commission licensing requirements, and fiduciary disclosure laws apply"
                            .into(),
                    );
                    assumptions.push(
                        "Assume transaction underwriting accounts for closing costs, vacancy reserves, and local market comparables"
                            .into(),
                    );
                }
            },
            _ => {}
        }
    }

    // Intent-based assumptions
    match profile.primary_intent {
        crate::npae::aggressive::intent::IntentClass::Build => {
            assumptions
                .push("Assume output should be immediately actionable and structured".into());
        }
        crate::npae::aggressive::intent::IntentClass::Explain => {
            assumptions.push(
                "Assume the audience needs foundational concepts clarified before deep dives"
                    .into(),
            );
        }
        crate::npae::aggressive::intent::IntentClass::Debug => {
            assumptions
                .push("Assume underlying systems are standard unless otherwise specified".into());
        }
        _ => {}
    }

    assumptions
}

/// Derive dynamic final instruction based on intent and goal engineering using precomputed [`BuildContext`].
pub(super) fn derive_dynamic_instruction_with_ctx(
    profile: &IntentProfile,
    ctx: &BuildContext<'_>,
) -> String {
    if let Some(tax) = ctx.domain_taxonomy_entry {
        if let Some(ref instructions) = tax.dynamic_instructions {
            if let Some(instr) =
                resolve_template_key(instructions, profile.perspective, &profile.primary_intent)
            {
                if !instr.is_empty() {
                    return instr.clone();
                }
            }
        }
    }
    let is_financial = ctx.is_wealth_query;
    let is_business = ctx.is_enterprise_business;

    if is_financial && !profile.domain.starts_with("workplace") {
        return "Execute the comprehensive Goal-Decomposition and Wealth-Engineering plan detailed in the Roadmap section, ensuring each of the 8 quantitative milestones is addressed with mathematical rigor.".into();
    }

    if is_business {
        return "Execute the Enterprise & Business Growth Engineering plan detailed in the Roadmap section, ensuring all valuation milestones and unit economic validations are addressed.".into();
    }

    match profile.primary_intent {
        IntentClass::Explain => {
            if matches!(
                profile.user_knowledge,
                crate::npae::aggressive::intent::KnowledgeLevel::Novice
            ) || profile.perspective == crate::npae::aggressive::intent::ActorPerspective::Consumer
            {
                "Start with a clear, jargon-free overview, walk through each step with concrete examples, and highlight common mistakes to avoid.".into()
            } else {
                "Open with a 1-sentence core intuition, build a clear mental model, walk through the step-by-step mechanism with a concrete worked example, and conclude with top misconceptions to avoid.".into()
            }
        }
        IntentClass::Build => match profile.domain.as_str() {
            "software" | "software-engineering" | "devops" | "devops-infra" | "computers" => {
                "State key architectural trade-offs first, then deliver complete, runnable, production-grade code (zero stubs or placeholders), enumerate edge cases and failure modes, and provide concrete verification tests.".into()
            }
            "legal" => {
                "Draft the requested legal analysis or instrument with jurisdiction-specific statutory precision, explicit liability and indemnification safeguards, and compliance checkpoints.".into()
            }
            "medical" | "health" | "pharma" => {
                "Construct the requested clinical protocol grounded in peer-reviewed evidence, explicit contraindications, safety monitoring, and regulatory standards.".into()
            }
            "nutrition" | "sports-nutrition" => {
                "Design the requested nutrition protocol with quantified caloric/macronutrient targets, meal structure, and strict adherence to dietary exclusions.".into()
            }
            "education" => {
                "Design the requested curriculum or learning plan with measurable learning objectives, active-recall scaffolding, and mastery assessments.".into()
            }
            "travel" => {
                "Construct the requested travel itinerary with geographically optimized daily logistics, itemized budget adherence, and traveler constraint compliance.".into()
            }
            "cybersecurity" => {
                "Architect the requested security defense or assessment plan covering threat vectors, control hardening, detection telemetry, and incident response.".into()
            }
            "data-science" | "ai-ml" => {
                "Design the requested ML/data pipeline with leak-free validation splits, concrete baseline benchmarks, error analysis, and deployment drift guardrails.".into()
            }
            "real-estate" => match profile.perspective {
                crate::npae::aggressive::intent::ActorPerspective::Consumer => {
                    "Construct the requested home-buying and mortgage readiness plan with neighborhood CMA valuation, inspection due diligence, and closing settlement milestones.".into()
                }
                _ => {
                    "Develop the requested real estate strategy with comparable market underwriting, regulatory compliance, and transaction milestones.".into()
                }
            },
            _ => "Deliver a complete, directly actionable solution with explicit trade-offs, edge cases, and verification steps.".into(),
        },
        IntentClass::Debug => {
            "Follow this diagnostic protocol: triage symptoms, rank the top 2-3 root-cause hypotheses with discriminating checks, provide the minimal verified fix explaining the root cause, and list regression tests.".into()
        }
        IntentClass::Analyze => {
            "Follow this analytical protocol: lead with an Executive Verdict, present a structured Comparison/Trade-off Matrix with quantified evidence, surface key risks and second-order effects, and end with a prioritized action plan.".into()
        }
        IntentClass::Transform => {
            "State the preservation invariants first, output the exact transformed artifact with 100% logic/data integrity, and conclude with a concise summary of changes and edge cases handled.".into()
        }
    }
}

/// Derive a high-resolution persona anchor for the role using the pre-resolved [`DomainTaxonomy`] entry.
pub(super) fn derive_persona_anchor_with_ctx(
    domain: &str,
    role: &str,
    perspective: crate::npae::aggressive::intent::ActorPerspective,
    domain_taxonomy_entry: Option<&crate::npae::aggressive::config::DomainTaxonomy>,
) -> String {
    if domain == "real-estate"
        && perspective == crate::npae::aggressive::intent::ActorPerspective::Consumer
    {
        return format!(
            "Experienced {role} specializing in residential homebuying, mortgage affordability & DTI analysis, neighborhood Comparative Market Analysis (CMA), inspection contingencies, and closing settlement."
        );
    }

    if let Some(tax) = domain_taxonomy_entry {
        if let Some(ref template) = tax.persona_template {
            return template.replace("{role}", role);
        }
    }

    match domain {
        "business" | "business-strategy" => format!("Senior {} with specialization in market scaling, resource optimization, and strategic growth.", role),
        "real-estate" => format!("Experienced {} specializing in licensing compliance, property marketing, client acquisition, and agency operations.", role),
        "software" | "software-engineering" => format!("Expert {} focused on scalable architecture, clean code principles, and performance optimization.", role),
        "devops" | "devops-infra" => format!("Principal {} specializing in cloud infrastructure, CI/CD automation, reliability engineering, and observability.", role),
        "computers" => format!("Principal {} specializing in low-level systems architecture, operating systems, hardware-software co-design, and runtime performance.", role),
        "ai-ml" => format!("Senior {} specializing in large-scale model alignment, safety protocols, and LLM architecture.", role),
        "data-science" => format!("Senior {} specializing in statistical modeling, feature engineering, predictive analytics, and experimental rigor.", role),
        "medical" | "health" | "pharma" => format!("Specialized {} with clinical expertise, diagnostic precision, and evidence-based practice.", role),
        "nutrition" | "sports-nutrition" => format!("Certified {} specializing in metabolic health, macronutrient periodization, and dietary protocol design.", role),
        "finance" => format!("Senior {} focused on quantitative analysis, risk management, and financial modeling.", role),
        "cybersecurity" => format!("Security {} specializing in threat detection, vulnerability research, and incident response.", role),
        "legal" => format!("Senior {} with expertise in regulatory compliance, contract law, and strategic advisory.", role),
        "education" => format!("Experienced {} specializing in curriculum design, cognitive load optimization, and mastery-based pedagogy.", role),
        "travel" => format!("Experienced {} specializing in bespoke itinerary planning, destination logistics, and travel budget optimization.", role),
        "marketing" | "ecommerce" => format!("Senior {} specializing in customer acquisition funnels, conversion rate optimization, and growth analytics.", role),
        "science" | "scientific-research" => format!("Principal {} specializing in experimental design, hypothesis testing, and peer-reviewed empirical methodology.", role),
        "workplace-productivity" => format!("Senior {} specializing in organizational dynamics, distributed team performance, and workplace culture.", role),
        "creative" | "creative-writing" => format!("Acclaimed {} specializing in narrative structure, rhetorical clarity, and audience engagement.", role),
        _ => format!("Professional {} with deep expertise in the {} domain and related methodologies.", role, domain),
    }
}
