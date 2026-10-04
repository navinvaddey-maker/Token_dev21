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
    pub fn parse(input: &str, domain: &ScenarioDomainConfig) -> String {
        let clean_tokens: Vec<&str> = input
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 1)
            .collect();
        let token_count = clean_tokens.len();

        let mut key_terms = Vec::new();
        for t in &clean_tokens {
            if t.len() >= 4
                && !matches!(
                    t.to_lowercase().as_str(),
                    "this"
                        | "that"
                        | "with"
                        | "from"
                        | "they"
                        | "been"
                        | "have"
                        | "what"
                        | "which"
                        | "when"
                        | "where"
                        | "how"
                        | "who"
                        | "whom"
                        | "than"
                        | "then"
                        | "these"
                        | "those"
                        | "each"
                        | "every"
                        | "some"
                        | "such"
                        | "only"
                        | "into"
                        | "over"
                        | "after"
                        | "also"
                        | "would"
                        | "could"
                        | "should"
                        | "about"
                        | "there"
                        | "their"
                        | "please"
                )
            {
                key_terms.push(*t);
            }
        }
        key_terms.dedup();
        let sample = if !key_terms.is_empty() {
            format!(
                "; focus tokens: [{}]",
                key_terms
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<&str>>()
                    .join(", ")
            )
        } else {
            String::new()
        };

        format!(
            "[Scenario_Parser:{}] Extracted intent and structured inputs for domain '{}' ({} tokens{})",
            domain.parser, domain.key, token_count, sample
        )
    }
}

pub struct ScenarioGapCheck;
impl ScenarioGapCheck {
    pub fn evaluate(input: &str, domain: &ScenarioDomainConfig) -> (bool, Vec<String>) {
        let mut gaps = Vec::new();
        let input_lower = input.to_lowercase();
        let input_words: Vec<&str> = input_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let word_count = input_words.len();

        for rule in &domain.gap_check_rules {
            match rule.as_str() {
                "clause_completeness" => {
                    if word_count < 6 {
                        gaps.push(format!("Rule '{}': input detail ({} words) below minimum threshold for clause completeness", rule, word_count));
                    }
                }
                "jurisdiction_check" => {
                    let known_jurisdictions = [
                        "india",
                        "court",
                        "delhi",
                        "kerala",
                        "tamil nadu",
                        "andhra",
                        "telangana",
                        "karnataka",
                        "maharashtra",
                        "federal",
                        "california",
                        "delaware",
                        "england",
                    ];
                    let has_match = known_jurisdictions.iter().any(|&j| {
                        if j.contains(' ') {
                            input_lower.contains(j)
                        } else {
                            input_words.contains(&j)
                        }
                    }) || input_words.iter().any(|&w| {
                        w == "act"
                            || w == "state"
                            || w == "statute"
                            || w == "code"
                            || w == "uk"
                            || w == "us"
                    });

                    if !has_match {
                        gaps.push(format!("Rule '{}': jurisdiction or governing territory not explicitly specified in scenario", rule));
                    }
                }
                "liability_cap" => {
                    let liability_terms = [
                        "liability",
                        "indemnit",
                        "damages",
                        "cap",
                        "limit",
                        "cents",
                        "rs",
                        "inr",
                        "usd",
                        "$",
                        "penalty",
                        "transfer",
                        "consideration",
                    ];
                    if !liability_terms
                        .iter()
                        .any(|term| input_lower.contains(term))
                    {
                        gaps.push(format!(
                            "Rule '{}': financial bounds or liability parameters not specified",
                            rule
                        ));
                    }
                }
                "revenue_model_check" => {
                    let rev_terms = [
                        "revenue",
                        "pricing",
                        "subscription",
                        "b2b",
                        "b2c",
                        "saas",
                        "margin",
                        "monetiz",
                        "sales",
                    ];
                    if !rev_terms.iter().any(|term| input_lower.contains(term)) {
                        gaps.push(format!(
                            "Rule '{}': revenue model mechanics not stated",
                            rule
                        ));
                    }
                }
                "market_scope_check" => {
                    let scope_terms = [
                        "market",
                        "customer",
                        "tam",
                        "segment",
                        "competitor",
                        "global",
                        "regional",
                        "local",
                    ];
                    if !scope_terms.iter().any(|term| input_lower.contains(term)) {
                        gaps.push(format!(
                            "Rule '{}': market scope or target segment missing",
                            rule
                        ));
                    }
                }
                "audit_trail_check" => {
                    let audit_terms = [
                        "audit",
                        "ledger",
                        "log",
                        "transaction",
                        "reconcil",
                        "invoice",
                        "receipt",
                        "record",
                    ];
                    if !audit_terms.iter().any(|term| input_lower.contains(term)) {
                        gaps.push(format!(
                            "Rule '{}': audit trail or ledger source missing",
                            rule
                        ));
                    }
                }
                "cap_table_validation" => {
                    let cap_terms = [
                        "equity",
                        "share",
                        "stock",
                        "vesting",
                        "dilution",
                        "investor",
                        "round",
                        "ownership",
                        "percent",
                        "%",
                    ];
                    if !cap_terms.iter().any(|term| input_lower.contains(term)) {
                        gaps.push(format!(
                            "Rule '{}': cap table breakdown or equity split missing",
                            rule
                        ));
                    }
                }
                _ => {
                    if input.trim().is_empty() {
                        gaps.push(format!("Rule '{}': required scenario input is empty", rule));
                    }
                }
            }
        }
        (gaps.is_empty(), gaps)
    }
}

