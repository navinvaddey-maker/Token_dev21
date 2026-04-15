use dashmap::DashMap;
use std::sync::Arc;
use token_compress_engine::{
    algorithms::predictive_coding::{PredictiveCoding, SessionTurn},
    types::{AlgorithmOutput, Mode, PromptTopology, ScoredToken, TokenSource},
};

fn make_tokens(words: &[&str]) -> Vec<ScoredToken> {
    words
        .iter()
        .map(|w| ScoredToken {
            text: w.to_string(),
            salience: 0.7,
            source: TokenSource::Sparse,
        })
        .collect()
}

#[test]
fn test_simple_prompt_routes_gentle() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    // Pre-fill schema with common tokens
    pc.update_schema(
        &["what", "is", "python", "programming"]
            .map(String::from)
            .to_vec(),
    );
    let tokens = make_tokens(&["what", "is", "python"]);
    let result = pc.compute_error(&tokens, &[], PromptTopology::Linear, &AlgorithmOutput::default());
    assert_eq!(result.mode, Mode::Gentle);
}

#[test]
fn test_complex_prompt_routes_aggressive() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    let tokens = make_tokens(&[
        "multi-tenant",
        "OAuth2",
        "HSM",
        "key-rotation",
        "fintech",
        "PKCE",
        "10M-TPS",
    ]);
    let result = pc.compute_error(&tokens, &[], PromptTopology::Linear, &AlgorithmOutput::default());
    assert_eq!(result.mode, Mode::Aggressive);
}

#[test]
fn test_schema_update_lowers_error_on_second_ask() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    let tokens = make_tokens(&["OAuth2", "PKCE", "fintech"]);

    let first = pc.compute_error(&tokens, &[], PromptTopology::Linear, &AlgorithmOutput::default());
    pc.update_schema(&first.delta_tokens);
    let second = pc.compute_error(&tokens, &[], PromptTopology::Linear, &AlgorithmOutput::default());

    assert!(
        second.error_score < first.error_score,
        "Error score must drop after schema update"
    );
}

#[test]
fn test_hysteresis_band() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    // Pre-fill schema with known tokens
    pc.update_schema(&["known1", "known2"].map(String::from).to_vec());
    // Create tokens that should give error score in hysteresis band (0.45-0.55)
    // Mix of known and unknown tokens
    let tokens = make_tokens(&["known1", "known2", "unknown1", "unknown2"]); // 50% unknown

    // With no session history, should be aggressive (new session)
    let result = pc.compute_error(&tokens, &[], PromptTopology::Linear, &AlgorithmOutput::default());
    println!(
        "Hysteresis test: no session - error_score: {}, mode: {:?}",
        result.error_score, result.mode
    );
    assert_eq!(result.mode, Mode::Aggressive);

    // With established session history (depth >= 3), should be gentle
    let mut session = Vec::new();
    session.push(SessionTurn {
        tokens: vec!["known1".to_string(), "known2".to_string()],
        mode: "gentle".to_string(),
        error_score: 0.3,
    });
    session.push(SessionTurn {
        tokens: vec!["known1".to_string(), "known2".to_string()],
        mode: "gentle".to_string(),
        error_score: 0.3,
    });
    session.push(SessionTurn {
        tokens: vec!["known1".to_string(), "known2".to_string()],
        mode: "gentle".to_string(),
        error_score: 0.3,
    });

    let result2 = pc.compute_error(&tokens, &session, PromptTopology::Linear, &AlgorithmOutput::default());
    println!(
        "Hysteresis test: with session - error_score: {}, mode: {:?}, session_depth: {}",
        result2.error_score,
        result2.mode,
        session.len()
    );

    assert_eq!(result2.mode, Mode::Gentle);
}

#[test]
fn test_session_discount_capped() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    let tokens = make_tokens(&["token1", "token2", "token3"]);

    // Create session history with lots of overlap
    let mut session = Vec::new();
    for _ in 0..10 {
        session.push(SessionTurn {
            tokens: vec![
                "token1".to_string(),
                "token2".to_string(),
                "token3".to_string(),
            ],
            mode: "gentle".to_string(),
            error_score: 0.5,
        });
    }

    let result = pc.compute_error(&tokens, &session, PromptTopology::Linear, &AlgorithmOutput::default());
    // With 3/3 tokens novel and max session discount of 0.35:
    // error_score = 1.0 * (1.0 - 0.35) = 0.65
    assert!(result.error_score > 0.0);
    assert!((result.error_score - 0.65).abs() < 0.01);
}
