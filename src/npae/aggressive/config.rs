use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use anyhow::{Result, Context};
use std::path::Path;

use std::collections::HashMap;
use crate::npae::schema::types::ExecutionPhase;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnifiedConfig {
    pub domain_taxonomy: Vec<DomainTaxonomy>,
    pub roles: Vec<RoleRule>,
    pub constraints: Vec<ConstraintRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DomainTaxonomy {
    pub domain: String,
    pub keywords: Vec<String>,
    pub boost: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_templates: Option<HashMap<String, Vec<ExecutionPhase>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleRule {
    pub priority: u32,
    pub domain: String,
    pub base_title: String,
    pub verbs: Vec<WeightedWord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstraintRule {
    pub id: String,
    pub domain: String,
    pub priority: u32,
    pub is_forbidden: bool,
    pub description: String,
    pub trigger: Vec<Vec<WeightedWord>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeightedWord {
    pub word: String,
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<Vec<String>>,
}

/// Thread-safe config handle — uses RwLock for zero-dependency implementation
pub struct ConfigHandle {
    inner: Arc<RwLock<UnifiedConfig>>,
}

impl ConfigHandle {
    pub fn new(initial: UnifiedConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(initial)),
        }
    }

    pub fn read(&self) -> UnifiedConfig {
        self.inner.read().unwrap().clone()
    }

    pub fn swap(&self, new_config: UnifiedConfig) {
        let mut lock = self.inner.write().unwrap();
        *lock = new_config;
    }
}

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<UnifiedConfig> {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file at {:?}", path.as_ref()))?;
        let config: UnifiedConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse unified.json")?;
        
        Ok(config)
    }

    pub fn save<P: AsRef<Path>>(path: P, config: &UnifiedConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
