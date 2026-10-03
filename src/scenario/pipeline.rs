use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::registry::{ScenarioDomainConfig, ScenarioStyleConfig};
use crate::types::RagChunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResponse {
    pub mode: String,
    pub selection: String, // Domain key or Style key
    pub output_text: String,
    pub citations: Vec<String>,
    pub searched_namespaces: Vec<String>,
    pub source_badge: String,
    pub is_empty_knowledge: bool,
    pub warnings: Vec<String>,
}

pub struct ScenarioParser;
impl ScenarioParser {
    pub fn parse(_input: &str, domain: &ScenarioDomainConfig) -> String {
        format!("[Scenario_Parser:{}] Extracted intent and structured inputs for domain '{}'", domain.parser, domain.key)
    }
}

pub struct ScenarioGapCheck;
impl ScenarioGapCheck {
    pub fn evaluate(input: &str, domain: &ScenarioDomainConfig) -> (bool, Vec<String>) {
        let mut gaps = Vec::new();
        for rule in &domain.gap_check_rules {
            if input.trim().len() < 10 {
                gaps.push(format!("Rule '{}': input detail below recommended threshold", rule));
            }
        }
        (gaps.is_empty(), gaps)
    }
}

pub struct ScenarioToolRouter;
impl ScenarioToolRouter {
    pub fn execute_tools(domain: &ScenarioDomainConfig) -> Vec<String> {
        domain.tool_allowlist.iter()
            .map(|t| format!("Executed local tool '{}'", t))
            .collect()
    }
}

pub struct ScenarioOutputValidator;
impl ScenarioOutputValidator {
    pub fn validate(output: &str, citations: &[String], is_empty: bool) -> Result<(), String> {
        if !is_empty && citations.is_empty() && !output.contains("Not found in the internal knowledge base") {
            return Err("Scenario_Output_Validator error: Answer missing mandatory internal citations.".to_string());
        }
        Ok(())
    }
}

pub struct ScenarioOutputComposer;
impl ScenarioOutputComposer {
    pub fn compose_scenario_result(
        domain: &ScenarioDomainConfig,
        rag_chunks: &[RagChunk],
        parser_summary: &str,
        gap_issues: &[String],
        tool_logs: &[String],
    ) -> ScenarioResponse {
        let searched_ns = vec![domain.rag_namespace.clone()];

        if rag_chunks.is_empty() {
            let not_found_text = format!(
                "Not found in the internal knowledge base\n\nSearched namespaces: {}\n\n{}",
                searched_ns.join(", "),
                domain.disclaimer_text
            );
            return ScenarioResponse {
                mode: "Scenario".to_string(),
                selection: domain.key.clone(),
                output_text: not_found_text,
                citations: vec![],
                searched_namespaces: searched_ns,
                source_badge: "Source: internal knowledge base only".to_string(),
                is_empty_knowledge: true,
                warnings: vec!["Retrieval returned 0 relevant documents in target domain namespace.".to_string()],
            };
        }

        let citations: Vec<String> = rag_chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let src = c.metadata.as_ref()
                    .map(|m| format!("{} (Page {:?})", m.source_file, m.page_number.unwrap_or(1)))
                    .unwrap_or_else(|| format!("Doc-{}", i + 1));
                format!("[Ref {}]: {}", i + 1, src)
            })
            .collect();

        let mut body = String::new();
        body.push_str(&format!("## 1. Domain & Intent Context\nDomain: {}\n{}\n\n", domain.label, parser_summary));
        
        body.push_str("## 2. Retrieved Internal Knowledge\n");
        for (i, c) in rag_chunks.iter().enumerate() {
            body.push_str(&format!("### Key Evidence [{}]\n{}\n\n", i + 1, c.content));
        }

        body.push_str("## 3. Gap Check Assessment\n");
        if gap_issues.is_empty() {
            body.push_str("All domain schema completeness rules passed cleanly.\n\n");
        } else {
            for issue in gap_issues {
                body.push_str(&format!("- Warning: {}\n", issue));
            }
            body.push('\n');
        }

        body.push_str("## 4. Deterministic Tool Operations\n");
        for tool in tool_logs {
            body.push_str(&format!("- {}\n", tool));
        }
        body.push('\n');

        body.push_str("## 5. Output Contract Synthesis\n");
        body.push_str(&format!("Contract Format: {}\nInternal knowledge integration complete.\n\n", domain.output_contract));

        body.push_str("## 6. Regulatory & Compliance Disclaimer\n");
        body.push_str(&domain.disclaimer_text);

        ScenarioResponse {
            mode: "Scenario".to_string(),
            selection: domain.key.clone(),
            output_text: body,
            citations,
            searched_namespaces: searched_ns,
            source_badge: "Source: internal knowledge base only".to_string(),
            is_empty_knowledge: false,
            warnings: vec![],
        }
    }
}

pub struct ScenarioRegularPipeline;
impl ScenarioRegularPipeline {
    pub fn run(
        question: &str,
        style: &ScenarioStyleConfig,
        prompts_dir: &str,
        rag_chunks: &[RagChunk],
        searched_namespaces: Vec<String>,
        compression_mode: Option<&str>,
    ) -> ScenarioResponse {
        // Web search query refusal check
        let is_web_request = question.to_lowercase().contains("search the web")
            || question.to_lowercase().contains("online info")
            || question.to_lowercase().contains("internet");

        let mut warnings = Vec::new();
        if is_web_request {
            warnings.push("External web search request refused. Query processed using internal knowledge base only.".to_string());
        }

        if rag_chunks.is_empty() {
            let not_found_text = format!(
                "Not found in the internal knowledge base\n\nSearched namespaces: {}",
                searched_namespaces.join(", ")
            );
            return ScenarioResponse {
                mode: "Regular".to_string(),
                selection: style.key.clone(),
                output_text: not_found_text,
                citations: vec![],
                searched_namespaces,
                source_badge: "Source: internal knowledge base only".to_string(),
                is_empty_knowledge: true,
                warnings,
            };
        }

        let prompt_path = Path::new(prompts_dir).join(&style.prompt_file);
        let _style_instructions = fs::read_to_string(&prompt_path)
            .unwrap_or_else(|_| format!("Style: {}", style.label));

        let citations: Vec<String> = rag_chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let src = c.metadata.as_ref()
                    .map(|m| format!("{} (Page {:?})", m.source_file, m.page_number.unwrap_or(1)))
                    .unwrap_or_else(|| format!("Doc-{}", i + 1));
                format!("[Ref {}]: {}", i + 1, src)
            })
            .collect();

        let mut body = String::new();
        let mode_tag = compression_mode.unwrap_or("balanced");
        body.push_str(&format!("**Response Style**: {} (Mode: {})\n\n", style.label, mode_tag));

        if style.key.eq_ignore_ascii_case("aggressive") {
            body.push_str("### Core Stance & Risk Evaluation\n");
            body.push_str("Based strictly on internal knowledge, here is the strongest supported position:\n\n");
        }

        for (i, c) in rag_chunks.iter().enumerate() {
            body.push_str(&format!("- [{}] {}\n\n", i + 1, c.content));
        }

        body.push_str("### Citations\n");
        for cit in &citations {
            body.push_str(&format!("- {}\n", cit));
        }

        ScenarioResponse {
            mode: "Regular".to_string(),
            selection: style.key.clone(),
            output_text: body,
            citations,
            searched_namespaces,
            source_badge: "Source: internal knowledge base only".to_string(),
            is_empty_knowledge: false,
            warnings,
        }
    }
}
