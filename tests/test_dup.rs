use token_compress_engine::engine::pipeline::{self, PipelineInput};

#[tokio::test]
async fn test_duplication() {
    let input = PipelineInput {
        raw_text: "Actually, here is basically a very whole bunch of text about something which is quite extremely important. We just really need to save a large number of tokens because it is literally necessary to do so. In order to achieve this, we should perhaps explore and discuss the following options below.".into(),
        task: "Compress this".into(),
        deliverables: "".into(),
        constraints: "".into(),
        reproducibility: "".into(),
        model: "Claude".into(),
        use_case: "generic".into(),
        mode: "aggressive".into(),
        max_tokens: 800,
        engine_version: "1.0.0".into(),
        protected_entities: vec![],
    };

    let result = pipeline::run(&input).unwrap();
    println!("DUMP:\n{}", result.optimized_prompt);

    let task_count = result
        .optimized_prompt
        .matches("Task: Compress this")
        .count();
    assert_eq!(task_count, 1, "Should only have ONE Task section");
}
