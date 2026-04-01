use dashmap::DashMap;
use std::sync::Arc;
use token_compress_engine::algorithms::predictive_coding::{PredictiveCoding, SessionTurn};
use token_compress_engine::types::{Mode, ScoredToken, TokenSource};

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

fn main() {
    let pc = PredictiveCoding::new(Arc::new(DashMap::new()));
    // Create tokens that should give error score in hysteresis band (0.45-0.55)
    // Mix of known and unknown tokens
    let tokens = make_tokens(&["known1", "known2", "unknown1", "unknown2"]); // 50% unknown

    // With no session history, should be aggressive (new session)
    let result = pc.compute_error(&tokens, &[]);
    println!(
        "No session - error_score: {}, mode: {:?}",
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

    let result2 = pc.compute_error(&tokens, &session);
    println!(
        "With session - error_score: {}, mode: {:?}, session_depth: {}",
        result2.error_score,
        result2.mode,
        session.len()
    );
    assert_eq!(result2.mode, Mode::Gentle);
}
