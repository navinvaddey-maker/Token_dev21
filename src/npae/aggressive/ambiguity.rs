//! Domain-Aware Ambiguity & Missing Required Slot Detector.
//!
//! // ARCH: ARCHITECTURE.md §1.4 (Step 6) — evaluates structural ambiguity and per-domain required
//! // slots across canonical domains (`legal`, `medical`/`health`, `nutrition`, `finance`,
//! // `education`, `travel`, `software`/`devops`, `business`, `data-science`/`ai-ml`,
//! // `cybersecurity`, `real-estate`) to drive targeted clarifying questions.

use crate::npae::compression::types::CompressedRepr;
use crate::npae::schema::types::AmbiguityAnalysis;
use std::collections::HashSet;

use super::intent::IntentProfile;
use super::rag_context::DerivedRagContext;

/// Scores ambiguity without domain-specific slot checks.
///
/// # Errors
/// Never returns `Err` in current implementation.
///
/// # Panics
/// Never.
pub fn score(
    repr: &CompressedRepr,
    raw: &str,
    reconstructed: &crate::types::ReconstructedInput,
    derived_rag: Option<&DerivedRagContext>,
) -> Result<AmbiguityAnalysis, String> {
    score_for_domain(repr, raw, reconstructed, derived_rag, None)
}

/// Domain-aware ambiguity scoring (Step 4).
///
/// Evaluates both structural/generic ambiguity and missing required domain slots
/// across primary and secondary domains (`finance`, `medical`/`health`, `nutrition`,
/// `legal`, `education`, `travel`, `software`/`devops`, `business`, `data-science`/`ai-ml`,
/// `cybersecurity`, `real-estate`).
///
/// # Errors
/// Never returns `Err` in current implementation.
///
/// # Panics
/// Never.
pub fn score_for_domain(
    repr: &CompressedRepr,
    raw: &str,
    reconstructed: &crate::types::ReconstructedInput,
    derived_rag: Option<&DerivedRagContext>,
    profile: Option<&IntentProfile>,
) -> Result<AmbiguityAnalysis, String> {
    let mut gap_zones = Vec::new();
    let mut score = 0.0;

    // 1. Structural Ambiguity (from Reconstruction Stage -1)
    if !reconstructed.ambiguity_register.is_empty() {
        let reg_score = (reconstructed.ambiguity_register.len() as f32 * 0.15).min(0.5);
        score += reg_score;
        for flag in &reconstructed.ambiguity_register {
            let gap_str = format!("structural_ambiguity: {}", flag.reason);
            // Skip gap zone if RAG facts cover this specific gap
            if let Some(rag) = derived_rag {
                if rag.covered_gaps.contains(&gap_str) || rag.covered_gaps.contains(&flag.reason) {
                    score -= (reg_score * 0.5).min(score);
                    continue;
                }
            }
            gap_zones.push(gap_str);
        }
    }

    let lower_raw = raw.to_lowercase();
    let words: HashSet<&str> = lower_raw
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();

    let vague_words = [
        "stuff",
        "things",
        "somehow",
        "maybe",
        "whatever",
        "something",
        "anyway",
    ];
    let vague_count = vague_words.iter().filter(|&&w| words.contains(w)).count();

    if vague_count > 0 {
        score += 0.25 + (0.05 * vague_count as f32);
        gap_zones.push("vague_language_detected".into());
    }

    let word_count = raw.split_whitespace().count();
    if word_count < 10 {
        if let Some(rag) = derived_rag {
            if !rag.facts.is_empty() {
                // RAG facts supply sufficient context, reduce short prompt penalty
                score += 0.05;
            } else {
                score += 0.2;
                gap_zones.push("prompt_too_short".into());
            }
        } else {
            score += 0.2;
            gap_zones.push("prompt_too_short".into());
        }
    } else if word_count > 40 {
        score -= 0.15;
    }

    if repr.compression_ratio < 0.8 {
        score += 0.2;
        gap_zones.push("output_format_ambiguous".into());
    }

    if repr.intent_vec.is_empty() {
        if let Some(rag) = derived_rag {
            if rag.covered_gaps.contains("domain_context_missing") || !rag.facts.is_empty() {
                score += 0.05;
            } else {
                score += 0.3;
                gap_zones.push("domain_context_missing".into());
            }
        } else {
            score += 0.3;
            gap_zones.push("domain_context_missing".into());
        }
    } else {
        let max_intent = repr.intent_vec.iter().fold(0.0f32, |a, &b| a.max(b));
        let min_intent = repr.intent_vec.iter().fold(1.0f32, |a, &b| a.min(b));
        if max_intent - min_intent < 0.12 {
            score += 0.3;
            gap_zones.push("intent_unclear".into());
        }
    }

    // 2. Domain-specific required slot checks (Step 4)
    if let Some(prof) = profile {
        let rag_text = derived_rag
            .map(|r| r.facts.join(" ").to_lowercase())
            .unwrap_or_default();
        let combined = if rag_text.is_empty() {
            lower_raw.clone()
        } else {
            format!("{} {}", lower_raw, rag_text)
        };
        let combined_words: HashSet<&str> = combined
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|s| !s.is_empty())
            .collect();

        let mut domains_to_check = vec![prof.domain.as_str()];
        for sec in &prof.secondary_domains {
            if !domains_to_check.contains(&sec.as_str()) {
                domains_to_check.push(sec.as_str());
            }
        }

        let mut domain_slot_gaps = Vec::new();
        for dom in domains_to_check {
            detect_missing_domain_slots(dom, &combined, &combined_words, &mut domain_slot_gaps);
        }

        if !domain_slot_gaps.is_empty() {
            // Add up to 0.36 ambiguity score when critical domain slots are missing
            let slot_penalty = (domain_slot_gaps.len() as f32 * 0.12).min(0.36);
            score += slot_penalty;
            for g in domain_slot_gaps {
                if !gap_zones.contains(&g) {
                    gap_zones.push(g);
                }
            }
        }
    }

    score = score.max(0.0);

    Ok(AmbiguityAnalysis {
        score,
        threshold: 0.65, // Will be overridden by engine config
        triggered: score > 0.65,
        gap_zones,
    })
}

