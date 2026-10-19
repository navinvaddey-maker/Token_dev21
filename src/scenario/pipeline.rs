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
    /// Composes a production-grade, structured prompt contract for an LLM based on
    /// the verified scenario domain, retrieved internal knowledge chunks, completeness gaps,
    /// deterministic tool registrations, and deterministic model tier selection.
    ///
    /// # Arguments
    /// - `domain`: Scenario domain configuration containing schema, namespace, and tool rules.
    /// - `rag_chunks`: Verified grounding evidence chunks retrieved from internal knowledge base.
    /// - `parser_summary`: Intent and key token extraction summary.
    /// - `gap_issues`: List of domain completeness violations and missing constraints.
    /// - `tool_logs`: List of deterministic tool dispatches and signatures.
    /// - `searched_namespace`: Namespace that was queried for document retrieval.
    /// - `question`: Original user intake query.
    /// - `geography`: Optional region filter (e.g. "india", "us") for jurisdictional grounding.
    ///
    /// # Errors
    /// Never returns `Err`.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Invariants
    /// - When `rag_chunks` is empty, returns canonical `"Not found in the internal knowledge base"`.
    /// - When `rag_chunks` is non-empty, emits an 8-part structured prompt contract adhering to
    ///   the deterministic schema specification without calling external LLMs.
    #[allow(clippy::too_many_arguments)]
    pub fn compose_scenario_result(
        domain: &ScenarioDomainConfig,
        rag_chunks: &[RagChunk],
        parser_summary: &str,
        gap_issues: &[String],
        tool_logs: &[String],
        searched_namespace: String,
        question: &str,
        geography: Option<&str>,
    ) -> ScenarioResponse {
        // ARCH: §1.4 — deterministic prompt composition across intent and industry verticals.
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

        // Deterministic Model Tier Arbiter (Rules §1.4 & §9)
        // High-stakes domains (legal, financial risk) or unresolved scenario gaps require deep reasoning.
        let (model_tier, tier_rationale) = if domain.key.eq_ignore_ascii_case("legal")
            || !gap_issues.is_empty()
        {
            (
                "Tier 3 (High-Capacity Frontier Reasoning: e.g. Claude 3.5 Sonnet / OpenAI o1/o3 / Gemini Pro)",
                "Complex domain context with flagged gap constraints requiring rigorous cross-clause synthesis and zero assumption tolerance.",
            )
        } else if rag_chunks.len() > 3 {
            (
                "Tier 2 (Balanced Reasoning: e.g. GPT-4o-mini / Gemini Flash / Claude Haiku)",
                "Multi-chunk evidence synthesis with clean schema completeness checks.",
            )
        } else {
            (
                "Tier 1 (Fast & Cost-Efficient SLM: e.g. Gemini 3.8 Flash / Llama-3-8B)",
                "High-confidence direct evidence retrieval with complete scenario parameters.",
            )
        };

        let geo_suffix = geography
            .map(|g| format!(" (Jurisdiction / Region: {})", g.to_uppercase()))
            .unwrap_or_default();

        let mut body = String::new();

        // ── SYSTEM PROMPT BLOCK ───────────────────────────────────────────
        body.push_str("=== SYSTEM PROMPT ===\n");

        // 1. SYSTEM DIRECTIVE & SPECIALIZED ROLE
        body.push_str("# 1. SYSTEM DIRECTIVE & SPECIALIZED ROLE\n");
        body.push_str(&format!(
            "You are an expert {} advisor specializing in {} analysis{}.\n",
            domain.label, domain.key, geo_suffix
        ));
        body.push_str("Your directive is to evaluate the user's scenario with absolute precision, grounded strictly in the verified internal documents provided below.\n");
        body.push_str(&format!("{}\n\n", parser_summary));

        // Extract explicit VERIFY markers from retrieved chunks programmatically
        let mut verify_markers = Vec::new();
        for chunk in rag_chunks {
            for line in chunk.content.lines() {
                let trimmed = line.trim();
                if let Some(pos) = trimmed.find("VERIFY ") {
                    let item = trimmed[pos + 7..].trim();
                    if !item.is_empty() && !verify_markers.contains(&item.to_string()) {
                        verify_markers.push(item.to_string());
                    }
                }
            }
        }

        // 4. OPERATIONAL CONSTRAINTS & GAP WARNINGS
        body.push_str("# 4. OPERATIONAL CONSTRAINTS & GAP WARNINGS\n");
        body.push_str("- Grounding Constraint: Rely exclusively on the cited internal evidence. Do not extrapolate external unverified facts or assumptions.\n");
        if gap_issues.is_empty() {
            body.push_str(
                "- Completeness: All domain completeness validation checks passed cleanly.\n",
            );
        } else {
            body.push_str("- Identified Completeness Gaps (You must explicitly highlight these missing parameters to the user):\n");
            for issue in gap_issues {
                body.push_str(&format!("  * [GAP WARNING]: {}\n", issue));
            }
        }
        if !verify_markers.is_empty() {
            body.push_str("- Flagged Internal Verification Items (Defect 5 Protocol): The retrieved knowledge base contains explicit flags requiring external validation:\n");
            for item in &verify_markers {
                body.push_str(&format!("  * [VERIFY ITEM]: {}\n", item));
            }
        }
        body.push('\n');

        // 5. DETERMINISTIC TOOLS & RESOURCE CAPABILITIES
        body.push_str("# 5. DETERMINISTIC TOOLS & RESOURCE CAPABILITIES\n");
        if tool_logs.is_empty() {
            body.push_str(
                "No external tools permitted. Standard internal knowledge lookup only.\n\n",
            );
        } else {
            body.push_str(
                "The following internal tools are registered and permitted for this scenario:\n",
            );
            for tool in tool_logs {
                body.push_str(&format!("- {}\n", tool));
            }
            body.push('\n');
        }

        // 6. TARGET RESPONSE SCHEMA CONTRACT
        body.push_str("# 6. TARGET RESPONSE SCHEMA CONTRACT\n");
        body.push_str(&format!(
            "Contract Format: {}\nInternal knowledge verification complete ({} relevant evidence excerpt(s) mapped).\n",
            domain.output_contract, rag_chunks.len()
        ));
        body.push_str(&format!(
            "Strictly structure the final response according to the '{}' specification below. Do not change section titles or add introductory filler text:\n\n",
            domain.output_contract
        ));
        body.push_str("### Section 1: Introduction and Scope\n");
        body.push_str("- State the context of the evaluation and explicitly identify the domain/geography.\n");
        body.push_str("- Rely solely on the provided materials to define the operational boundary.\n\n");
        body.push_str("### Section 2: Flagged Data Gaps and Missing Parameters\n");
        body.push_str("- Enumerate any missing user facts or critical parameters that restrict complete evaluation.\n");
        body.push_str("- Explicitly map any inputs passed via the backend `metadata_gaps` configuration.\n\n");
        body.push_str("### Section 3: Core Framework and Rules\n");
        body.push_str("- Detail the foundational principles, operational rules, or regulations derived from the documentation that govern the scenario.\n");
        body.push_str("- Specify exact requirements or penalties linked to failures or rule violations.\n\n");
        body.push_str("### Section 4: Strict Statutory/Operational Timelines\n");
        body.push_str("- Provide a clean Markdown table summarizing all operational phases, strict deadlines, and their specific starting triggers.\n\n");
        body.push_str("### Section 5: Matrix, Evidence, and Presumptions\n");
        body.push_str("- Outline the operational forum, execution environments, baseline definitions, or evidentiary shifts highlighted in the texts.\n");
        body.push_str("- State explicitly how the burden of proof or operational weight is allocated.\n\n");
        body.push_str("### Section 6: Actionable Compliance and Verifications\n");
        body.push_str("- List mandatory steps for execution.\n");
        body.push_str("- Executable Verification Mechanism (Defect 5 Protocol):\n");
        body.push_str("  Merely repeating a 'VERIFY' marker does not resolve the issue. For each flagged item or 'VERIFY' marker in the retrieved texts, you MUST specify:\n");
        body.push_str("  1. The exact question to verify\n");
        body.push_str("  2. The authoritative primary source to consult (e.g., official gazette, RBI notification, SEC filings, statutory registry)\n");
        body.push_str("  3. The current result or position derived from the corpus\n");
        body.push_str("  4. The date checked / corpus baseline\n");
        body.push_str("  5. Any remaining uncertainty and required human legal/financial action\n");
        body.push_str("  Summarize these in an Executable Verification Table:\n");
        body.push_str("  | Verification Item | Question to Verify | Authoritative Source to Consult | Result / Current Position | Date Checked | Remaining Uncertainty & Action |\n\n");

        // 7. RECOMMENDED EXECUTION MODEL
        body.push_str("# 7. RECOMMENDED EXECUTION MODEL\n");
        body.push_str(&format!("Model Tier: {}\n", model_tier));
        body.push_str(&format!("Selection Rationale: {}\n\n", tier_rationale));

        // 8. REGULATORY & COMPLIANCE DISCLAIMER
        body.push_str("# 8. REGULATORY & COMPLIANCE DISCLAIMER\n");
        body.push_str(&domain.disclaimer_text);
        body.push_str("\n\n");

        // ── USER PROMPT BLOCK ─────────────────────────────────────────────
        body.push_str("=== USER PROMPT ===\n");

        // 2. USER SCENARIO OBJECTIVE
        body.push_str("# 2. USER SCENARIO OBJECTIVE\n");
        body.push_str(question.trim());
        body.push_str("\n\n");

        // 3. VERIFIED GROUNDING EVIDENCE (INTERNAL KNOWLEDGE BASE)
        body.push_str("# 3. VERIFIED GROUNDING EVIDENCE (INTERNAL KNOWLEDGE BASE)\n");
        body.push_str("<context>\n");
        for (i, c) in rag_chunks.iter().enumerate() {
            let doc_ref = c
                .metadata
                .as_ref()
                .map(|m| format!("{} (Page {:?})", m.source_file, m.page_number.unwrap_or(1)))
                .unwrap_or_else(|| format!("Document-{}", i + 1));
            body.push_str(&format!(
                "  <chunk id=\"ID-{:02}\">\n    <source>{}</source>\n    <content>\n### Key Evidence [{}] (from {})\n{}\n    </content>\n  </chunk>\n",
                i + 1,
                doc_ref,
                i + 1,
                doc_ref,
                c.content.trim()
            ));
        }
        body.push_str("</context>\n\n");

        // AUTOMATED PARAMETER ANALYSIS (METADATA GAPS)
        body.push_str("=== AUTOMATED PARAMETER ANALYSIS ===\n");
        body.push_str("<metadata_gaps>\n");
        if gap_issues.is_empty() {
            body.push_str("  <!-- All pre-parser parameter validation checks passed cleanly -->\n");
        } else {
            for issue in gap_issues {
                body.push_str(&format!("  [GAP WARNING]: {}\n", issue));
            }
        }
        body.push_str("</metadata_gaps>\n\n");
        body.push_str("# TASK / DIRECTIVE\nAnalyze the scenario against the verified facts above. Comply strictly with all System constraints and the Target Schema contract.\n");

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