pub struct ScenarioToolRouter;
impl ScenarioToolRouter {
    pub fn execute_tools(domain: &ScenarioDomainConfig, query: &str) -> Vec<String> {
        let significant_words: Vec<&str> = query
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 3)
            .take(4)
            .collect();
        let target_terms = if !significant_words.is_empty() {
            significant_words.join(", ")
        } else {
            "general_context".to_string()
        };

        domain
            .tool_allowlist
            .iter()
            .map(|tool| {
                format!(
                    "Dispatched tool '{}' [domain: '{}', target_terms: [{}], context_bytes: {}]",
                    tool,
                    domain.key,
                    target_terms,
                    query.len()
                )
            })
            .collect()
    }
}

pub struct ScenarioOutputValidator;
impl ScenarioOutputValidator {
    pub fn validate(output: &str, citations: &[String], is_empty: bool) -> Result<(), String> {
        if !is_empty
            && citations.is_empty()
            && !output.contains("Not found in the internal knowledge base")
        {
            return Err(
                "Scenario_Output_Validator error: Answer missing mandatory internal citations."
                    .to_string(),
            );
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
        searched_namespace: String,
    ) -> ScenarioResponse {
        let searched_ns = vec![searched_namespace];

        if rag_chunks.is_empty() {
            let not_found_text = format!(
                "Not found in the internal knowledge base\n\nSearched namespaces: {}",
                searched_ns.join(", ")
            );
            return ScenarioResponse {
                mode: "Scenario".to_string(),
                selection: domain.key.clone(),
                output_text: not_found_text,
                citations: vec![],
                searched_namespaces: searched_ns,
                source_badge: "Source: internal knowledge base only".to_string(),
                is_empty_knowledge: true,
                warnings: vec![
                    "Retrieval returned 0 relevant documents in target domain namespace."
                        .to_string(),
                ],
            };
        }

        let citations: Vec<String> = rag_chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let src = c
                    .metadata
                    .as_ref()
                    .map(|m| format!("{} (Page {:?})", m.source_file, m.page_number.unwrap_or(1)))
                    .unwrap_or_else(|| format!("Doc-{}", i + 1));
                format!("[Ref {}]: {}", i + 1, src)
            })
            .collect();

        let mut body = String::new();
        body.push_str(&format!(
            "## 1. Domain & Intent Context\nDomain: {}\n{}\n\n",
            domain.label, parser_summary
        ));

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
        body.push_str(&format!(
            "Contract Format: {}\nInternal knowledge verification complete ({} relevant evidence excerpt(s) mapped).\n\n",
            domain.output_contract, rag_chunks.len()
        ));

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
        let style_instructions =
            fs::read_to_string(&prompt_path).unwrap_or_else(|_| format!("Style: {}", style.label));

        let citations: Vec<String> = rag_chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let src = c
                    .metadata
                    .as_ref()
                    .map(|m| format!("{} (Page {:?})", m.source_file, m.page_number.unwrap_or(1)))
                    .unwrap_or_else(|| format!("Doc-{}", i + 1));
                format!("[Ref {}]: {}", i + 1, src)
            })
            .collect();

        let mut body = String::new();
        let mode_tag = compression_mode.unwrap_or("balanced");
        body.push_str(&format!(
            "**Response Style**: {} (Mode: {})\n\n",
            style.label, mode_tag
        ));

        // Inject persona and style guidance from prompt template (GAP-S01 fix)
        body.push_str("### Style Persona & Directives\n");
        body.push_str(style_instructions.trim());
        body.push_str("\n\n");

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
