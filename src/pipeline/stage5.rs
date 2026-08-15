use crate::{
    algorithms::{
        schema_filling::DeterminismScopeInjector,
        sparse_coding::SparseCoding,
    },
    types::{AlgorithmOutput, Mode},
};

/// Handles Stage 5: Scope Injection
pub struct Stage5 {
    _sparse: SparseCoding,
}

impl Stage5 {
    /// Creates a new Stage5 instance
    pub fn new() -> Self {
        Self {
            _sparse: SparseCoding::default(),
        }
    }

    /// Processes input through Stage 5: Scope Injection
    /// Mode-aware: Gentle = no scope injection, Aggressive = scope injection from clustering
    pub fn run(&self, out: &mut AlgorithmOutput) {
        let mode = out.mode.as_ref().unwrap_or(&Mode::Gentle);

        match mode {
            Mode::Gentle | Mode::Ambiguous | Mode::Balanced => {
                // Gentle: no scope injection
                out.scope_injections = vec![];
            }
            Mode::Aggressive => {
                // Use domain-aware scope injection from the schema filling module
                let injections = DeterminismScopeInjector::inject(
                    &out.resolved_schema.task,
                    &None, // deliverable
                    &out.resolved_schema.context.iter().cloned().collect::<Vec<_>>(),
                );
                out.scope_injections = if injections.is_empty() {
                    // Fallback to cluster-derived, but filter non-semantic labels
                    out.cluster_labels
                        .iter()
                        .filter(|l| l.len() > 4 && !l.to_lowercase().starts_with("cluster_"))
                        .map(|label| format!("Consider {}", label))
                        .collect()
                } else {
                    injections
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CompressionSchema;

    #[test]
    fn test_gentle_mode_no_injections() {
        let stage5 = Stage5::new();
        let mut out = AlgorithmOutput {
            mode: Some(Mode::Gentle),
            cluster_labels: vec!["authentication".to_string()],
            ..Default::default()
        };
        stage5.run(&mut out);
        assert!(out.scope_injections.is_empty());
    }

    #[test]
    fn test_aggressive_mode_domain_aware_injections() {
        let stage5 = Stage5::new();
        let mut out = AlgorithmOutput {
            mode: Some(Mode::Aggressive),
            resolved_schema: CompressionSchema {
                task: Some("Build authentication microservice".to_string()),
                ..Default::default()
            },
            cluster_labels: vec!["Cluster_0".to_string(), "Delta".to_string()],
            ..Default::default()
        };
        stage5.run(&mut out);
        assert!(!out.scope_injections.is_empty());
        assert!(out.scope_injections.iter().any(|s| s.contains("OAuth 2.0")));
    }

    #[test]
    fn test_aggressive_mode_fallback_filters_cluster_labels() {
        let stage5 = Stage5::new();
        let mut out = AlgorithmOutput {
            mode: Some(Mode::Aggressive),
            resolved_schema: CompressionSchema {
                task: Some("Generic unmapped task".to_string()),
                ..Default::default()
            },
            cluster_labels: vec![
                "Cluster_0".to_string(),
                "cluster_1".to_string(),
                "foo".to_string(), // len <= 4
                "optimization".to_string(),
            ],
            ..Default::default()
        };
        stage5.run(&mut out);
        assert_eq!(out.scope_injections, vec!["Consider optimization".to_string()]);
    }
}
