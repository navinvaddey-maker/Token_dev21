use std::sync::Arc;
use tracing::info;

use crate::rag::retriever::DocumentRetriever;
use crate::rag::types::RetrievalQuery;

use super::guards::{ScenarioEgressGuard, ScenarioNamespaceGuard};
use super::pipeline::{
    ScenarioGapCheck, ScenarioOutputComposer, ScenarioOutputValidator, ScenarioParser,
    ScenarioRegularPipeline, ScenarioResponse, ScenarioToolRouter,
};
use super::registry::{ScenarioDomainRegistry, ScenarioStyleRegistry};

pub struct ScenarioModeRouter {
    pub domain_registry: Arc<ScenarioDomainRegistry>,
    pub style_registry: Arc<ScenarioStyleRegistry>,
    pub egress_guard: Arc<ScenarioEgressGuard>,
    pub prompts_dir: String,
}

impl ScenarioModeRouter {
    pub fn new(
        domain_registry: Arc<ScenarioDomainRegistry>,
        style_registry: Arc<ScenarioStyleRegistry>,
        egress_guard: Arc<ScenarioEgressGuard>,
        prompts_dir: String,
    ) -> Self {
        Self {
            domain_registry,
            style_registry,
            egress_guard,
            prompts_dir,
        }
    }

