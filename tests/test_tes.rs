use token_compress_engine::scoring::tes::TokenEfficiencyScorer;
use token_compress_engine::types::{AlgorithmOutput, FieldValidationIssue};

#[test]
fn test_tes_no_issues() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 50,
        ..Default::default()
    };
    let score = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 0.5); // 50/100 = 0.5 compression ratio, no penalty
}

#[test]
fn test_tes_with_issues() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 50,
        ..Default::default()
    };
    let field_issues = vec![
        FieldValidationIssue {
            field_name: "task".to_string(),
            issue_type: "type_mismatch".to_string(),
            description: "Task field expects String but got Number".to_string(),
            severity: "error".to_string(),
        },
        FieldValidationIssue {
            field_name: "deliverable".to_string(),
            issue_type: "missing_required".to_string(),
            description: "Deliverable field is required but missing".to_string(),
            severity: "error".to_string(),
        },
    ];
    let score = TokenEfficiencyScorer::score(&output, &field_issues);
    // Should be less than 0.5 due to penalty from issues
    assert!(score < 0.5);
    // Should still be positive
    assert!(score > 0.0);
}

#[test]
fn test_tes_perfect_compression() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 100,
        ..Default::default()
    };
    let score = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 1.0); // 100/100 = 1.0, no penalty
}

#[test]
fn test_tes_expansion() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 150,
        ..Default::default()
    };
    let score = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 1.5); // 150/100 = 1.5, but we expect this to be clamped in practice
                            // Note: In practice, TES might be clamped, but the basic calculation allows expansion
}

#[test]
fn test_tes_zero_input() {
    let output = AlgorithmOutput {
        input_token_count: 0,
        output_token_count: 50,
        ..Default::default()
    };
    let score = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 0.0); // Handle division by zero
}
