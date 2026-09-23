use crate::{
    algorithms::{lexical::LexicalCompression, sparse_coding::SparseCoding},
    types::AlgorithmOutput,
};

/// Handles Stage 1: Signal Reduction (Lexical Compression → Sparse Coding)
#[derive(Default)]
pub struct Stage1 {
    lexical: LexicalCompression,
    sparse: SparseCoding,
}

impl Stage1 {
    /// Creates a new Stage1 instance
    pub fn new() -> Self {
        Self {
            lexical: LexicalCompression::default(),
            sparse: SparseCoding::default(),
        }
    }

    /// Processes input through Stage 1: Signal Reduction
    /// Now mode-aware based on forced_mode or topology classification.
    pub fn run(
        &self,
        raw_prompt: &str,
        out: &mut AlgorithmOutput,
        topology: &crate::types::PromptTopology,
        forced_mode: Option<&str>,
    ) {
        // Map PromptTopology (metadata) to TopologyResult (routing engine)
        let routing_topology = crate::engine::neuro::routing::TopologyResult {
            topology_type: match topology {
                crate::types::PromptTopology::Linear => crate::engine::neuro::routing::TopologyType::Sequential,
                crate::types::PromptTopology::Hierarchical => crate::engine::neuro::routing::TopologyType::Hierarchical,
                crate::types::PromptTopology::Network => crate::engine::neuro::routing::TopologyType::Graph,
                crate::types::PromptTopology::Flat => crate::engine::neuro::routing::TopologyType::Tabular,
            },
        };

        // Topology Routing to determine sparse mode
        let router = crate::engine::neuro::routing::TopologyRouter;
        let profile = router.configure(&routing_topology);
        let sparse_mode = profile.sparse_mode;

        // Determine keep_ratio based on mode or topology proxy
        let keep_ratio = match forced_mode {
            Some("gentle") => self.sparse.gentle_keep_ratio,
            Some("aggressive") => self.sparse.aggressive_keep_ratio,
            Some("balanced") => (self.sparse.gentle_keep_ratio + self.sparse.aggressive_keep_ratio) / 2.0,
            _ => {
                // Proxy ratio based on topology if mode is not forced
                match routing_topology.topology_type {
                    crate::engine::neuro::routing::TopologyType::Hierarchical
                    | crate::engine::neuro::routing::TopologyType::Graph => self.sparse.aggressive_keep_ratio,
                    _ => self.sparse.gentle_keep_ratio,
                }
            }
        };

        // Existing: Lexical Compression
        let lex_result = self.lexical.compress(raw_prompt, out);
        out.clean_tokens = lex_result.tokens;
        out.compression_ratio = lex_result.ratio;
        out.input_token_count = lex_result.input_tokens;
        out.output_token_count = lex_result.output_tokens;

        // New: Sparse Coding — receives clean_tokens and resolved keep_ratio
        let scored = self.sparse.apply(&out.clean_tokens, keep_ratio, &out.constraint_locks, &sparse_mode);
        out.salience_map = scored
            .iter()
            .map(|t| (t.text.clone(), t.salience))
            .collect();
        out.sparse_tokens = scored;
    }
}
