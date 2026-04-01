use crate::{
    algorithms::{lexical::LexicalCompression, sparse_coding::SparseCoding},
    types::AlgorithmOutput,
};

/// Handles Stage 1: Signal Reduction (Lexical Compression → Sparse Coding)
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
    /// Always runs, both modes. No mode awareness yet.
    pub fn run(&self, raw_prompt: &str, out: &mut AlgorithmOutput) {
        // Existing: Lexical Compression
        let lex_result = self.lexical.compress(raw_prompt, out);
        out.clean_tokens = lex_result.tokens;
        out.compression_ratio = lex_result.ratio;
        out.input_token_count = lex_result.input_tokens;
        out.output_token_count = lex_result.output_tokens;

        // New: Sparse Coding — receives clean_tokens
        // Use aggressive ratio at stage 1 (mode unknown; be generous)
        // Increased threshold for more aggressive compression
        let scored = self.sparse.apply(&out.clean_tokens, 0.65);
        out.salience_map = scored
            .iter()
            .map(|t| (t.text.clone(), t.salience))
            .collect();
        out.sparse_tokens = scored;
    }
}
