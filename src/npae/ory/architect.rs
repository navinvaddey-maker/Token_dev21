//! Ory Architect v2 — Adaptive Blueprint Generation
//!
//! Generates dynamic flow architectures using strategy selection,
//! domain-specific phase templates, dependency graphs, and resource estimation.

use crate::npae::ory::types::{
    DynamicBlueprint, BlueprintPhase, BlueprintStrategy, PhaseGraph,
    ResourceEstimate, ScopeClass, PhaseType, LearnedIntent, FlowAudit,
    AuditRecommendation,
};
use anyhow::Result;

pub struct OryArchitect;

impl OryArchitect {
    pub fn design(intent: &LearnedIntent, audit: &FlowAudit) -> Result<DynamicBlueprint> {
        let arch_id = format!("ORY-{}-{}",
            intent.inferred_domain.to_uppercase().replace(' ', "_"),
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        // 1. Select strategy based on audit
        let strategy = select_strategy(audit);

        // 2. Generate phases based on strategy + domain
        let phases = generate_phases(intent, audit, &strategy);

        // 3. Build dependency graph
        let phase_graph = build_phase_graph(&phases);

        // 4. Estimate resources
        let resource_estimate = estimate_resources(intent, &phases);

        // 5. Build validation checklist
        let validation_checklist = build_validation(intent);

        let rationale = format!(
            "Dynamic architecture for '{}' domain. Strategy: {}. Coverage: {:.0}%.",
            intent.inferred_domain,
            strategy_name(&strategy),
            audit.coverage_score * 100.0
        );

        let developer_notes = format!(
            "Complexity: {:.1}/10. Scope: {:?}. Phases: {}. Fingerprint: {}",
            intent.complexity_score,
            resource_estimate.scope_class,
            phases.len(),
            intent.intent_fingerprint
        );

        Ok(DynamicBlueprint {
            architecture_id: arch_id,
            strategy,
            rationale,
            phases,
            phase_graph,
            resource_estimate,
            validation_checklist,
            developer_notes,
            reuse_patterns: vec![],
        })
    }
}

fn strategy_name(s: &BlueprintStrategy) -> &'static str {
    match s {
        BlueprintStrategy::TemplateExtension { .. } => "TemplateExtension",
        BlueprintStrategy::DomainTransfer { .. } => "DomainTransfer",
        BlueprintStrategy::CompositeDesign { .. } => "CompositeDesign",
        BlueprintStrategy::NovelDesign { .. } => "NovelDesign",
    }
}

fn select_strategy(audit: &FlowAudit) -> BlueprintStrategy {
    match audit.recommendation {
        AuditRecommendation::AugmentExistingFlow => {
            let additions: Vec<String> = audit.gap_details.iter()
                .map(|g| g.description.clone())
                .collect();
            BlueprintStrategy::TemplateExtension {
                base_template: audit.existing_domain_match.clone().unwrap_or_default(),
                additions,
            }
        }
        AuditRecommendation::BuildDynamicFlow => {
            // Check if we can transfer from a related domain
            if audit.domain_audit.match_confidence > 0.3 {
                BlueprintStrategy::DomainTransfer {
                    source_domain: audit.domain_audit.matched_domain.clone().unwrap_or_default(),
                    adaptations: audit.gap_details.iter()
                        .map(|g| g.suggested_action.clone())
                        .collect(),
                }
            } else {
                BlueprintStrategy::NovelDesign {
                    reasoning: format!("No existing domain match (confidence: {:.2})",
                        audit.domain_audit.match_confidence),
                }
            }
        }
        AuditRecommendation::UseExistingFlow => {
            BlueprintStrategy::TemplateExtension {
                base_template: audit.existing_domain_match.clone().unwrap_or_default(),
                additions: vec![],
            }
        }
    }
}

