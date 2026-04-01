use token_compress_engine::{
    algorithms::{lexical::LexicalCompression, sparse_coding::SparseCoding},
    types::AlgorithmOutput,
};

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("=== INPUT ===");
    println!("{}", prompt);
    println!();

    // Test lexical compression
    let lc = LexicalCompression::new();
    let mut output = AlgorithmOutput::default();
    let lex_result = lc.compress(prompt, &mut output);

    println!("=== LEXICAL COMPRESSION OUTPUT ===");
    println!("Tokens: {:?}", lex_result.tokens);
    println!("Compression ratio: {}", lex_result.ratio);
    println!();

    // Test sparse coding
    let sparse = SparseCoding::default();
    let scored = sparse.apply(&lex_result.tokens, 0.60); // using aggressive ratio as in stage1

    println!("=== SPARSE CODING OUTPUT ===");
    println!("Number of scored tokens: {}", scored.len());
    for token in &scored {
        println!("  {}: {:.3}", token.text, token.salience);
    }
    println!();

    // Show tokens above threshold
    let selected: Vec<&ScoredToken> = scored.iter().filter(|t| t.salience >= 0.60).collect();
    println!("=== SELECTED TOKENS (salience >= 0.60) ===");
    for token in selected {
        println!("  {}: {:.3}", token.text, token.salience);
    }
    println!("Count: {}", selected.len());
}
