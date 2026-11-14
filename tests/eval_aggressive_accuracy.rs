//! Accuracy evaluation suite for the NPAE Aggressive path (Step 0 baseline + Step 1 verification).
//!
//! Rules §2 & §9: "Evaluation before claims" — measures domain accuracy, macro-F1, wrong-domain vs
//! abstain rate, intent accuracy, and constraint recall on a curated golden set covering
//! finance, medical, nutrition, legal, education, travel, and adversarial substring cases.
//!
//! Run baseline/eval report: `cargo test --test eval_aggressive_accuracy -- --nocapture`

#![allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

use std::collections::{BTreeMap, BTreeSet};
use token_compress_engine::npae::aggressive::config::{ConfigLoader, UnifiedConfig};
use token_compress_engine::npae::aggressive::constraints::extract_constraints;
use token_compress_engine::npae::aggressive::intent::{extract_with_config, IntentClass};
use token_compress_engine::npae::compression::pipeline::run_parallel_pipeline;
use token_compress_engine::npae::ory::OryEngine;

struct GoldenCase {
    id: &'static str,
    prompt: &'static str,
    /// Acceptable canonical domains (any match counts as domain-correct).
    expected_domains: &'static [&'static str],
    expected_intent: IntentClass,
    /// Expected constraint description substrings from `extract_constraints` (when applicable).
    expected_constraints: &'static [&'static str],
}

