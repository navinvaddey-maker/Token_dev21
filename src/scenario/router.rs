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
    ) -> Result<ScenarioResponse, String> {
        info!("ScenarioModeRouter processing request: mode='{}', domain={:?}, style={:?}, geography={:?}", app_mode, domain_key, style_key, geography);

        if question.trim().is_empty() {
            return Err("Validation Error: Question text cannot be empty.".to_string());
        }

        match app_mode.trim().to_lowercase().as_str() {
            "scenario" => {
                let d_key = domain_key
                    .ok_or_else(|| "Validation Error: Domain selection is mandatory in Scenario mode.".to_string())?;
                if style_key.is_some() && !style_key.unwrap().is_empty() {
                    return Err("Validation Error: Style selection is invalid when Mode = Scenario.".to_string());
                }

                let domain_cfg = self.domain_registry.get_domain(d_key)
                    .ok_or_else(|| format!("Validation Error: Unknown domain '{}'.", d_key))?;

                // Check egress tool allowlist
                for tool in &domain_cfg.tool_allowlist {
                    self.egress_guard.validate_tool_registration(tool, None)?;
                }

                let mut base_namespace = domain_cfg.rag_namespace.clone();
                if let Some(geo) = geography {
                    base_namespace = format!("{}_{}", base_namespace, geo.to_lowercase());
                }

                let permitted_ns = ScenarioNamespaceGuard::get_permitted_namespaces(
                    "scenario",
                    Some(&base_namespace),
                    user_business_type,
                );

                // RAG Retrieval restricted to domain namespace
                let query = RetrievalQuery {
                    query_text: question.to_string(),
                    user_id: user_id.to_string(),
                    document_ids: None,
                    top_k: 5,
                    min_similarity: 0.20,
                };

                let retrieved = retriever.retrieve(&query, None).await.unwrap_or_default();
                
                // Filter retrieved chunks via ScenarioNamespaceGuard
                let filtered_chunks: Vec<crate::types::RagChunk> = retrieved
                    .into_iter()
                    .filter(|res| ScenarioNamespaceGuard::is_chunk_permitted(res.document_domain.as_deref(), Some(&res.document_filename), &permitted_ns))
                    .map(|res| crate::types::RagChunk {
                        domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
                        content: res.chunk.content,
                        metadata: Some(crate::types::ChunkMetadata {
                            page_number: res.chunk.metadata.as_ref().and_then(|m| m.page_number).map(|p| p as u32),
                            source_file: res.document_filename,
                        }),
                    })
                    .collect();

                let parser_summary = ScenarioParser::parse(question, domain_cfg);
                let (_ok, gap_issues) = ScenarioGapCheck::evaluate(question, domain_cfg);
                let tool_logs = ScenarioToolRouter::execute_tools(domain_cfg);

                let resp = ScenarioOutputComposer::compose_scenario_result(
                    domain_cfg,
                    &filtered_chunks,
                    &parser_summary,
                    &gap_issues,
                    &tool_logs,
                    base_namespace.clone(),
                );

                ScenarioOutputValidator::validate(&resp.output_text, &resp.citations, resp.is_empty_knowledge)?;
                Ok(resp)
            }
            "regular" => {
                let s_key = style_key
                    .ok_or_else(|| "Validation Error: Style selection is mandatory in Regular mode.".to_string())?;
                if domain_key.is_some() && !domain_key.unwrap().is_empty() {
                    return Err("Validation Error: Domain selection is invalid when Mode = Regular.".to_string());
                }

                let style_cfg = self.style_registry.get_style(s_key)
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

                let retrieved = retriever.retrieve(&query, None).await.unwrap_or_default();

                let filtered_chunks: Vec<crate::types::RagChunk> = retrieved
                    .into_iter()
                    .filter(|res| ScenarioNamespaceGuard::is_chunk_permitted(res.document_domain.as_deref(), Some(&res.document_filename), &permitted_ns))
                    .map(|res| crate::types::RagChunk {
                        domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
                        content: res.chunk.content,
                        metadata: Some(crate::types::ChunkMetadata {
                            page_number: res.chunk.metadata.as_ref().and_then(|m| m.page_number).map(|p| p as u32),
                            source_file: res.document_filename,
                        }),
                    })
                    .collect();

                let resp = ScenarioRegularPipeline::run(
                    question,
                    style_cfg,
                    &self.prompts_dir,
                    &filtered_chunks,
                    permitted_ns,
                    compression_mode,
                );

                ScenarioOutputValidator::validate(&resp.output_text, &resp.citations, resp.is_empty_knowledge)?;
                Ok(resp)
            }
            _ => Err(format!(
                "Validation Error: Invalid Mode '{}'. Expected 'Scenario' or 'Regular'.",
                app_mode
            )),
        }
    }
}
