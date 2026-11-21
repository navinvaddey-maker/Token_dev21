use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDomainConfig {
    pub key: String,
    pub label: String,
    pub schema_class: String,
    pub parser: String,
    pub gap_check_rules: Vec<String>,
    pub rag_namespace: String,
    #[serde(default)]
    pub min_similarity: Option<f32>,
    pub tool_allowlist: Vec<String>,
    pub output_contract: String,
    pub disclaimer_text: String,
    /// `rag_documents.domain` values (as written by `rag::ingest::detect_domain` from the
    /// `config/unified.json` taxonomy) that belong to this scenario domain's `rag_namespace`.
    ///
    /// FIX: D6 — ingestion never writes `legal_docs`/`financial_records`/`business_strategy`; it
    /// writes `legal`/`finance`/`business`. Without this explicit mapping the namespace guard
    /// dropped every normally ingested document. Defaults to empty (legacy behaviour).
    #[serde(default)]
    pub document_domain_aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDomainRegistry {
    pub domains: Vec<ScenarioDomainConfig>,
}

impl ScenarioDomainRegistry {
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read domain registry file '{}': {}", path, e))?;
        let registry: ScenarioDomainRegistry = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse domain registry JSON: {}", e))?;
        info!(
            "Loaded {} scenario domains from {}",
            registry.domains.len(),
            path
        );
        Ok(registry)
    }

    pub fn get_domain(&self, key: &str) -> Option<&ScenarioDomainConfig> {
        self.domains
            .iter()
            .find(|d| d.key.eq_ignore_ascii_case(key))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStyleConfig {
    pub key: String,
    pub label: String,
    pub description: String,
    pub prompt_file: String,
    pub version: String,
    /// Cached file content of the style prompt loaded at startup.
    ///
    /// ARCH: §2.1 & Perf Workstream 3 — cached in-memory so request threads never perform
    /// synchronous disk I/O when resolving style prompts.
    #[serde(skip, default)]
    pub prompt_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStyleRegistry {
    pub styles: Vec<ScenarioStyleConfig>,
}

impl ScenarioStyleRegistry {
    /// Loads the scenario style registry and validates/caches all referenced prompt templates.
    ///
    /// # Arguments
    /// * `path` - Path to the styles.json configuration file.
    /// * `prompts_dir` - Directory containing versioned style prompt template files.
    ///
    /// # Errors
    /// Returns `Err(String)` if reading/parsing the JSON fails, or if any configured
    /// `prompt_file` does not exist or fails to read into memory.
    ///
    /// # Panics
    /// Never.
    pub fn load_from_file(path: &str, prompts_dir: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read style registry file '{}': {}", path, e))?;
        let mut registry: ScenarioStyleRegistry = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse style registry JSON: {}", e))?;

        // Read and cache each versioned style prompt file into prompt_content once at startup
        for style in &mut registry.styles {
            let prompt_path = Path::new(prompts_dir).join(&style.prompt_file);
            let prompt_str = fs::read_to_string(&prompt_path).map_err(|e| {
                format!(
                    "Startup validation failed: Style '{}' requires prompt file '{:?}' which failed to read: {}",
                    style.key, prompt_path, e
                )
            })?;
            style.prompt_content = prompt_str;
        }

        info!(
            "Loaded and cached {} scenario styles with prompt templates from {}",
            registry.styles.len(),
            path
        );
        Ok(registry)
    }

    pub fn get_style(&self, key: &str) -> Option<&ScenarioStyleConfig> {
        self.styles.iter().find(|s| s.key.eq_ignore_ascii_case(key))
    }
}
