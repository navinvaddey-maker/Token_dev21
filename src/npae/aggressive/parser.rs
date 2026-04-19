#[derive(Debug, Clone)]
pub struct ParsedPrompt {
    pub tokens: Vec<String>,
    pub bigrams: Vec<(String, String)>,
    pub domain_signals: Vec<String>,
    pub raw: String,
}

pub fn parse_prompt(raw: &str) -> ParsedPrompt {
    let lower = raw.to_lowercase();
    
    // Tokenization
    let tokens: Vec<String> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Bigrams
    let bigrams = tokens
        .windows(2)
        .map(|w| (w[0].clone(), w[1].clone()))
        .collect();

    // Domain signals (e.g. tech terms, nutrition terms)
    // For now, let's keep it simple: everything is a potential signal
    // ConfigVerifier will filter them.
    let domain_signals = tokens.clone();

    ParsedPrompt {
        tokens,
        bigrams,
        domain_signals,
        raw: raw.to_string(),
    }
}