    // Router entrypoint accepts distinct scenario parameters, context flags, and orchestrator hook
    #[allow(clippy::too_many_arguments)]
    pub async fn route_and_execute(
        &self,
        app_mode: &str,
        domain_key: Option<&str>,
        style_key: Option<&str>,
        geography: Option<&str>,
        question: &str,
        user_id: &str,
        user_business_type: &str,
        compression_mode: Option<&str>,
        retriever: &DocumentRetriever,
        orchestrator: Option<&crate::pipeline::orchestrator::PipelineOrchestrator>,
    ) -> Result<ScenarioResponse, String> {
        info!("ScenarioModeRouter processing request: mode='{}', domain={:?}, style={:?}, geography={:?}", app_mode, domain_key, style_key, geography);

        if question.trim().is_empty() {
            return Err("Validation Error: Question text cannot be empty.".to_string());
        }

        match app_mode.trim().to_lowercase().as_str() {
            "scenario" => {
                let d_key = domain_key.ok_or_else(|| {
                    "Validation Error: Domain selection is mandatory in Scenario mode.".to_string()
                })?;
                if style_key.is_some() && !style_key.unwrap().is_empty() {
                    return Err(
                        "Validation Error: Style selection is invalid when Mode = Scenario."
                            .to_string(),
                    );
                }

                let domain_cfg = self
                    .domain_registry
                    .get_domain(d_key)
                    .ok_or_else(|| format!("Validation Error: Unknown domain '{}'.", d_key))?;

                // Check egress tool allowlist
                for tool in &domain_cfg.tool_allowlist {
                    self.egress_guard.validate_tool_registration(tool, None)?;
                }

                let mut base_namespace = domain_cfg.rag_namespace.clone();
                if let Some(geo) = geography {
                    base_namespace = format!("{}_{}", base_namespace, geo.to_lowercase());
                }

                let mut permitted_ns = ScenarioNamespaceGuard::get_permitted_namespaces(
                    "scenario",
                    Some(&base_namespace),
                    user_business_type,
                );
                // FIX: D6 — accept documents whose ingested `domain` is one of this scenario
                // domain's declared aliases (e.g. `legal` for `legal_docs`). Exact-match only, so
                // other domains (e.g. a `legal` doc in a Finance scenario) stay isolated.
                // ARCH: §1.4 — scenario domains isolate retrieval by namespace.
                permitted_ns.extend(domain_cfg.document_domain_aliases.iter().cloned());

                // RAG Retrieval restricted to domain namespace with pre-truncation filtering (D2)
                let min_sim = domain_cfg.min_similarity.unwrap_or(0.40);
                let query = RetrievalQuery {
                    query_text: question.to_string(),
                    user_id: user_id.to_string(),
                    document_ids: None,
                    top_k: 5,
                    min_similarity: min_sim,
                };

                let filter_fn = |res: &crate::rag::types::SearchResult| {
                    ScenarioNamespaceGuard::is_chunk_permitted(
                        res.document_domain.as_deref(),
                        Some(&res.document_filename),
                        &permitted_ns,
                    ) && ScenarioNamespaceGuard::is_chunk_content_relevant(
                        question,
                        &res.chunk.content,
                    )
                };

                // FIX: D3 — propagate retrieval errors rather than masking as empty knowledge
                let retrieved = retriever
                    .retrieve_filtered(&query, None, &filter_fn)
                    .await
                    .map_err(|e| format!("Retrieval Error: {}", e))?;

                let filtered_chunks: Vec<crate::types::RagChunk> = retrieved
                    .into_iter()
                    .map(|res| crate::types::RagChunk {
                        domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
                        content: res.chunk.content,
                        metadata: Some(crate::types::ChunkMetadata {
                            page_number: res
                                .chunk
                                .metadata
                                .as_ref()
                                .and_then(|m| m.page_number)
                                .map(|p| p as u32),
                            source_file: res.document_filename,
                        }),
                    })
                    .collect();

                let parser_summary = ScenarioParser::parse(question, domain_cfg);
                let (_ok, gap_issues) = ScenarioGapCheck::evaluate(question, domain_cfg);
                let tool_logs = ScenarioToolRouter::execute_tools(domain_cfg, question);

                let mut resp = ScenarioOutputComposer::compose_scenario_result(
                    domain_cfg,
                    &filtered_chunks,
                    &parser_summary,
                    &gap_issues,
                    &tool_logs,
                    base_namespace.clone(),
                    question,
                    geography,
                );

                // GAP-S02 / GAP-42 Fix: In Aggressive mode, synthesize grounded evidence and strip raw chunk wrappers
                if let (Some(cm), Some(orch)) = (compression_mode, orchestrator) {
                    if !resp.is_empty_knowledge {
                        let mut session = crate::session::SessionHistory::new(5);
                        let enrichment = crate::types::EnrichmentContext {
                            cluster_vocab: None,
                            rag_chunks: Some(filtered_chunks.clone()),
                        };
                        if let Ok(comp_res) = orch
                            .process(
                                question,
                                &mut session,
                                None,
                                Some(cm),
                                None,
                                Some(enrichment),
                            )
                            .await
                        {
                            let compressed_prompt = match comp_res {
                                crate::types::OrchestratorResponse::Aggressive(r) => {
                                    r.optimized_prompt
                                }
                                crate::types::OrchestratorResponse::Legacy(r) => r.response,
                            };

                            if cm.eq_ignore_ascii_case("aggressive") {
                                // ARCH: §12.3 & §4 — Extract clean system prompt contract and combine with condensed aggressive user prompt
                                if let Some(sys_start) =
                                    resp.output_text.find("=== SYSTEM PROMPT ===")
                                {
                                    if let Some(user_start) =
                                        resp.output_text.find("=== USER PROMPT ===")
                                    {
                                        let system_part =
                                            resp.output_text[sys_start..user_start].trim_end();

                                        // Extract derived facts cleanly
                                        let derived_rag = crate::npae::aggressive::rag_context::DerivedRagContext::derive(
                                            &filtered_chunks,
                                            question,
                                            250,
                                        );

                                        let mut fact_chunks_xml = String::new();
                                        fact_chunks_xml.push_str("<context>\n");
                                        if !derived_rag.facts.is_empty() {
                                            for (idx, fact) in derived_rag.facts.iter().enumerate()
                                            {
                                                let src = filtered_chunks
                                                    .get(idx)
                                                    .and_then(|c| c.metadata.as_ref())
                                                    .map(|m| m.source_file.clone())
                                                    .unwrap_or_else(|| domain_cfg.label.clone());
                                                fact_chunks_xml.push_str(&format!(
                                                    "  <chunk id=\"ID-{:02}\">\n    <source>{}</source>\n    <content>\n{}. {}\n    </content>\n  </chunk>\n",
                                                    idx + 1,
                                                    src,
                                                    idx + 1,
                                                    fact.trim()
                                                ));
                                            }
                                        } else {
                                            for (idx, c) in filtered_chunks.iter().enumerate() {
                                                let src = c
                                                    .metadata
                                                    .as_ref()
                                                    .map(|m| m.source_file.clone())
                                                    .unwrap_or_else(|| domain_cfg.label.clone());
                                                fact_chunks_xml.push_str(&format!(
                                                    "  <chunk id=\"ID-{:02}\">\n    <source>{}</source>\n    <content>\n{}. {}\n    </content>\n  </chunk>\n",
                                                    idx + 1,
                                                    src,
                                                    idx + 1,
                                                    c.content.trim()
                                                ));
                                            }
                                        }
                                        fact_chunks_xml.push_str("</context>");

                                        let mut gaps_xml = String::new();
                                        gaps_xml.push_str("=== AUTOMATED PARAMETER ANALYSIS ===\n<metadata_gaps>\n");
                                        if gap_issues.is_empty() {
                                            gaps_xml.push_str("  <!-- All pre-parser parameter validation checks passed cleanly -->\n");
                                        } else {
                                            for issue in &gap_issues {
                                                gaps_xml.push_str(&format!("  [GAP WARNING]: {}\n", issue));
                                            }
                                        }
                                        gaps_xml.push_str("</metadata_gaps>\n");

                                        let clean_aggressive = format!(
                                            "{}\n\n=== USER PROMPT ===\n# 2. USER SCENARIO OBJECTIVE\n{}\n\n# 3. VERIFIED GROUNDING CONTEXT (COMPRESSED FACT MATRIX)\n{}\n\n{}\n# TASK / DIRECTIVE\nAnalyze the scenario against the verified facts above. Comply strictly with all System constraints and the Target Schema contract.\n",
                                            system_part,
                                            question.trim(),
                                            fact_chunks_xml.trim(),
                                            gaps_xml.trim()
                                        );
                                        resp.output_text = clean_aggressive;
                                    } else {
                                        resp.output_text
                                            .push_str("\n\n=== COMPRESSED USER PROMPT ===\n");
                                        resp.output_text.push_str(&compressed_prompt);
                                    }
                                } else {
                                    resp.output_text
                                        .push_str("\n\n=== COMPRESSED USER PROMPT ===\n");
                                    resp.output_text.push_str(&compressed_prompt);
                                }
                            } else {
                                resp.output_text
                                    .push_str("\n\n## 7. Compressed Execution Prompt\n");
                                resp.output_text.push_str(&compressed_prompt);
                            }
                        }
                    }
                }

                ScenarioOutputValidator::validate(
                    &resp.output_text,
                    &resp.citations,
                    resp.is_empty_knowledge,
                )?;
                Ok(resp)
            }
            "regular" => {
                let s_key = style_key.ok_or_else(|| {
                    "Validation Error: Style selection is mandatory in Regular mode.".to_string()
                })?;
                if domain_key.is_some() && !domain_key.unwrap().is_empty() {
                    return Err(
                        "Validation Error: Domain selection is invalid when Mode = Regular."
                            .to_string(),
                    );
                }

                let style_cfg = self
                    .style_registry
                    .get_style(s_key)
                    .ok_or_else(|| format!("Validation Error: Unknown style '{}'.", s_key))?;

                let permitted_ns = ScenarioNamespaceGuard::get_permitted_namespaces(
                    "regular",
                    None,
                    user_business_type,
                );

                let query = RetrievalQuery {
                    query_text: question.to_string(),
                    user_id: user_id.to_string(),
                    document_ids: None,
                    top_k: 5,
                    min_similarity: 0.20,
                };

                let filter_fn = |res: &crate::rag::types::SearchResult| {
                    ScenarioNamespaceGuard::is_chunk_permitted(
                        res.document_domain.as_deref(),
                        Some(&res.document_filename),
                        &permitted_ns,
                    ) && ScenarioNamespaceGuard::is_chunk_content_relevant(
                        question,
                        &res.chunk.content,
                    )
                };

                // FIX: D3 — propagate retrieval errors rather than masking as empty knowledge
                let retrieved = retriever
                    .retrieve_filtered(&query, None, &filter_fn)
                    .await
                    .map_err(|e| format!("Retrieval Error: {}", e))?;

                let filtered_chunks: Vec<crate::types::RagChunk> = retrieved
                    .into_iter()
                    .map(|res| crate::types::RagChunk {
                        domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
                        content: res.chunk.content,
                        metadata: Some(crate::types::ChunkMetadata {
                            page_number: res
                                .chunk
                                .metadata
                                .as_ref()
                                .and_then(|m| m.page_number)
                                .map(|p| p as u32),
                            source_file: res.document_filename,
                        }),
                    })
                    .collect();

                let mut resp = ScenarioRegularPipeline::run(
                    question,
                    style_cfg,
                    &self.prompts_dir,
                    &filtered_chunks,
                    permitted_ns,
                    compression_mode,
                );

                // GAP-S02 Fix: Wire compression pipeline if compression_mode is specified and orchestrator is available
                if let (Some(cm), Some(orch)) = (compression_mode, orchestrator) {
                    if !resp.is_empty_knowledge {
                        let mut session = crate::session::SessionHistory::new(5);
                        let enrichment = crate::types::EnrichmentContext {
                            cluster_vocab: None,
                            rag_chunks: Some(filtered_chunks.clone()),
                        };
                        if let Ok(comp_res) = orch
                            .process(
                                question,
                                &mut session,
                                None,
                                Some(cm),
                                None,
                                Some(enrichment),
                            )
                            .await
                        {
                            let compressed_prompt = match comp_res {
                                crate::types::OrchestratorResponse::Aggressive(r) => {
                                    r.optimized_prompt
                                }
                                crate::types::OrchestratorResponse::Legacy(r) => r.response,
                            };
                            resp.output_text
                                .push_str("\n\n### Compressed Operational Prompt\n");
                            resp.output_text.push_str(&compressed_prompt);
                        }
                    }
                }

                ScenarioOutputValidator::validate(
                    &resp.output_text,
                    &resp.citations,
                    resp.is_empty_knowledge,
                )?;
                Ok(resp)
            }
            _ => Err(format!(
                "Validation Error: Invalid Mode '{}'. Expected 'Scenario' or 'Regular'.",
                app_mode
            )),
        }
    }
}