fn generate_phases(
    intent: &LearnedIntent,
    _audit: &FlowAudit,
    _strategy: &BlueprintStrategy,
) -> Vec<BlueprintPhase> {
    let mut phases = Vec::new();
    let domain = &intent.inferred_domain;

    // Phase 1: Alignment (always present)
    phases.push(BlueprintPhase {
        name: "Conceptual Alignment".into(),
        description: format!("Align with core objective: {}", intent.core_objective),
        expected_deliverables: vec!["Intent Verification".into()],
        phase_type: PhaseType::Analysis,
        inputs: vec!["raw_prompt".into()],
        outputs: vec!["verified_intent".into()],
        estimated_complexity: 2.0,
    });

    // Domain-specific phases
    match domain.as_str() {
        "software-engineering" | "devops-infra" => {
            phases.push(bp("Architecture & Design", "System design and tech selection",
                vec!["Architecture document"], PhaseType::Design,
                vec!["verified_intent"], vec!["architecture_doc"], 5.0));
            phases.push(bp("Implementation", "Core development and testing",
                vec!["Working codebase", "Test suite"], PhaseType::Implementation,
                vec!["architecture_doc"], vec!["codebase"], 7.0));
            phases.push(bp("Deployment & Validation", "Deploy, test, monitor",
                vec!["Deployed service"], PhaseType::Validation,
                vec!["codebase"], vec!["deployed_service"], 4.0));
        }
        "business-strategy" | "finance" => {
            phases.push(bp("Market Research", "Competitor analysis and market sizing",
                vec!["Market analysis report"], PhaseType::Analysis,
                vec!["verified_intent"], vec!["market_data"], 4.0));
            phases.push(bp("Strategy Design", "Business model and revenue planning",
                vec!["Business model canvas"], PhaseType::Design,
                vec!["market_data"], vec!["strategy_doc"], 6.0));
            phases.push(bp("Execution Plan", "Go-to-market and milestone planning",
                vec!["Launch roadmap"], PhaseType::Implementation,
                vec!["strategy_doc"], vec!["execution_plan"], 5.0));
        }
        "ai-ml" | "data-science" => {
            phases.push(bp("Data Assessment", "Data quality, availability, and preprocessing",
                vec!["Data quality report"], PhaseType::Analysis,
                vec!["verified_intent"], vec!["data_assessment"], 4.0));
            phases.push(bp("Model Design", "Architecture selection and training plan",
                vec!["Model specification"], PhaseType::Design,
                vec!["data_assessment"], vec!["model_spec"], 6.0));
            phases.push(bp("Training & Evaluation", "Model training and benchmarking",
                vec!["Trained model", "Benchmark results"], PhaseType::Implementation,
                vec!["model_spec"], vec!["trained_model"], 8.0));
            phases.push(bp("Safety & Alignment", "Bias testing, safety guardrails",
                vec!["Safety report"], PhaseType::Validation,
                vec!["trained_model"], vec!["safe_model"], 5.0));
        }
        "medical" => {
            phases.push(bp("Evidence Review", "Literature and clinical evidence gathering",
                vec!["Evidence summary"], PhaseType::Analysis,
                vec!["verified_intent"], vec!["evidence_base"], 6.0));
            phases.push(bp("Protocol Design", "Clinical protocol and risk assessment",
                vec!["Clinical protocol"], PhaseType::Design,
                vec!["evidence_base"], vec!["protocol"], 7.0));
            phases.push(bp("Regulatory Check", "FDA/EMA compliance verification",
                vec!["Compliance report"], PhaseType::Validation,
                vec!["protocol"], vec!["compliant_protocol"], 5.0));
        }
        "education" => {
            phases.push(bp("Knowledge Mapping", "Core concepts and learning gaps",
                vec!["Concept map"], PhaseType::Analysis,
                vec!["verified_intent"], vec!["knowledge_map"], 3.0));
            phases.push(bp("Curriculum Design", "Learning path and methodology",
                vec!["Curriculum plan"], PhaseType::Design,
                vec!["knowledge_map"], vec!["curriculum"], 5.0));
            phases.push(bp("Assessment Design", "Mastery checks and feedback loops",
                vec!["Assessment framework"], PhaseType::Validation,
                vec!["curriculum"], vec!["assessment"], 4.0));
        }
        _ => {
            // Generic phases based on intent objective
            phases.push(bp("Discovery & Research", 
                format!("Investigate requirements for {}", intent.core_objective).as_str(),
                vec!["Research brief"], PhaseType::Analysis,
                vec!["verified_intent"], vec!["research"], 4.0));
            phases.push(bp("Design & Planning",
                format!("Design solution for {}", intent.core_objective).as_str(),
                vec!["Solution design"], PhaseType::Design,
                vec!["research"], vec!["design"], 5.0));
            phases.push(bp("Execution",
                format!("Implement solution for {}", intent.core_objective).as_str(),
                vec!["Deliverable"], PhaseType::Implementation,
                vec!["design"], vec!["output"], 6.0));
        }
    }

    // Dynamic phases for hidden dependencies
    for dep in &intent.hidden_dependencies {
        phases.push(BlueprintPhase {
            name: format!("{} Module", dep),
            description: format!("Address dependency: {}", dep),
            expected_deliverables: vec![format!("{} Protocol", dep)],
            phase_type: PhaseType::Custom(dep.clone()),
            inputs: vec!["verified_intent".into()],
            outputs: vec![format!("{}_output", dep.to_lowercase().replace(' ', "_"))],
            estimated_complexity: 3.0,
        });
    }

    // Novel signal phase
    if !intent.novel_signals.is_empty() {
        phases.push(BlueprintPhase {
            name: "Unconventional Adaptation".into(),
            description: format!("Address novel signals: {:?}", intent.novel_signals),
            expected_deliverables: vec!["Modified Execution Path".into()],
            phase_type: PhaseType::Custom("novel_adaptation".into()),
            inputs: vec!["verified_intent".into()],
            outputs: vec!["adapted_output".into()],
            estimated_complexity: 4.0,
        });
    }

    // Final synthesis (always)
    phases.push(BlueprintPhase {
        name: "Final Synthesis".into(),
        description: "Consolidate all modules into final output".into(),
        expected_deliverables: vec!["Optimized Output".into()],
        phase_type: PhaseType::Synthesis,
        inputs: phases.iter().flat_map(|p| p.outputs.clone()).collect(),
        outputs: vec!["final_output".into()],
        estimated_complexity: 3.0,
    });

    phases
}

