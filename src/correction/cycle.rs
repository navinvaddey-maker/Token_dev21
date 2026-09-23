use crate::types::{AlgorithmOutput, ScoreAxis, TextCorrection};

/// Maximum correction iterations per request (Stage 6B).
pub const MAX_CORRECTION_CYCLES: u32 = 2;

/// Applies a targeted pipeline adjustment for the failing score axis.
///
/// @param out - Mutable pipeline state.
/// @param axis - Which dimension failed the threshold.
/// @returns Human-readable correction steps applied.
pub fn apply_targeted_correction(out: &mut AlgorithmOutput, axis: &ScoreAxis) -> Vec<TextCorrection> {
    let mut corrections = Vec::new();

    match axis {
        ScoreAxis::SchemaFidelity => {
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
                corrected: "Merged constraints/deliverables and inferred implicit outputs".to_string(),
                confidence: 0.85,
                correction_type: "semantic_completeness".to_string(),
            });
        }
        ScoreAxis::TaskEssential => {
            out.force_compact_generation = true;
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
pub fn merge_reconstruction_into_schema(out: &mut AlgorithmOutput) {
    for lock in &out.constraint_locks {
        let name = lock.text.clone();
        if !out
            .resolved_schema
            .constraints
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            out.resolved_schema.constraints.push(crate::types::Constraint { name });
        }
    }

    for deliverable in &out.expected_deliverables {
        if !out
            .resolved_schema
            .output
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(deliverable))
        {
            out.resolved_schema
                .output
                .push(crate::types::Deliverable {
                    name: deliverable.clone(),
                });
        }
    }
}

/// Adds implicit deliverables for Balanced/Aggressive when constraints imply them.
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

    let meal_plan = task_text.contains("meal") || task_text.contains("plan") || task_text.contains("nutrition");
    let dietary_constraints = ["fiber", "dairy", "nightshade", "carb", "macro"]
        .iter()
        .any(|k| constraint_text.contains(k));

    if meal_plan && dietary_constraints {
        for extra in ["shopping_list", "substitution_guide"] {
            if !out.resolved_schema.output.iter().any(|d| d.name == extra) {
                out.resolved_schema.output.push(crate::types::Deliverable {
                    name: extra.to_string(),
                });
            }
        }
    }
}

