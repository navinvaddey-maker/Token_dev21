use crate::algorithms::lexical::{LexicalCompression, OrdinalExtractor};
use crate::types::AlgorithmOutput;

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("=== INPUT ===");
    println!("{}", prompt);
    println!();

    println!("=== ORDINAL EXTRACTION ===");
    let sequence = OrdinalExtractor::extract(prompt);
    println!("Ordinal sequence: {:?}", sequence);
    println!();

    println!("=== LEXICAL COMPRESSION ===");
    let lc = LexicalCompression::new();
    let mut output = AlgorithmOutput::default();
    let result = lc.compress(prompt, &mut output);

    println!("Tokens: {:?}", result.tokens);
    println!("Compression ratio: {}", result.ratio);
    println!("Input tokens: {}", result.input_tokens);
    println!("Output tokens: {}", result.output_tokens);
    println!();

    println!("=== ALGORITHM OUTPUT ===");
    if let Some(seq) = &output.ordinal_sequence {
        println!("Ordinal sequence in output: {:?}", seq.sequence);
        println!("Ordinal score: {}", seq.score);
    } else {
        println!("No ordinal sequence in output");
    }
}