fn bp(name: &str, desc: &str, deliverables: Vec<&str>, pt: PhaseType,
      inputs: Vec<&str>, outputs: Vec<&str>, complexity: f32) -> BlueprintPhase {
    BlueprintPhase {
        name: name.into(), description: desc.into(),
        expected_deliverables: deliverables.into_iter().map(|s| s.into()).collect(),
        phase_type: pt,
        inputs: inputs.into_iter().map(|s| s.into()).collect(),
        outputs: outputs.into_iter().map(|s| s.into()).collect(),
        estimated_complexity: complexity,
    }
}

fn build_phase_graph(phases: &[BlueprintPhase]) -> PhaseGraph {
    let mut edges = Vec::new();
    let mut parallel_groups: Vec<Vec<usize>> = Vec::new();

    // Build edges: phase j depends on phase i if j.inputs ∩ i.outputs ≠ ∅
    for j in 0..phases.len() {
        for i in 0..j {
            let has_dep = phases[j].inputs.iter()
                .any(|inp| phases[i].outputs.contains(inp));
            if has_dep {
                edges.push((i, j));
            }
        }
    }

    // Find parallel groups: phases with same prerequisites
    let mut visited = vec![false; phases.len()];
    for i in 0..phases.len() {
        if visited[i] { continue; }
        let my_preds: Vec<usize> = edges.iter()
            .filter(|(_, dep)| *dep == i)
            .map(|(pre, _)| *pre)
            .collect();
        let mut group = vec![i];
        for j in (i + 1)..phases.len() {
            if visited[j] { continue; }
            let j_preds: Vec<usize> = edges.iter()
                .filter(|(_, dep)| *dep == j)
                .map(|(pre, _)| *pre)
                .collect();
            if my_preds == j_preds { group.push(j); }
        }
        if group.len() > 1 {
            for &idx in &group { visited[idx] = true; }
            parallel_groups.push(group);
        }
    }

    PhaseGraph { edges, parallel_groups }
}

fn estimate_resources(intent: &LearnedIntent, phases: &[BlueprintPhase]) -> ResourceEstimate {
    let total_complexity: f32 = phases.iter().map(|p| p.estimated_complexity).sum();
    let avg = total_complexity / phases.len().max(1) as f32;

    let scope_class = if avg <= 2.0 { ScopeClass::Quick }
        else if avg <= 4.0 { ScopeClass::Standard }
        else if avg <= 7.0 { ScopeClass::Deep }
        else { ScopeClass::Research };

    let token_budget = match scope_class {
        ScopeClass::Quick => 500,
        ScopeClass::Standard => 1500,
        ScopeClass::Deep => 3000,
        ScopeClass::Research => 5000,
    };

    ResourceEstimate {
        complexity_score: intent.complexity_score,
        estimated_token_budget: token_budget,
        suggested_phases: phases.len() as u8,
        scope_class,
    }
}

fn build_validation(intent: &LearnedIntent) -> Vec<String> {
    let mut checklist = vec!["Does the output solve the core objective?".into()];
    for sig in &intent.novel_signals {
        checklist.push(format!("Verify signal: {}", sig));
    }
    for dep in &intent.hidden_dependencies {
        checklist.push(format!("Verify dependency: {}", dep));
    }
    for cp in &intent.constraint_phrases {
        checklist.push(format!("Verify constraint: {}", cp));
    }
    checklist
}
