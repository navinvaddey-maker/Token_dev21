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
    pub tool_allowlist: Vec<String>,
    pub output_contract: String,
    pub disclaimer_text: String,
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
        info!("Loaded {} scenario domains from {}", registry.domains.len(), path);
        Ok(registry)
    }

    pub fn get_domain(&self, key: &str) -> Option<&ScenarioDomainConfig> {
        self.domains.iter().find(|d| d.key.eq_ignore_ascii_case(key))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStyleConfig {
    pub key: String,
    pub label: String,
    pub description: String,
    pub prompt_file: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStyleRegistry {
    pub styles: Vec<ScenarioStyleConfig>,
}

impl ScenarioStyleRegistry {
    pub fn load_from_file(path: &str, prompts_dir: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read style registry file '{}': {}", path, e))?;
        let registry: ScenarioStyleRegistry = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse style registry JSON: {}", e))?;

        // Validate that each versioned style prompt file exists
        for style in &registry.styles {
            let prompt_path = Path::new(prompts_dir).join(&style.prompt_file);
            if !prompt_path.exists() {
                return Err(format!(
                    "Startup validation failed: Style '{}' requires prompt file '{:?}' which does not exist.",
                    style.key, prompt_path
                ));
            }
        }

        info!("Loaded and validated {} scenario styles from {}", registry.styles.len(), path);
        Ok(registry)
    }

    pub fn get_style(&self, key: &str) -> Option<&ScenarioStyleConfig> {
        self.styles.iter().find(|s| s.key.eq_ignore_ascii_case(key))
    }
}
