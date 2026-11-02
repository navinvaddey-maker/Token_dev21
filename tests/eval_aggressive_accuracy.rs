//! Accuracy evaluation suite for the NPAE Aggressive path (Step 0 baseline + Step 1 verification).
//!
//! Rules §2 & §9: "Evaluation before claims" — measures domain accuracy, macro-F1, wrong-domain vs
//! abstain rate, intent accuracy, and constraint recall on a curated golden set covering
//! finance, medical, nutrition, legal, education, travel, and adversarial substring cases.
//!
//! Run baseline/eval report: `cargo test --test eval_aggressive_accuracy -- --nocapture`

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
}
