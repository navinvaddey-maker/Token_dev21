use token_compress_engine::{
    algorithms::sparse_coding::SparseCoding,
    types::{ScoredToken, TokenSource},
};

#[test]
fn test_sparse_coding_basic() {
    let sc = SparseCoding::new(0.3, 0.6);
    let tokens = vec![
        "the".to_string(),
        "quick".to_string(),
        "brown".to_string(),
        "fox".to_string(),
        "authentication".to_string(),
    ];
    let result = sc.apply(&tokens, 0.4); // Keep 40%

    // Should have fewer tokens than input
    assert!(result.len() < tokens.len());

    // All tokens should have Sparse source
    for token in &result {
        assert_eq!(token.source, TokenSource::Sparse);
    }
}

#[test]
fn test_sparse_coding_salience_range() {
    let sc = SparseCoding::new(0.3, 0.6);
    let tokens = vec![
        "OAuth2".to_string(),
        "the".to_string(),
        "authentication".to_string(),
    ];
    let scored = sc.compute_salience(&tokens);

    // All salience scores should be between 0.0 and 1.0
    for (_, score) in &scored {
        assert!(*score >= 0.0 && *score <= 1.0);
    }
}

#[test]
fn test_sparse_coding_technical_higher_salience() {
    let sc = SparseCoding::new(0.3, 0.6);
    let tokens = vec!["authentication".to_string(), "the".to_string()];
    let scored = sc.compute_salience(&tokens);

    // Find scores for each token
    let auth_score = scored
        .iter()
        .find(|(t, _)| t == "authentication")
        .map(|(_, s)| *s)
        .unwrap_or(0.0);

    let the_score = scored
        .iter()
        .find(|(t, _)| t == "the")
        .map(|(_, s)| *s)
        .unwrap_or(0.0);

    // Technical token should have higher salience than common word
    assert!(auth_score > the_score);
}

#[test]
fn test_sparse_coding_preserves_order() {
    let sc = SparseCoding::new(0.3, 0.6);
    let tokens = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
        "fourth".to_string(),
        "fifth".to_string(),
    ];
    let result = sc.apply(&tokens, 0.6); // Keep 60%

    // Extract just the text to check order preservation
    let result_texts: Vec<String> = result.iter().map(|t| t.text.clone()).collect();

    // Should preserve original order (just with fewer elements)
    let expected_order = vec!["first", "second", "third", "fourth", "fifth"];
    let mut expected_filtered = Vec::new();
    for text in expected_order {
        if result_texts.contains(&text.to_string()) {
            expected_filtered.push(text.to_string());
        }
    }

    assert_eq!(result_texts, expected_filtered);
}
