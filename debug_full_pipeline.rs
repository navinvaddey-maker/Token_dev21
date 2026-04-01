use token_compress_engine::{
    algorithms::{
        field_validator::FieldTypeValidator,
        lexical::LexicalCompression,
        predictive_coding::{PredictiveCoding, SessionTurn},
        schema_filling::SchemaFilling,
        sparse_coding::SparseCoding,
        working_memory::{ContextFrame, WorkingMemory},
    },
    correction::cycle::CorrectionCycle,
    domain::AlgorithmOutput,
    pipeline::orchestrator::PipelineOrchestrator,
    pipeline::{
        stage0_normalize::NormalizationPrePass, stage0b_topology::TopologyClassifier,
        stage1::Stage1, stage2::Stage2, stage3::Stage3, stage4::Stage4, stage6a::Stage6a,
    },
    scoring::{sfs::SemanticFidelityScorer, tes::TokenEfficiencyScorer},
    session::SessionHistory,
    types::{Mode, SlotSource, WmSlot},
};

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("=== INPUT ===");
    println!("{}", prompt);
    println!();

    // Create mock dependencies for the pipeline
    let schema_priors = std::sync::Arc::new(token_compress_engine::dashmap::DashMap::new());
    let normalization_pre_pass =
        token_compress_engine::pipeline::stage0_normalize::NormalizationPrePass;
    let topology_classifier = token_compress_engine::pipeline::stage0b_topology::TopologyClassifier;
    let stage1 = token_compress_engine::pipeline::stage1::Stage1::new();
    let stage2 = token_compress_engine::pipeline::stage2::Stage2::new(
        token_compress_engine::algorithms::predictive_coding::PredictiveCoding::new(
            schema_priors.clone(),
        ),
    );
    let stage3 = token_compress_engine::pipeline::stage3::Stage3 {};
    let stage4 = token_compress_engine::pipeline::stage4::Stage4 {};
    let field_validator = token_compress_engine::algorithms::field_validator::FieldTypeValidator;
    let token_efficiency_scorer = token_compress_engine::scoring::tes::TokenEfficiencyScorer;
    let semantic_fidelity_scorer = token_compress_engine::scoring::sfs::SemanticFidelityScorer;
    let stage6a = token_compress_engine::pipeline::stage6a::Stage6a {};
    let correction_cycle = token_compress_engine::correction::cycle::CorrectionCycle::new(0);
    let session = token_compress_engine::session::SessionHistory::new(10);

    // Create orchestrator
    let orchestrator = token_compress_engine::pipeline::orchestrator::PipelineOrchestrator::new(
        normalization_pre_pass,
        topology_classifier,
        stage1,
        stage2,
        stage3,
        stage4,
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