fn has_any_word_or_phrase(text: &str, words: &HashSet<&str>, terms: &[&str]) -> bool {
    terms.iter().any(|t| {
        if t.contains(' ') {
            text.contains(t)
        } else {
            words.contains(t)
        }
    })
}

fn has_number(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

fn detect_missing_domain_slots(
    domain: &str,
    text: &str,
    words: &HashSet<&str>,
    out_gaps: &mut Vec<String>,
) {
    match domain {
        "legal" => {
            let has_jurisdiction = has_any_word_or_phrase(
                text,
                words,
                &[
                    "india",
                    "indian",
                    "us",
                    "usa",
                    "uk",
                    "eu",
                    "europe",
                    "california",
                    "delaware",
                    "new york",
                    "texas",
                    "canada",
                    "australia",
                    "singapore",
                    "federal",
                    "state law",
                    "gdpr",
                    "hipaa",
                    "jurisdiction",
                ],
            );
            if !has_jurisdiction {
                out_gaps.push("missing_slot:legal:jurisdiction".into());
            }
            let has_party_or_doc = has_any_word_or_phrase(
                text,
                words,
                &[
                    "nda",
                    "lease",
                    "employment",
                    "vendor",
                    "saas",
                    "license",
                    "partnership",
                    "buyer",
                    "seller",
                    "landlord",
                    "tenant",
                    "employer",
                    "employee",
                    "client",
                    "contractor",
                ],
            );
            if !has_party_or_doc {
                out_gaps.push("missing_slot:legal:contract_or_party_role".into());
            }
        }
        "medical" | "health" => {
            let has_patient_profile = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "adult",
                        "child",
                        "pediatric",
                        "elderly",
                        "geriatric",
                        "male",
                        "female",
                        "pregnant",
                        "age",
                        "year-old",
                        "years old",
                    ],
                );
            if !has_patient_profile {
                out_gaps.push("missing_slot:medical:patient_age_or_profile".into());
            }
            let has_meds_or_duration = has_any_word_or_phrase(
                text,
                words,
                &[
                    "medication",
                    "medications",
                    "drug",
                    "prescription",
                    "dosage",
                    "mg",
                    "insulin",
                    "metformin",
                    "statin",
                    "acute",
                    "chronic",
                    "days",
                    "weeks",
                    "months",
                    "history",
                ],
            );
            if !has_meds_or_duration {
                out_gaps.push("missing_slot:medical:duration_or_medications".into());
            }
        }
        "nutrition" => {
            let has_cal_or_goal = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "cal",
                        "kcal",
                        "calorie",
                        "calories",
                        "deficit",
                        "surplus",
                        "weight loss",
                        "fat loss",
                        "muscle",
                        "bulk",
                        "maintenance",
                        "marathon",
                        "endurance",
                    ],
                );
            if !has_cal_or_goal {
                out_gaps.push("missing_slot:nutrition:calorie_or_goal_target".into());
            }
            let has_restrictions = has_any_word_or_phrase(
                text,
                words,
                &[
                    "vegan",
                    "vegetarian",
                    "veg",
                    "keto",
                    "paleo",
                    "gluten",
                    "dairy",
                    "lactose",
                    "nut",
                    "peanuts",
                    "shellfish",
                    "soy",
                    "allergy",
                    "allergies",
                    "intolerant",
                    "halal",
                    "kosher",
                    "no",
                    "without",
                    "avoid",
                ],
            );
            if !has_restrictions {
                out_gaps.push("missing_slot:nutrition:dietary_restrictions_or_allergies".into());
            }
        }
        "finance" => {
            let has_amount_or_horizon = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "year",
                        "years",
                        "month",
                        "months",
                        "retirement",
                        "long-term",
                        "short-term",
                        "million",
                        "millions",
                        "billion",
                        "usd",
                        "inr",
                        "dollars",
                    ],
                );
            if !has_amount_or_horizon {
                out_gaps.push("missing_slot:finance:capital_or_time_horizon".into());
            }
            let has_risk = has_any_word_or_phrase(
                text,
                words,
                &[
                    "risk",
                    "conservative",
                    "moderate",
                    "aggressive",
                    "safe",
                    "preservation",
                    "volatility",
                    "hedged",
                ],
            );
            if !has_risk {
                out_gaps.push("missing_slot:finance:risk_tolerance".into());
            }
        }
        "education" => {
            let has_level = has_any_word_or_phrase(
                text,
                words,
                &[
                    "beginner",
                    "intermediate",
                    "advanced",
                    "university",
                    "college",
                    "high school",
                    "k-12",
                    "undergraduate",
                    "graduate",
                    "phd",
                    "grade",
                    "novice",
                    "professional",
                ],
            );
            if !has_level {
                out_gaps.push("missing_slot:education:learner_level".into());
            }
            let has_duration = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "week", "weeks", "month", "months", "semester", "hour", "hours", "day",
                        "days", "module", "modules",
                    ],
                );
            if !has_duration {
                out_gaps.push("missing_slot:education:timeline_or_duration".into());
            }
        }
        "travel" => {
            let has_destination = has_any_word_or_phrase(
                text,
                words,
                &[
                    "goa",
                    "japan",
                    "tokyo",
                    "paris",
                    "europe",
                    "india",
                    "usa",
                    "uk",
                    "bali",
                    "thailand",
                    "dubai",
                    "singapore",
                    "italy",
                    "spain",
                    "swiss",
                    "london",
                    "new york",
                    "beach",
                    "mountains",
                    "city",
                    "island",
                ],
            );
            if !has_destination {
                out_gaps.push("missing_slot:travel:destination".into());
            }
            let has_duration_or_dates = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "day",
                        "days",
                        "week",
                        "weeks",
                        "weekend",
                        "month",
                        "january",
                        "february",
                        "march",
                        "april",
                        "may",
                        "june",
                        "july",
                        "august",
                        "september",
                        "october",
                        "november",
                        "december",
                        "summer",
                        "winter",
                        "spring",
                    ],
                );
            if !has_duration_or_dates {
                out_gaps.push("missing_slot:travel:dates_or_duration".into());
            }
            let has_budget_or_pax = has_any_word_or_phrase(
                text,
                words,
                &[
                    "budget",
                    "usd",
                    "inr",
                    "eur",
                    "dollars",
                    "rupees",
                    "cost",
                    "luxury",
                    "backpacking",
                    "cheap",
                    "solo",
                    "couple",
                    "family",
                    "kids",
                    "group",
                    "friends",
                ],
            );
            if !has_budget_or_pax {
                out_gaps.push("missing_slot:travel:budget_or_travelers".into());
            }
        }
        "software" | "software-engineering" | "devops" | "devops-infra" => {
            let has_stack_or_env = has_any_word_or_phrase(
                text,
                words,
                &[
                    "rust",
                    "python",
                    "go",
                    "golang",
                    "java",
                    "typescript",
                    "javascript",
                    "node",
                    "react",
                    "postgres",
                    "postgresql",
                    "sqlite",
                    "mysql",
                    "redis",
                    "kubernetes",
                    "k8s",
                    "docker",
                    "aws",
                    "gcp",
                    "azure",
                    "linux",
                    "axum",
                    "tokio",
                    "fastapi",
                    "django",
                    "spring",
                    "graphql",
                    "rest",
                    "grpc",
                    "terraform",
                ],
            );
            if !has_stack_or_env {
                out_gaps.push("missing_slot:software:stack_or_environment".into());
            }
        }
        "business" | "business-strategy" => {
            let has_market_or_stage = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "b2b",
                        "b2c",
                        "saas",
                        "enterprise",
                        "smb",
                        "consumer",
                        "seed",
                        "series",
                        "bootstrap",
                        "bootstrapped",
                        "pre-seed",
                        "mvp",
                        "pharma",
                        "biotech",
                        "fintech",
                        "healthcare",
                        "ecommerce",
                        "marketplace",
                        "retail",
                        "agency",
                        "subscription",
                    ],
                );
            if !has_market_or_stage {
                out_gaps.push("missing_slot:business:target_market_or_stage".into());
            }
        }
        "data-science" | "ai-ml" => {
            let has_dataset_or_metric = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "dataset",
                        "csv",
                        "parquet",
                        "sql",
                        "tabular",
                        "images",
                        "text",
                        "corpus",
                        "f1",
                        "auc",
                        "roc-auc",
                        "accuracy",
                        "precision",
                        "recall",
                        "rmse",
                        "mae",
                        "latency",
                        "perplexity",
                        "benchmark",
                        "pytorch",
                        "tensorflow",
                        "xgboost",
                        "transformer",
                        "llm",
                        "rag",
                    ],
                );
            if !has_dataset_or_metric {
                out_gaps.push("missing_slot:data-science:dataset_or_metric".into());
            }
        }
        "cybersecurity" => {
            let has_scope_or_asset = has_any_word_or_phrase(
                text,
                words,
                &[
                    "web",
                    "api",
                    "cloud",
                    "aws",
                    "kubernetes",
                    "network",
                    "endpoint",
                    "iam",
                    "oauth",
                    "jwt",
                    "tls",
                    "firewall",
                    "siem",
                    "soc",
                    "owasp",
                    "pentest",
                    "penetration",
                    "zero-trust",
                    "ransomware",
                    "phishing",
                    "database",
                ],
            );
            if !has_scope_or_asset {
                out_gaps.push("missing_slot:cybersecurity:target_scope_or_environment".into());
            }
        }
        "real-estate" => {
            let has_location_or_type = has_number(text)
                || has_any_word_or_phrase(
                    text,
                    words,
                    &[
                        "residential",
                        "commercial",
                        "multifamily",
                        "single-family",
                        "condo",
                        "apartment",
                        "house",
                        "rental",
                        "reit",
                        "mortgage",
                        "cap rate",
                        "cashflow",
                        "licensing",
                        "license",
                        "broker",
                        "brokerage",
                        "realtor",
                        "agent",
                        "india",
                        "us",
                        "usa",
                        "uk",
                        "texas",
                        "california",
                        "new york",
                        "florida",
                    ],
                );
            if !has_location_or_type {
                out_gaps.push("missing_slot:real-estate:location_or_property_type".into());
            }
        }
        _ => {}
    }
}