const GOLDEN_SET: &[GoldenCase] = &[
    // --- FINANCE (6) ---
    GoldenCase {
        id: "FIN-01",
        prompt: "Build a diversified stock and dividend portfolio to grow my wealth and passive income over 10 years",
        expected_domains: &["finance"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "FIN-02",
        prompt: "Analyze my monthly cashflow, loan interest, and tax liability to optimize my savings rate",
        expected_domains: &["finance"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "FIN-03",
        prompt: "Explain how compound interest and asset allocation work for retirement investing",
        expected_domains: &["finance"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "FIN-04",
        prompt: "Create a budgeting and debt payoff plan to increase my net worth and reach financial freedom",
        expected_domains: &["finance"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "FIN-05",
        prompt: "Compare equity index funds versus banking fixed deposits for capital preservation",
        expected_domains: &["finance"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "FIN-06",
        prompt: "How to make millions in pharma by building a biotech empire with FDA compliance",
        expected_domains: &["business", "finance"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    // --- MEDICAL (5) ---
    GoldenCase {
        id: "MED-01",
        prompt: "Explain the clinical diagnosis and treatment protocol for a cardiology patient with chest symptoms",
        expected_domains: &["medical", "health"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "MED-02",
        prompt: "Analyze the pathology and radiology findings before scheduling hospital surgery for the patient",
        expected_domains: &["medical", "health"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "MED-03",
        prompt: "Design a clinical rehabilitation and preventative healthcare screening protocol for doctor review",
        expected_domains: &["medical", "health"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "MED-04",
        prompt: "Compare neurology and oncology clinical treatment pathways for hospital patient care",
        expected_domains: &["medical", "health"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "MED-05",
        prompt: "What is the physiological mechanism of cardiovascular disease and how do doctors diagnose symptoms?",
        expected_domains: &["medical", "health"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    // --- NUTRITION (5) ---
    GoldenCase {
        id: "NUT-01",
        prompt: "Create a high protein vegetarian meal plan with no gluten and no dairy for metabolic health",
        expected_domains: &["nutrition"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["Gluten", "dairy"],
    },
    GoldenCase {
        id: "NUT-02",
        prompt: "Design a keto diet and macro caloric deficit nutrition protocol without nightshades and low fiber",
        expected_domains: &["nutrition"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["Nightshade", "Low fiber"],
    },
    GoldenCase {
        id: "NUT-03",
        prompt: "Explain how probiotic supplements, vitamins, and micronutrients affect gut hydration and diet",
        expected_domains: &["nutrition"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "NUT-04",
        prompt: "Analyze my daily calorie and carbohydrate macro intake for a vegan dietitian meal protocol",
        expected_domains: &["nutrition"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "NUT-05",
        prompt: "Build a high carb loading nutrition and diet plan for marathon race week avoiding gluten",
        expected_domains: &["nutrition"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["High carbohydrate", "Gluten"],
    },
    // --- LEGAL (5) ---
    GoldenCase {
        id: "LEG-01",
        prompt: "Review this commercial contract clause for liability, intellectual property, and GDPR compliance",
        expected_domains: &["legal"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &["GDPR"],
    },
    GoldenCase {
        id: "LEG-02",
        prompt: "Explain trademark and patent litigation procedure under federal jurisdiction and statute",
        expected_domains: &["legal"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "LEG-03",
        prompt: "Draft a legal compliance and data privacy regulation checklist with an attorney for GDPR",
        expected_domains: &["legal"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["GDPR"],
    },
    GoldenCase {
        id: "LEG-04",
        prompt: "Analyze the legal liability and statutory compliance risks in this vendor contract",
        expected_domains: &["legal"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "LEG-05",
        prompt: "Is my NDA contract enforceable in this jurisdiction if the attorney omitted a liability clause?",
        expected_domains: &["legal"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    // --- EDUCATION (5) ---
    GoldenCase {
        id: "EDU-01",
        prompt: "Design a 12-week university course syllabus and pedagogy curriculum with student assessments",
        expected_domains: &["education"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "EDU-02",
        prompt: "Explain active recall pedagogy strategies to help a tutor teach an e-learning lecture",
        expected_domains: &["education"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "EDU-03",
        prompt: "Analyze student assessment scores and academic certification outcomes across the curriculum",
        expected_domains: &["education"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "EDU-04",
        prompt: "Create an academic instructor lecture plan and student training assessment rubric",
        expected_domains: &["education"],
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "EDU-05",
        prompt: "Compare synchronous classroom lecture pedagogy with self-paced e-learning course design",
        expected_domains: &["education"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    // --- TRAVEL (5) ---
    GoldenCase {
        id: "TRV-01",
        prompt: "Plan a 5-day Goa travel itinerary and sightseeing tour under 30000 INR with vegetarian food only",
        expected_domains: &["travel"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["vegetarian", "budget"],
    },
    GoldenCase {
        id: "TRV-02",
        prompt: "Compare hotel resort booking options and flight transit routes for a family holiday vacation",
        expected_domains: &["travel"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "TRV-03",
        prompt: "Create a backpacking roadtrip and homestay excursion itinerary under budget to optimize cost",
        expected_domains: &["travel"],
        expected_intent: IntentClass::Build,
        expected_constraints: &["budget"],
    },
    GoldenCase {
        id: "TRV-04",
        prompt: "Explain the destination tourism logistics and sightseeing schedule for a 7-day holiday trip",
        expected_domains: &["travel"],
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "TRV-05",
        prompt: "Analyze travel flight and hotel booking costs for a sightseeing vacation tour",
        expected_domains: &["travel"],
        expected_intent: IntentClass::Analyze,
        expected_constraints: &[],
    },
    // --- ADVERSARIAL SUBSTRING / INTENT CASES (Step 1.2 & 1.4) ---
    GoldenCase {
        id: "ADV-01",
        prompt: "Explain the prefix notation and terror management theory in an academic lecture course",
        expected_domains: &["education"],
        // Must be Explain, NOT Debug ("prefix" contains "fix", "terror" contains "error")
        expected_intent: IntentClass::Explain,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "ADV-02",
        prompt: "Build a classroom syllabus and anyhow include a prefix index for the student course",
        expected_domains: &["education"],
        // Must be Build, NOT Analyze/Debug ("anyhow" contains "why", "prefix" contains "fix")
        expected_intent: IntentClass::Build,
        expected_constraints: &[],
    },
    GoldenCase {
        id: "ADV-03",
        prompt: "Refactor the curriculum lecture notes and prefix each student module with clear outcomes",
        expected_domains: &["education"],
        // Must be Transform ("refactor" = +0.6), NOT Debug ("prefix" = +0.7 if substring matched!)
        expected_intent: IntentClass::Transform,
        expected_constraints: &[],
    },
];

/// Simulate the domain resolution logic in `AggressiveEngine::run` (lines 37–71 of `engine.rs`)
/// so we measure end-to-end domain accuracy after Ory's confidence check.
fn run_aggressive_domain_and_intent(
    prompt: &str,
    config: &UnifiedConfig,
    ory: &OryEngine,
) -> (String, IntentClass, f32, bool) {
    let repr = run_parallel_pipeline(prompt).expect("compression pipeline");
    let ory_res = ory.process(prompt, config).expect("ory process");
    let mut profile = extract_with_config(&repr, prompt, Some(config)).expect("extract intent");

    let ory_canonical = ory_res
        .audit
        .existing_domain_match
        .as_deref()
        .unwrap_or(&ory_res.intent.inferred_domain);
    let ory_canonical = token_compress_engine::npae::aggressive::config::normalize_domain(
        ory_canonical,
        &config.domain_taxonomy,
    )
    .to_string();

    if !profile.rule_locked && ory_res.intent.confidence_score > profile.confidence + 0.15 {
        profile.domain = ory_canonical;
        profile.confidence = ory_res.intent.confidence_score;
    }

    (
        profile.domain,
        profile.primary_intent,
        profile.confidence,
        profile.rule_locked,
    )
}

#[test]
fn evaluate_aggressive_golden_set() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let ory = OryEngine::new();

    let mut domain_hits = 0usize;
    let mut wrong_domain = 0usize;
    let mut abstained = 0usize;
    let mut intent_hits = 0usize;
    let mut total_constraints = 0usize;
    let mut matched_constraints = 0usize;

    let mut per_domain_tp: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_domain_fp: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_domain_fn: BTreeMap<&str, usize> = BTreeMap::new();
    let canonical_domains: BTreeSet<&str> = [
        "finance",
        "medical",
        "nutrition",
        "legal",
        "education",
        "travel",
    ]
    .into_iter()
    .collect();

    for case in GOLDEN_SET {
        let (pred_domain, pred_intent, conf, locked) =
            run_aggressive_domain_and_intent(case.prompt, &config, &ory);

        let domain_ok = case.expected_domains.contains(&pred_domain.as_str());
        let primary_gold = case.expected_domains[0];
        if domain_ok {
            domain_hits += 1;
            *per_domain_tp.entry(primary_gold).or_default() += 1;
        } else {
            *per_domain_fn.entry(primary_gold).or_default() += 1;
            if pred_domain == "general" {
                abstained += 1;
            } else {
                wrong_domain += 1;
                if canonical_domains.contains(pred_domain.as_str()) {
                    *per_domain_fp
                        .entry(Box::leak(pred_domain.clone().into_boxed_str()))
                        .or_default() += 1;
                }
            }
        }

        let intent_ok = pred_intent == case.expected_intent;
        if intent_ok {
            intent_hits += 1;
        }

        let (inc, forb) =
            extract_constraints(case.prompt, &config.domain_taxonomy, &config.constraints);
        let all_c: Vec<String> = inc.into_iter().chain(forb).collect();
        for exp_c in case.expected_constraints {
            total_constraints += 1;
            if all_c
                .iter()
                .any(|c| c.to_lowercase().contains(&exp_c.to_lowercase()))
            {
                matched_constraints += 1;
            }
        }

        if !domain_ok || !intent_ok {
            println!(
                "[MISS] {} | domain: pred='{}' exp={:?} (conf={:.3}, locked={}) | intent: pred={:?} exp={:?}",
                case.id,
                pred_domain,
                case.expected_domains,
                conf,
                locked,
                pred_intent,
                case.expected_intent
            );
        }
    }

    let n = GOLDEN_SET.len() as f64;
    let mut f1_sum = 0.0f64;
    for &d in &canonical_domains {
        let tp = *per_domain_tp.get(d).unwrap_or(&0) as f64;
        let fp = *per_domain_fp.get(d).unwrap_or(&0) as f64;
        let f_n = *per_domain_fn.get(d).unwrap_or(&0) as f64;
        let f1 = if 2.0 * tp + fp + f_n > 0.0 {
            (2.0 * tp) / (2.0 * tp + fp + f_n)
        } else {
            0.0
        };
        f1_sum += f1;
    }
    let macro_f1 = f1_sum / canonical_domains.len() as f64;
    let constraint_recall = if total_constraints > 0 {
        matched_constraints as f64 / total_constraints as f64
    } else {
        1.0
    };

    println!(
        "\n=== AGGRESSIVE GOLDEN SET EVALUATION ({} cases) ===",
        GOLDEN_SET.len()
    );
    println!(
        "Domain Accuracy   : {}/{} ({:.1}%)",
        domain_hits,
        GOLDEN_SET.len(),
        (domain_hits as f64 / n) * 100.0
    );
    println!("Domain Macro-F1   : {:.3}", macro_f1);
    println!(
        "Wrong-Domain Rate : {}/{} ({:.1}%) | Abstain Rate: {}/{} ({:.1}%)",
        wrong_domain,
        GOLDEN_SET.len(),
        (wrong_domain as f64 / n) * 100.0,
        abstained,
        GOLDEN_SET.len(),
        (abstained as f64 / n) * 100.0
    );
    println!(
        "Intent Accuracy   : {}/{} ({:.1}%)",
        intent_hits,
        GOLDEN_SET.len(),
        (intent_hits as f64 / n) * 100.0
    );
    println!(
        "Constraint Recall : {}/{} ({:.1}%)",
        matched_constraints,
        total_constraints,
        constraint_recall * 100.0
    );

    assert!(
        domain_hits >= 32,
        "Domain accuracy regression: {}/{}",
        domain_hits,
        GOLDEN_SET.len()
    );
    assert!(
        intent_hits >= 32,
        "Intent accuracy regression: {}/{}",
        intent_hits,
        GOLDEN_SET.len()
    );
    assert_eq!(
        matched_constraints, total_constraints,
        "Expected 100% constraint recall on golden set"
    );
}

#[test]
fn test_multi_label_domains_and_verbatim_constraints_e2e() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let raw = "Plan a 1800 cal vegetarian meal and diet itinerary for my 7-day holiday trip to Goa with no peanuts and no shellfish under 2000 usd";
    let repr = run_parallel_pipeline(raw).expect("compression pipeline");
    let profile = extract_with_config(&repr, raw, Some(&config)).expect("extract intent");

    // Primary + secondary must cover both nutrition and travel
    let mut all_domains = vec![profile.domain.clone()];
    all_domains.extend(profile.secondary_domains.clone());
    assert!(
        all_domains.contains(&"nutrition".to_string())
            && all_domains.contains(&"travel".to_string()),
        "Expected multi-label domains to include both nutrition and travel, got: {:?}",
        all_domains
    );

    let (inc, forb) = extract_constraints(raw, &config.domain_taxonomy, &config.constraints);
    assert!(
        forb.iter().any(|c| c.to_lowercase().contains("peanuts")),
        "Verbatim exclusion 'no peanuts' must be captured, got: {:?}",
        forb
    );
    assert!(
        forb.iter().any(|c| c.to_lowercase().contains("shellfish")),
        "Verbatim exclusion 'no shellfish' must be captured, got: {:?}",
        forb
    );
    assert!(
        inc.iter().any(|c| c.to_lowercase().contains("1800 cal")),
        "Numeric bound '1800 cal' must be captured, got: {:?}",
        inc
    );
    assert!(
        inc.iter().any(|c| c.to_lowercase().contains("2000 usd")),
        "Numeric bound '2000 usd' must be captured, got: {:?}",
        inc
    );

    let role = token_compress_engine::npae::aggressive::role::generate_role(
        &profile,
        raw,
        &config.domain_taxonomy,
        &config.roles,
    );
    let resolved = token_compress_engine::npae::aggressive::resolver::ResolvedPrompt {
        role,
        inclusions: inc,
        forbidden: forb,
        confidence: profile.confidence,
        status: "resolved".to_string(),
    };
    let reconstructed = token_compress_engine::types::ReconstructedInput::default();
    let structured = token_compress_engine::npae::aggressive::structurer::build(
        &profile,
        raw,
        &resolved,
        Some(&config),
        &reconstructed,
        None,
    )
    .expect("build structured prompt");
    let rendered = token_compress_engine::npae::aggressive::structurer::render_crisp_prompt(
        &structured,
        &[],
        raw,
    );

    assert!(
        rendered.contains("Secondary domain:"),
        "Rendered CRISP prompt must surface secondary domain:\n{}",
        rendered
    );
    assert!(
        rendered.to_lowercase().contains("peanuts") && rendered.to_lowercase().contains("1800 cal"),
        "Rendered CRISP prompt must include verbatim constraints:\n{}",
        rendered
    );
}

#[test]
fn test_domain_required_slots_generate_targeted_clarifying_questions() {
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let reconstructed = token_compress_engine::types::ReconstructedInput::default();

    // 1. Legal prompt missing jurisdiction -> should emit `missing_slot:legal:jurisdiction`
    //    and generate a jurisdiction clarifying question.
    let legal_raw = "Review this contract clause for liability and attorney compliance";
    let legal_repr = run_parallel_pipeline(legal_raw).expect("compression");
    let legal_prof = extract_with_config(&legal_repr, legal_raw, Some(&config)).expect("intent");
    assert_eq!(legal_prof.domain, "legal");

    let legal_amb = token_compress_engine::npae::aggressive::ambiguity::score_for_domain(
        &legal_repr,
        legal_raw,
        &reconstructed,
        None,
        Some(&legal_prof),
    )
    .expect("ambiguity");
    assert!(
        legal_amb
            .gap_zones
            .iter()
            .any(|g| g == "missing_slot:legal:jurisdiction"),
        "Expected missing_slot:legal:jurisdiction, got: {:?}",
        legal_amb.gap_zones
    );
    let legal_qs = token_compress_engine::npae::aggressive::questions::generate_with_domain(
        &legal_repr,
        &legal_amb,
        3,
        &legal_prof.domain,
    )
    .expect("questions");
    assert!(
        legal_qs
            .iter()
            .any(|q| q.question.to_lowercase().contains("jurisdiction")),
        "Expected jurisdiction clarifying question, got: {:?}",
        legal_qs
    );

    // 2. Medical prompt missing patient age/profile and duration/medications
    let med_raw = "Explain clinical diagnosis and doctor treatment for chest symptoms";
    let med_repr = run_parallel_pipeline(med_raw).expect("compression");
    let med_prof = extract_with_config(&med_repr, med_raw, Some(&config)).expect("intent");
    let med_amb = token_compress_engine::npae::aggressive::ambiguity::score_for_domain(
        &med_repr,
        med_raw,
        &reconstructed,
        None,
        Some(&med_prof),
    )
    .expect("ambiguity");
    assert!(
        med_amb
            .gap_zones
            .iter()
            .any(|g| g == "missing_slot:medical:patient_age_or_profile")
            && med_amb
                .gap_zones
                .iter()
                .any(|g| g == "missing_slot:medical:duration_or_medications"),
        "Expected medical missing slots, got: {:?}",
        med_amb.gap_zones
    );
    let med_qs = token_compress_engine::npae::aggressive::questions::generate_with_domain(
        &med_repr,
        &med_amb,
        3,
        &med_prof.domain,
    )
    .expect("questions");
    assert!(
        med_qs
            .iter()
            .any(|q| q.question.to_lowercase().contains("age")
                || q.question.to_lowercase().contains("medication")),
        "Expected age/medication clarifying question, got: {:?}",
        med_qs
    );

    // 3. Complete travel prompt (has destination "Goa", duration "5-day", budget "30000 INR")
    //    -> should have ZERO missing travel slots.
    let trv_raw =
        "Plan a 5-day Goa travel itinerary and sightseeing tour under 30000 INR for a family";
    let trv_repr = run_parallel_pipeline(trv_raw).expect("compression");
    let trv_prof = extract_with_config(&trv_repr, trv_raw, Some(&config)).expect("intent");
    let trv_amb = token_compress_engine::npae::aggressive::ambiguity::score_for_domain(
        &trv_repr,
        trv_raw,
        &reconstructed,
        None,
        Some(&trv_prof),
    )
    .expect("ambiguity");
    assert!(
        !trv_amb
            .gap_zones
            .iter()
            .any(|g| g.starts_with("missing_slot:travel:")),
        "Complete travel prompt must not flag missing travel slots, got: {:?}",
        trv_amb.gap_zones
    );

    // 4. Expanded missing-slot detections for canonical domains:
    //    software, business, data-science, cybersecurity, real-estate.
    let expanded_slot_cases = [
        (
            "Build a backend microservice API with authentication and database caching",
            "software",
            "missing_slot:software:stack_or_environment",
            "language",
        ),
        (
            "Create a go-to-market business strategy and revenue growth plan for a new startup",
            "business",
            "missing_slot:business:target_market_or_stage",
            "market",
        ),
        (
            "Train a machine learning classification model and feature engineering pipeline",
            "data-science",
            "missing_slot:data-science:dataset_or_metric",
            "dataset",
        ),
        (
            "Conduct a cybersecurity vulnerability assessment and threat model audit",
            "cybersecurity",
            "missing_slot:cybersecurity:target_scope_or_environment",
            "scope",
        ),
        (
            "Create a real estate property investment and underwriting acquisition strategy",
            "real-estate",
            "missing_slot:real-estate:location_or_property_type",
            "property",
        ),
    ];

    for (raw_prompt, expected_dom, expected_gap, expected_q_word) in expanded_slot_cases {
        let repr = run_parallel_pipeline(raw_prompt).expect("compression");
        let mut prof = extract_with_config(&repr, raw_prompt, Some(&config)).expect("intent");
        if prof.domain != expected_dom {
            prof.domain = expected_dom.to_string();
        }
        let amb = token_compress_engine::npae::aggressive::ambiguity::score_for_domain(
            &repr,
            raw_prompt,
            &reconstructed,
            None,
            Some(&prof),
        )
        .expect("ambiguity");
        assert!(
            amb.gap_zones.iter().any(|g| g == expected_gap),
            "[{expected_dom}] Expected gap '{expected_gap}', got: {:?}",
            amb.gap_zones
        );
        let qs = token_compress_engine::npae::aggressive::questions::generate_with_domain(
            &repr,
            &amb,
            3,
            &prof.domain,
        )
        .expect("questions");
        assert!(
            qs.iter()
                .any(|q| q.question.to_lowercase().contains(expected_q_word)),
            "[{expected_dom}] Expected clarifying question mentioning '{expected_q_word}', got: {:?}",
            qs
        );
    }
}

#[tokio::test]
async fn test_aggressive_engine_structurer_and_ory_canonical_domain_roles_phases_and_validation() {
    use std::sync::Arc;
    use token_compress_engine::npae::aggressive::config::ConfigHandle;
    use token_compress_engine::npae::aggressive::engine::AggressiveEngine;
    use token_compress_engine::npae::aggressive::structurer::HttpStructurer;
    use token_compress_engine::npae::schema::types::{DeliverableType, NpaeConfig};
    use tokio::sync::Mutex;

    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let npae_cfg = NpaeConfig::default();
    let reconstructed = token_compress_engine::types::ReconstructedInput::default();

    // 1. Canonical Build prompts across software, cybersecurity, finance, legal
    //    must produce domain-specific roles, non-empty curated execution phases, and domain-specific
    //    validation steps when validation is requested.
    let build_cases = [
        (
            "Build and validate a fault-tolerant Rust microservice with PostgreSQL and Kubernetes deployment",
            "software",
            "Unit Tests",
        ),
        (
            "Design and verify a zero-trust cloud IAM and Kubernetes security architecture against OWASP threats",
            "cybersecurity",
            "Threat Model",
        ),
        (
            "Draft and verify a GDPR data privacy compliance checklist and vendor contract liability framework under EU law",
            "legal",
            "Jurisdictional",
        ),
        (
            "Build and verify a 10-year diversified equity and bond portfolio strategy with tax-loss harvesting",
            "finance",
            "Mathematical",
        ),
    ];

    for (prompt, expected_domain, expected_validation_substr) in build_cases {
        let repr = run_parallel_pipeline(prompt).expect("compression pipeline");
        let structurer = HttpStructurer {
            route: "/api/compress".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: prompt.to_string(),
        };
        let resp = AggressiveEngine::run(
            prompt,
            &repr,
            &npae_cfg,
            config_handle.clone(),
            ory_engine.clone(),
            &structurer,
            &reconstructed,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("AggressiveEngine::run failed for {expected_domain}: {e}"));

        assert_eq!(
            resp.structured_prompt.context.domain, expected_domain,
            "Expected canonical domain '{expected_domain}', got '{}'",
            resp.structured_prompt.context.domain
        );
        assert!(
            !resp.structured_prompt.role.primary.is_empty()
                && resp.structured_prompt.role.primary != "Domain Expert",
            "[{expected_domain}] Expected specific primary role, got '{}'",
            resp.structured_prompt.role.primary
        );
        assert!(
            !resp.structured_prompt.execution_phases.is_empty(),
            "[{expected_domain}] Build prompt must produce curated execution phases"
        );
        assert!(
            resp.structured_prompt
                .validation_steps
                .iter()
                .any(|s| s.name.contains(expected_validation_substr)),
            "[{expected_domain}] Expected validation step containing '{expected_validation_substr}', got: {:?}",
            resp.structured_prompt.validation_steps
        );
    }

    // 2. Pure Explain / Content prompts must NOT have forced execution phases even when OryEngine
    //    synthesizes a DynamicBlueprint (`execution_phases` must remain empty).
    let explain_prompts = [
        "Explain how compound interest and asset allocation work for retirement investing",
        "What is the physiological mechanism of cardiovascular disease and how do doctors diagnose symptoms?",
    ];

    for prompt in explain_prompts {
        let repr = run_parallel_pipeline(prompt).expect("compression pipeline");
        let structurer = HttpStructurer {
            route: "/api/compress".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            body: prompt.to_string(),
        };
        let resp = AggressiveEngine::run(
            prompt,
            &repr,
            &npae_cfg,
            config_handle.clone(),
            ory_engine.clone(),
            &structurer,
            &reconstructed,
            None,
        )
        .await
        .expect("AggressiveEngine::run on Explain prompt");

        assert!(
            resp.structured_prompt.execution_phases.is_empty()
                || resp
                    .structured_prompt
                    .objective
                    .as_ref()
                    .is_some_and(|o| o.deliverable_type != DeliverableType::Content),
            "Explain/Content prompt must not have forced generic Ory blueprint phases: {:?}",
            resp.structured_prompt.execution_phases
        );
        assert!(
            resp.structured_prompt.execution_phases.is_empty(),
            "Pure Explain prompt '{prompt}' must keep execution_phases empty, got: {:?}",
            resp.structured_prompt.execution_phases
        );
    }
}

#[tokio::test]
async fn test_buying_a_house_consumer_vs_agent_and_no_cross_domain_hijack() {
    use std::sync::Arc;
    use token_compress_engine::npae::aggressive::config::ConfigHandle;
    use token_compress_engine::npae::aggressive::engine::AggressiveEngine;
    use token_compress_engine::npae::aggressive::structurer::HttpStructurer;
    use token_compress_engine::npae::schema::types::NpaeConfig;
    use tokio::sync::Mutex;

    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let npae_cfg = NpaeConfig::default();
    let reconstructed = token_compress_engine::types::ReconstructedInput::default();

    // 1. Consumer home-buying prompt with adversarial triggers ("# TASK:", filler, "learn", "$500k",
    //    "appraisal", "appreciate", "do not use adjustable-rate mortgages")
    let consumer_prompt = "# TASK: I would really appreciate it if you could please help me learn and build a step-by-step plan for buying a house in Austin with a $500k budget, evaluate mortgage options and neighborhood appraisal comparables, and do not use adjustable-rate mortgages.";
    let repr = run_parallel_pipeline(consumer_prompt).expect("compression pipeline");
    let structurer = HttpStructurer {
        route: "/api/compress".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: consumer_prompt.to_string(),
    };
    let resp = AggressiveEngine::run(
        consumer_prompt,
        &repr,
        &npae_cfg,
        config_handle.clone(),
        ory_engine.clone(),
        &structurer,
        &reconstructed,
        None,
    )
    .await
    .expect("AggressiveEngine::run on consumer home-buying prompt");

    assert_eq!(
        resp.structured_prompt.context.domain, "real-estate",
        "Expected domain 'real-estate', got '{}'",
        resp.structured_prompt.context.domain
    );
    assert!(
        resp.structured_prompt
            .role
            .primary
            .starts_with("Residential Home-Buying & Mortgage Advisor"),
        "Expected consumer home-buying role, got '{}'",
        resp.structured_prompt.role.primary
    );

    let phase_names: Vec<&str> = resp
        .structured_prompt
        .execution_phases
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    for expected_phase in [
        "Financial Readiness & Mortgage Pre-Approval",
        "Property Search, Neighborhood Comps & Offer Strategy",
        "Inspection, Appraisal, Underwriting & Closing",
    ] {
        assert!(
            phase_names.contains(&expected_phase),
            "Expected consumer phase '{expected_phase}', got: {phase_names:?}"
        );
    }

    for forbidden in [
        "Licensing & Pre-Registration",
        "Broker Sponsorship",
        "Business Entity (LLC)",
        "Digital Product",
        "Wealth-Engineering",
        "Income ≠ Revenue",
        "DomainTransfer",
    ] {
        assert!(
            !resp.optimized_prompt.contains(forbidden),
            "Consumer home-buying output must NOT contain '{forbidden}':\n{}",
            resp.optimized_prompt
        );
    }

    // Verify `# TASK / INSTRUCTION:` pruned duplicate `# TASK:` header and conversational filler
    // while preserving `$500k`, `Austin`, and `adjustable-rate mortgages`.
    let task_section = resp
        .optimized_prompt
        .split("# TASK / INSTRUCTION:")
        .nth(1)
        .unwrap_or(&resp.optimized_prompt);
    let task_block = task_section.split("\n# ").next().unwrap_or(task_section);
    assert!(
        !task_block.contains("# TASK:"),
        "Duplicate '# TASK:' header must be pruned from task block: {task_block}"
    );
    assert!(
        !task_block
            .to_lowercase()
            .contains("i would really appreciate it if you could please"),
        "Conversational filler must be pruned from task block: {task_block}"
    );
    assert!(
        task_block.contains("$500k")
            && task_block.contains("Austin")
            && task_block.contains("adjustable-rate mortgages"),
        "Task block must preserve '$500k', 'Austin', and 'adjustable-rate mortgages': {task_block}"
    );

    // 2. Practitioner real-estate agent licensing prompt must still emit practitioner phases
    let practitioner_prompt =
        "How do I become a licensed real estate agent and build a real estate brokerage from scratch?";
    let pract_repr = run_parallel_pipeline(practitioner_prompt).expect("compression pipeline");
    let pract_structurer = HttpStructurer {
        route: "/api/compress".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: practitioner_prompt.to_string(),
    };
    let pract_resp = AggressiveEngine::run(
        practitioner_prompt,
        &pract_repr,
        &npae_cfg,
        config_handle,
        ory_engine,
        &pract_structurer,
        &reconstructed,
        None,
    )
    .await
    .expect("AggressiveEngine::run on practitioner prompt");

    assert_eq!(pract_resp.structured_prompt.context.domain, "real-estate");
    assert!(
        pract_resp
            .optimized_prompt
            .contains("Licensing & Pre-Registration")
            || pract_resp.optimized_prompt.contains("Broker Sponsorship"),
        "Practitioner real-estate prompt must emit licensing/brokerage phases:\n{}",
        pract_resp.optimized_prompt
    );
}

#[tokio::test]
async fn test_multi_domain_consumer_vs_practitioner_separation() {
    use std::sync::Arc;
    use token_compress_engine::npae::aggressive::config::ConfigHandle;
    use token_compress_engine::npae::aggressive::engine::AggressiveEngine;
    use token_compress_engine::npae::aggressive::intent::{
        detect_actor_perspective, ActorPerspective,
    };
    use token_compress_engine::npae::aggressive::structurer::HttpStructurer;
    use token_compress_engine::npae::schema::types::NpaeConfig;
    use tokio::sync::Mutex;

    // 1. Perspective detection across legal, finance, medical, and education
    let perspective_cases = [
        (
            "My landlord is refusing to return my security deposit, how do I dispute this?",
            "legal",
            ActorPerspective::Consumer,
        ),
        (
            "Draft a commercial vendor indemnification contract with GDPR compliance",
            "legal",
            ActorPerspective::Practitioner,
        ),
        (
            "Help me set up a personal budget from my paycheck to pay off credit card debt",
            "finance",
            ActorPerspective::Consumer,
        ),
        (
            "Build a Sharpe-optimized multi-asset portfolio with CAGR and underwriting targets",
            "finance",
            ActorPerspective::Investor,
        ),
        (
            "Explain my symptoms of knee pain and when I should see a doctor",
            "medical",
            ActorPerspective::Consumer,
        ),
        (
            "Design a clinical trial protocol with FDA compliance for cardiology patients",
            "medical",
            ActorPerspective::Practitioner,
        ),
        (
            "Help me learn calculus and study for my exam",
            "education",
            ActorPerspective::Consumer,
        ),
        (
            "Design a 12-week university syllabus and pedagogy curriculum for instructors",
            "education",
            ActorPerspective::Practitioner,
        ),
    ];

    for (prompt, domain, expected) in perspective_cases {
        let got = detect_actor_perspective(&prompt.to_lowercase(), domain);
        assert_eq!(
            got, expected,
            "Perspective mismatch for domain '{domain}' on prompt '{prompt}': expected {expected:?}, got {got:?}"
        );
    }

    // 2. End-to-end check: "Help me learn calculus and study for my exam" must NOT trigger
    //    "Wealth-Engineering" from "learn" containing "earn".
    let config = ConfigLoader::load("config/unified.json").expect("load unified.json");
    let config_handle = Arc::new(ConfigHandle::new(config));
    let ory_engine = Arc::new(Mutex::new(OryEngine::new()));
    let npae_cfg = NpaeConfig::default();
    let reconstructed = token_compress_engine::types::ReconstructedInput::default();

    let edu_prompt = "Help me learn calculus and study for my exam";
    let repr = run_parallel_pipeline(edu_prompt).expect("compression pipeline");
    let structurer = HttpStructurer {
        route: "/api/compress".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: edu_prompt.to_string(),
    };
    let resp = AggressiveEngine::run(
        edu_prompt,
        &repr,
        &npae_cfg,
        config_handle,
        ory_engine,
        &structurer,
        &reconstructed,
        None,
    )
    .await
    .expect("AggressiveEngine::run on education consumer prompt");

    assert_eq!(resp.structured_prompt.context.domain, "education");
    assert!(
        !resp.optimized_prompt.contains("Wealth-Engineering")
            && !resp.optimized_prompt.contains("Income ≠ Revenue"),
        "Education prompt with 'learn' must NOT trigger Wealth-Engineering:\n{}",
        resp.optimized_prompt
    );
}
