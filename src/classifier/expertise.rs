/// Detects the user's apparent expertise level from the raw query text.
///
/// This is intentionally rule-based (not a second embedding call) because:
/// 1. Expertise is a lexical signal — jargon presence/absence is explicit.
/// 2. Avoids the overhead of a second cosine lookup for a 3-way classification.
/// 3. Decoupled from domain — expertise signals are domain-agnostic.
///
/// Returns: "beginner" | "intermediate" | "expert"
pub fn detect_expertise_from_query(query: &str) -> &'static str {
    let normalized = query.to_lowercase();

    // Expert-level signals: domain-specific jargon requiring prior knowledge
    // A query containing 2+ of these very likely comes from an expert user
    let expert_lexicon: &[&str] = &[
        // Real estate / finance
        "arbitrage", "cap rate", "irr", "noi", "1031 exchange",
        "securitization", "due diligence", "encumbrance", "lien",
        "amortization schedule", "ltv ratio", "debt service coverage",
        // Legal
        "deposition", "writ", "indemnification", "tort", "injunction",
        "fiduciary", "subrogation", "estoppel",
        // Tech
        "eigenvalue", "convexity", "mutex", "deadlock", "async runtime",
        "borrow checker", "lifetime annotation",
        // Finance/investment
        "tranche", "cds", "mbs", "alpha", "beta coefficient",
        "sharpe ratio", "basis points",
    ];

    // Beginner-level signals: vague, aspirational, or explicitly self-identified
    let beginner_lexicon: &[&str] = &[
        "how do i", "what is", "explain to me", "i want to learn",
        "beginner", "newbie", "just starting", "no experience",
        "simple", "easy way", "guide me", "help me understand",
        "i don't know", "where do i start", "from scratch",
    ];

    let expert_matches = expert_lexicon.iter()
        .filter(|term| normalized.contains(*term))
        .count();

    let beginner_matches = beginner_lexicon.iter()
        .filter(|term| normalized.contains(*term))
        .count();

    let word_count = normalized.split_whitespace().count();

    if expert_matches >= 2 {
        "expert"
    } else if beginner_matches >= 2 || (word_count < 8 && beginner_matches >= 1) {
        // Short vague queries with even one beginner signal → beginner
        "beginner"
    } else {
        // Default: intermediate covers the broad middle — long queries with
        // no strong signal in either direction
        "intermediate"
    }
}
