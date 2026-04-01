use token_compress_engine::engine::pipeline::{run, PipelineInput};

fn main() {
    let input = PipelineInput {
        raw_text: "test content".to_string(),
        task: "summarize".to_string(),
        use_case: "generic".to_string(),
        mode: "balanced".to_string(),
        max_tokens: 1000,
        engine_version: "1.0".to_string(),
        protected_entities: vec![],
    };
    let output = run(&input).unwrap();
    println!("{}", output.optimized_prompt);
}
