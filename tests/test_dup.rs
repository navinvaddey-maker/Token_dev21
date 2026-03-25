use token_compress_engine::engine::pipeline::{self, PipelineInput};

#[tokio::test]
async fn test_duplication() {
    let input = PipelineInput {
        raw_text: "Here is a whole bunch of text about something extremely important. We need to save tokens.".into(),
        task: "Compress this".into(),
        use_case: "generic".into(),
        mode: "balanced".into(),
        max_tokens: 800,
        engine_version: "1.0.0".into(),
        protected_entities: vec![],
    };

    let result = pipeline::run(&input).unwrap();
    println!("DUMP:\n{}", result.optimized_prompt);
    
    let task_count = result.optimized_prompt.matches("Task: Compress this").count();
    assert_eq!(task_count, 1, "Should only have ONE Task section");
}
