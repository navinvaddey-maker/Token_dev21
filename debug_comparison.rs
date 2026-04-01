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
    pipeline::{
        orchestrator::PipelineOrchestrator, stage0_normalize::NormalizationPrePass,
        stage0b_topology::TopologyClassifier, stage1::Stage1, stage2::Stage2, stage3::Stage3,
        stage4::Stage4, stage6a::Stage6a,
    },
    scoring::{sfs::SemanticFidelityScorer, tes::TokenEfficiencyScorer},
    session::SessionHistory,
    types::{Mode, SlotSource, WmSlot},
};

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("=== INPUT ===");
    println!("{}", prompt);
    println!("Word count: {}", prompt.split_whitespace().count());
    println!();

    // Test just lexical compression
    println!("=== LEXICAL COMPRESSION ONLY ===");
    let lc = LexicalCompression::new();
    let mut lexical_output = AlgorithmOutput::default();
    let lexical_result = lc.compress(prompt, &mut lexical_output);

    println!("Tokens: {:?}", lexical_result.tokens);
    println!("Compression ratio: {}", lexical_result.ratio);
    println!("Input tokens: {}", lexical_result.input_tokens);
    println!("Output tokens: {}", lexical_result.output_tokens);
    if let Some(seq) = &lexical_output.ordinal_sequence {
        println!("Ordinal sequence: {:?}", seq.sequence);
        println!("Ordinal score: {}", seq.score);
    } else {
        println!("No ordinal sequence found");
    }
    println!();

    // Test full pipeline (if it compiles)
    println!("=== ATTEMPTING FULL PIPELINE ===");
    match test_full_pipeline(prompt) {
        Ok(Some(response)) => {
            println!("Pipeline succeeded!");
            println!("Response: {}", response.response);
            println!("Mode: {}", response.mode);
            // Add more pipeline output analysis as needed
        }
        Ok(None) => {
            println!("Pipeline returned None");
        }
        Err(e) => {
            println!("Pipeline failed: {}", e);
            println!("This indicates there are still issues in the pipeline workflow");
        }
    }
}

fn test_full_pipeline(
    prompt: &str,
) -> Result<
    Option<token_compress_engine::types::CompressionResponse>,
    Box<dyn std::std::error::Error>,
> {
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
        Ok(response) => Ok(Some(response)),
        Err(e) => {
            eprintln!("Pipeline error: {}", e);
            Ok(None)
        }
    }
}
