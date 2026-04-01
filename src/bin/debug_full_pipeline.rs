use dashmap::DashMap;
use std::sync::Arc;
use token_compress_engine::{
    algorithms::{
        field_validator::FieldTypeValidator,
        lexical::LexicalCompression,
        predictive_coding::{PredictiveCoding, SessionTurn},
        schema_filling::SchemaFilling,
        sparse_coding::SparseCoding,
        working_memory::{ContextFrame, WorkingMemory},
    },
    correction::cycle, // for CorrectionCycle impl
    pipeline::{
        orchestrator::PipelineOrchestrator, stage0_normalize::NormalizationPrePass,
        stage0b_topology::TopologyClassifier, stage1::Stage1, stage2::Stage2, stage3::Stage3,
        stage4::Stage4, stage5::Stage5, stage6a::Stage6a,
    },
    scoring::{sfs::SemanticFidelityScorer, tes::TokenEfficiencyScorer},
    session::SessionHistory,
    types::{AlgorithmOutput, CorrectionCycle, Mode, SlotSource, WmSlot},
};

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("=== INPUT ===");
    println!("{}", prompt);
    println!();

    // Create mock dependencies for the pipeline
    let normalization_pre_pass = NormalizationPrePass;
    let topology_classifier = TopologyClassifier;
    let stage1 = Stage1::new();
    let stage2 = Stage2::new(PredictiveCoding::new(Arc::new(DashMap::new())));
    let stage3 = Stage3::new();
    let stage4 = Stage4::new();
    let field_validator = FieldTypeValidator::new();
    let token_efficiency_scorer = TokenEfficiencyScorer::new();
    let semantic_fidelity_scorer = SemanticFidelityScorer::new();
    let stage5 = Stage5::new();
    let stage6a = Stage6a::new();
    let correction_cycle = CorrectionCycle::new(0);
    let session = SessionHistory::new(10);

    // Create orchestrator
    let orchestrator = PipelineOrchestrator::new(
        normalization_pre_pass,
        topology_classifier,
        stage1,
        stage2,
        stage3,
        stage4,
        stage5,
        field_validator,
        token_efficiency_scorer,
        semantic_fidelity_scorer,
        stage6a,
        correction_cycle,
    );

    // Process through full pipeline
    match orchestrator.process(&prompt, &session) {
        Ok(response) => {
            println!("=== FULL PIPELINE OUTPUT ===");
            println!("Mode: {}", response.mode);
            println!("Error score: {}", response.error_score);
            println!("Fidelity: {}", response.fidelity);
            println!("Task: {:?}", response.task);
            println!("Deliverable: {:?}", response.deliverable);
            println!("Context: {:?}", response.context);
            println!("Constraints: {:?}", response.constraints);
            println!("WM slots used: {}", response.wm_slots_used);
            println!("Null fields: {:?}", response.null_fields);
            println!("Response: {}", response.response);
            println!("Normalization: {:?}", response.normalization);
            println!("Field issues: {:?}", response.field_issues);
            println!("Topology: {:?}", response.topology);
            println!("Dual score: {:?}", response.dual_score);
            println!("Correction cycle: {:?}", response.correction_cycle);
        }
        Err(e) => {
            println!("Error processing pipeline: {}", e);
        }
    }
}
