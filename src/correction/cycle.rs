//! Targeted Correction Cycle (`ARCHITECTURE.md` §1.2, §1.3, §4).
//!
//! Adjusts pipeline parameters and schema fields in response to the lowest
//! failing score axis (`TaskEssential`, `SchemaFidelity`, `SemanticCompleteness`).

use crate::types::{AlgorithmOutput, ScoreAxis, TextCorrection};

/// Maximum correction iterations per request (Stage 6B).
pub const MAX_CORRECTION_CYCLES: u32 = 2;

/// Applies a targeted pipeline adjustment for the failing score axis.
///
/// # Panics
/// Never.
pub fn apply_targeted_correction(
    out: &mut AlgorithmOutput,
    axis: &ScoreAxis,
) -> Vec<TextCorrection> {
    let mut corrections = Vec::new();

    match axis {
        ScoreAxis::SchemaFidelity => {
            // ARCH: Balanced Mode Contract (A1, E4) — preserve constraints in schema without discarding compact state unnecessarily.
            out.force_compact_generation = false;
            merge_reconstruction_into_schema(out);
            corrections.push(TextCorrection {
                original: String::new(),
                corrected: "Re-ran schema fill with validation-driven merge".to_string(),
                confidence: 0.85,
                correction_type: "schema_fidelity".to_string(),
            });
        }
        ScoreAxis::SemanticCompleteness => {
            out.force_compact_generation = false;
            merge_reconstruction_into_schema(out);
            infer_implicit_deliverables(out);
            corrections.push(TextCorrection {
                original: String::new(),
                corrected: "Merged constraints/deliverables and inferred implicit outputs"
                    .to_string(),
                confidence: 0.85,
                correction_type: "semantic_completeness".to_string(),
            });
        }
        ScoreAxis::TaskEssential => {
            // ARCH: Balanced Mode Contract (A2) — even when switching to compact generation, merge
            // constraint locks into schema so Stage 6A compact renderer retains every constraint.
            out.force_compact_generation = true;
            merge_reconstruction_into_schema(out);
            corrections.push(TextCorrection {
                original: String::new(),
                corrected: "Switched to compact generation to reduce token expansion".to_string(),
                confidence: 0.8,
                correction_type: "task_essential".to_string(),
            });
        }
    }

    corrections
}

/// Ensures constraint locks and expected deliverables appear in the resolved schema.
///
/// # Panics
/// Never.
pub fn merge_reconstruction_into_schema(out: &mut AlgorithmOutput) {
    for lock in &out.constraint_locks {
        let name = lock.text.clone();
        if !out
            .resolved_schema
            .constraints
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            out.resolved_schema
                .constraints
                .push(crate::types::Constraint { name });
        }
    }

    for deliverable in &out.expected_deliverables {
        if !out
            .resolved_schema
            .output
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(deliverable))
        {
            out.resolved_schema.output.push(crate::types::Deliverable {
                name: deliverable.clone(),
            });
        }
    }
}

/// Adds implicit deliverables for Balanced/Aggressive when constraints or domain signals imply them.
///
/// # Panics
/// Never.
pub fn infer_implicit_deliverables(out: &mut AlgorithmOutput) {
    let mode = out.mode.as_ref();
    let allow_infer = matches!(
        mode,
        Some(crate::types::Mode::Balanced) | Some(crate::types::Mode::Aggressive)
    );
    if !allow_infer {
        return;
    }

    let constraint_text: String = out
        .resolved_schema
        .constraints
        .iter()
        .map(|c| c.name.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let task_text = out
        .resolved_schema
        .task
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let norm_text = out
        .normalization
        .as_ref()
        .map(|n| n.normalized_text.to_lowercase())
        .unwrap_or_default();
    let combined = format!("{task_text} {constraint_text} {norm_text}");
    let domain = out
        .scenario
        .as_ref()
        .map(|s| s.domain.as_str())
        .unwrap_or("");

    let push_unique = |list: &mut Vec<crate::types::Deliverable>, item: &str| {
        if !list.iter().any(|d| d.name.eq_ignore_ascii_case(item)) {
            list.push(crate::types::Deliverable {
                name: item.to_string(),
            });
        }
    };

    let meal_plan = domain == "nutrition"
        || combined.contains("meal")
        || combined.contains("nutrition")
        || (combined.contains("plan")
            && ["diet", "calorie", "protein", "macro"]
                .iter()
                .any(|k| combined.contains(k)));
    let dietary_constraints = [
        "fiber",
        "dairy",
        "nightshade",
        "carb",
        "macro",
        "calorie",
        "vegan",
        "keto",
    ]
    .iter()
    .any(|k| combined.contains(k));

    if meal_plan && dietary_constraints {
        for extra in ["shopping_list", "substitution_guide"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    }

    if out.resolved_schema.output.len() >= 2 {
        return;
    }

    if matches!(
        domain,
        "software" | "software-engineering" | "devops" | "devops-infra" | "tech" | "computers"
    ) || [
        "kubernetes",
        "microservice",
        "backend",
        "dockerfile",
        "pipeline",
        "rust",
        "python",
    ]
    .iter()
    .any(|k| combined.contains(k))
    {
        for extra in ["architecture_outline", "verification_steps"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if domain == "finance"
        || [
            "portfolio",
            "asset allocation",
            "cashflow",
            "retirement",
            "sharpe",
        ]
        .iter()
        .any(|k| combined.contains(k))
    {
        for extra in ["risk_breakdown", "allocation_summary"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if domain == "legal"
        || [
            "contract",
            "statute",
            "indemnification",
            "gdpr",
            "hipaa",
            "nda",
        ]
        .iter()
        .any(|k| combined.contains(k))
    {
        for extra in ["statutory_basis", "compliance_checklist"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if matches!(domain, "medical" | "health" | "pharma")
        || ["clinical", "patient", "diagnosis", "symptom", "biomarker"]
            .iter()
            .any(|k| combined.contains(k))
    {
        for extra in ["clinical_considerations", "safety_precautions"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if domain == "travel"
        || ["itinerary", "vacation", "sightseeing", "flight", "hotel"]
            .iter()
            .any(|k| combined.contains(k))
    {
        for extra in ["daily_itinerary", "logistics_summary"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if domain == "education"
        || ["curriculum", "syllabus", "pedagogy", "lesson", "student"]
            .iter()
            .any(|k| combined.contains(k))
    {
        for extra in ["learning_objectives", "practice_exercises"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if matches!(
        domain,
        "business" | "business-strategy" | "marketing" | "ecommerce"
    ) || ["go-to-market", "monetize", "campaign", "revenue", "startup"]
        .iter()
        .any(|k| combined.contains(k))
    {
        for extra in ["action_plan", "kpi_metrics"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    } else if domain == "real-estate"
        || ["property", "mortgage", "cap rate", "reit", "multifamily"]
            .iter()
            .any(|k| combined.contains(k))
    {
        for extra in ["market_comparables", "roi_projection"] {
            push_unique(&mut out.resolved_schema.output, extra);
        }
    }
}
