use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressConfig {
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScenarioEgressGuard {
    allowlist: HashSet<String>,
}

impl ScenarioEgressGuard {
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read egress config file '{}': {}", path, e))?;
        let cfg: EgressConfig = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse egress config JSON: {}", e))?;

        let allowlist = cfg.allowlist.into_iter().collect();
        info!(
            "Scenario_Egress_Guard initialized with allowlist: {:?}",
            allowlist
        );
        Ok(Self { allowlist })
    }

    /// Check whether a host target is allowed. Returns true if permitted.
    pub fn is_allowed(&self, host: &str) -> bool {
        let clean_host = host.split(':').next().unwrap_or(host).trim().to_lowercase();
        self.allowlist.contains(&clean_host) || self.allowlist.contains(host)
    }

    /// Guard an outbound call. Logs and returns error if blocked.
    pub fn validate_egress(&self, destination: &str) -> Result<(), String> {
        if !self.is_allowed(destination) {
            warn!(
                "Scenario_Egress_Guard BLOCKED unauthorized egress attempt to: {}",
                destination
            );
            return Err(format!(
                "Security Egress Violation: Network call to '{}' denied by Scenario_Egress_Guard allowlist.",
                destination
            ));
        }
        Ok(())
    }

    /// Reject registration of any tool requiring non-allowlisted network calls.
    pub fn validate_tool_registration(
        &self,
        tool_name: &str,
        required_endpoint: Option<&str>,
    ) -> Result<(), String> {
        if let Some(endpoint) = required_endpoint {
            if !self.is_allowed(endpoint) {
                return Err(format!(
                    "Tool registration rejected for '{}': endpoint '{}' not on egress allowlist.",
                    tool_name, endpoint
                ));
            }
        }
        Ok(())
    }
}

pub struct ScenarioNamespaceGuard;

impl ScenarioNamespaceGuard {
    /// Determines permitted namespaces based on mode and user entitlements.
    /// Admin has access to all namespaces (*).
    /// Standard users get namespaces matching their business_type plus general.
    pub fn get_permitted_namespaces(
        app_mode: &str,
        domain_namespace: Option<&str>,
        user_business_type: &str,
    ) -> Vec<String> {
        if app_mode.eq_ignore_ascii_case("scenario") {
            // Scenario mode strictly isolates to domain namespace only
            if let Some(ns) = domain_namespace {
                return vec![ns.to_string()];
            }
            return vec![];
        }

        // Regular mode
        let b_type = user_business_type.trim().to_lowercase();
        if b_type == "admin" {
            return vec![
                "all".to_string(),
                "legal_docs".to_string(),
                "business_strategy".to_string(),
                "financial_records".to_string(),
                "general".to_string(),
            ];
        }

        let mut permitted = vec!["general".to_string()];
        match b_type.as_str() {
            "legal" => permitted.push("legal_docs".to_string()),
            "business" => permitted.push("business_strategy".to_string()),
            "finance" | "financial" => permitted.push("financial_records".to_string()),
            "researcher" | "research" => {
                permitted.push("legal_docs".to_string());
                permitted.push("business_strategy".to_string());
                permitted.push("financial_records".to_string());
            }
            _ => {}
        }
        permitted
    }

    /// Checks if a retrieved chunk domain/namespace is permitted under current guard rules.
    pub fn is_chunk_permitted(
        chunk_domain: Option<&str>,
        chunk_filename: Option<&str>,
        permitted_namespaces: &[String],
    ) -> bool {
        if permitted_namespaces.contains(&"all".to_string()) {
            return true;
        }
        let domain = chunk_domain.unwrap_or("general").to_lowercase();
        let filename = chunk_filename.unwrap_or("").to_lowercase();

        permitted_namespaces.iter().any(|p| {
            let p_lower = p.to_lowercase();
            if p_lower == domain {
                return true;
            }

            if p_lower.starts_with(&domain) || domain.starts_with(&p_lower) {
                if filename.is_empty() || filename.contains(&p_lower) || !p_lower.contains('_') {
                    return true;
                }
            }
            false
        })
    }

    /// Generic content relevance filter:
    /// Ensures retrieved chunks have topical keyword/semantic relevance to the user question,
    /// preventing cross-domain or off-topic spurious matches (e.g. criminal code tables on civil succession queries).
    pub fn is_chunk_content_relevant(query: &str, chunk_content: &str) -> bool {
        let query_lower = query.to_lowercase();
        let chunk_lower = chunk_content.to_lowercase();

        // Extract query terms (length >= 4, ignoring common stop words)
        let query_terms: Vec<&str> = query_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| {
                w.len() >= 4
                    && !matches!(
                        *w,
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
                            | "search"
                    )
            })
            .collect();

        if query_terms.is_empty() {
            return true;
        }

        // Must match at least one significant query term or stem
        query_terms.iter().any(|&term| {
            let stem = if term.len() > 5 {
                &term[..term.len() - 1]
            } else {
                term
            };
            chunk_lower.contains(stem)
        })
    }
}
