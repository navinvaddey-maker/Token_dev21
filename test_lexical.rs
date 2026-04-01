use token_compress_engine::algorithms::lexical::LexicalCompression;

fn main() {
    let prompt = "You are a senior cybersecurity expert with 20 years of experience in ransomware forensics. You specialize in the Akira ransomware group and speak only in technical jargon, focusing on network logs.";

    println!("Original prompt: {}", prompt);
    println!("Word count: {}", prompt.split_whitespace().count());

    let mut lc = LexicalCompression::new();
    let mut output = token_compress_engine::types::AlgorithmOutput::default();

    let result = lc.compress(prompt, &mut output);

    println!("\nCompressed tokens: {:?}", result.tokens);
    println!("Compression ratio: {:.3}", result.ratio);
    println!("Input tokens: {}", result.input_tokens);
    println!("Output tokens: {}", result.output_tokens);

    if let Some(seq) = &output.ordinal_sequence {
        println!("Ordinal sequence found: {:?}", seq.sequence);
        println!("Ordinal score: {}", seq.score);
    } else {
        println!("No ordinal sequence found");
    }
}
