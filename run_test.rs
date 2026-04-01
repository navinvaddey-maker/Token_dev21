use token_compress_engine::engine::pipeline::{run, PipelineInput};

fn main() {
    let input = PipelineInput {
        raw_text: "Here is some content. It is very long. Let us summarize it.".to_string(),
        task: "summarize".to_string(),
        use_case: "generic".to_string(),
        mode: "balanced".to_string(),
        max_tokens: 1000,
        engine_version: "1.0".to_string(),
        protected_entities: vec![],
    };
    let output = run(&input).unwrap();
    println!(">>> OPTIMIZED PROMPT START <<<");
    println!("{}", output.optimized_prompt);
    println!(">>> OPTIMIZED PROMPT END <<<");
}
