use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::Arc;
use token_compress_engine::{
    pipeline::orchestrator::PipelineOrchestrator,
    session::SessionHistory,
    types::OrchestratorResponse,
    npae::aggressive::config::{UnifiedConfig, ConfigHandle},
};

#[derive(Deserialize)]
struct TestCases {
    test_cases: Vec<Category>,
}

#[derive(Deserialize)]
struct Category {
    category: String,
    prompts: Vec<String>,
}

#[derive(Serialize)]
struct TestResult {
    category: String,
    prompt: String,
    mode: String,
    tes: f32,
    sfs: f32,
    scs: f32,
    error: Option<String>,
    optimized_prompt: String,
    field_issues: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load test cases
    let mut file = File::open("/Users/navinvaddey/token_compress_engine/tests/automation_prompts.json")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let test_data: TestCases = serde_json::from_str(&contents)?;

    // 2. Initialize orchestrator
    let schema_priors = Arc::new(DashMap::new());
    let npae_config_data = token_compress_engine::npae::aggressive::config::ConfigLoader::load("/Users/navinvaddey/token_compress_engine/config/unified.json")
        .unwrap_or_else(|_| UnifiedConfig {
            domain_taxonomy: vec![],
            roles: vec![],
            constraints: vec![],
        });
    let npae_config = Arc::new(ConfigHandle::new(npae_config_data));
    let orchestrator = PipelineOrchestrator::build(schema_priors, npae_config);
    let mut session = SessionHistory::new(100);

    // 3. Run tests and collect results
    let mut results = Vec::new();

    for cat in test_data.test_cases {
        println!("Testing category: {}", cat.category);
        for prompt in cat.prompts {
            let display_prompt = if prompt.len() > 50 {
                match prompt.char_indices().nth(50) {
                    Some((idx, _)) => format!("{}...", &prompt[..idx]),
                    None => prompt.clone(),
                }
            } else {
                prompt.clone()
            };
            println!("  Prompt: {}", display_prompt);
            
            // Run in Balanced mode (default)
            match orchestrator.process(&prompt, &mut session, Some("balanced")) {
                Ok(OrchestratorResponse::Legacy(resp)) => {
                    results.push(TestResult {
                        category: cat.category.clone(),
                        prompt: prompt.clone(),
                        mode: "Balanced (Legacy)".to_string(),
                        tes: resp.scoring_result.tes,
                        sfs: resp.scoring_result.sfs,
                        scs: resp.scoring_result.scs,
                        error: None,
                        optimized_prompt: resp.response,
                        field_issues: resp.field_issues.len(),
                    });
                }
                Ok(OrchestratorResponse::Aggressive(resp)) => {
                    results.push(TestResult {
                        category: cat.category.clone(),
                        prompt: prompt.clone(),
                        mode: "Aggressive (Structured)".to_string(),
                        tes: 0.0, // Aggressive might not have these scores in the same way
                        sfs: 0.0,
                        scs: 0.0,
                        error: None,
                        optimized_prompt: resp.optimized_prompt,
                        field_issues: 0,
                    });
                }
                Err(e) => {
                    results.push(TestResult {
                        category: cat.category.clone(),
                        prompt: prompt.clone(),
                        mode: "Error".to_string(),
                        tes: 0.0,
                        sfs: 0.0,
                        scs: 0.0,
                        error: Some(e.to_string()),
                        optimized_prompt: "".to_string(),
                        field_issues: 0,
                    });
                }
            }
        }
    }

    // 4. Generate report
    generate_markdown_report(&results)?;

    println!("Test completed. Results written to automation_results.md");
    Ok(())
}

fn generate_markdown_report(results: &[TestResult]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create("/Users/navinvaddey/token_compress_engine/automation_results.md")?;
    
    writeln!(file, "# Automation Test Results")?;
    writeln!(file, "Generated at: {}\n", chrono::Local::now())?;
    
    writeln!(file, "## Summary")?;
    let total = results.len();
    let errors = results.iter().filter(|r| r.error.is_some()).count();
    let successes = total - errors;
    writeln!(file, "- Total tests: {}", total)?;
    writeln!(file, "- Successes: {}", successes)?;
    writeln!(file, "- Errors: {}\n", errors)?;

    writeln!(file, "## Detailed Results")?;
    
    let mut current_category = "";
    for r in results {
        if r.category != current_category {
            current_category = &r.category;
            writeln!(file, "### {}\n", current_category)?;
        }
        
        writeln!(file, "#### Prompt: `{}`", r.prompt)?;
        writeln!(file, "- **Mode**: {}", r.mode)?;
        if let Some(err) = &r.error {
            writeln!(file, "- **Status**: ❌ Error: {}", err)?;
        } else {
            writeln!(file, "- **Status**: ✅ Success")?;
            writeln!(file, "- **Scores**: TES: {:.1}, SFS: {:.1}, SCS: {:.1}", r.tes, r.sfs, r.scs)?;
            writeln!(file, "- **Field Issues**: {}", r.field_issues)?;
            writeln!(file, "- **Optimized Prompt Length**: {} chars", r.optimized_prompt.len())?;
            writeln!(file, "\n**Optimized Prompt:**\n```\n{}\n```", r.optimized_prompt)?;
            
            if r.tes < 6.0 || r.sfs < 6.0 {
                writeln!(file, "> [!WARNING]")?;
                writeln!(file, "> Low score detected. Developer should check for missing domain logic or field inference.")?;
            }
        }
        writeln!(file, "\n---\n")?;
    }

    Ok(())
}
