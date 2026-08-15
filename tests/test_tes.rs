use token_compress_engine::scoring::tes::TokenEfficiencyScorer;
use token_compress_engine::types::{AlgorithmOutput, FieldValidationIssue};

#[test]
fn test_tes_no_issues() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 50,
        ..Default::default()
    };
    let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
    assert!((score - 8.0).abs() < 0.1); // 50% savings -> ~8.0 in v2
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
    let (score, _) = TokenEfficiencyScorer::score(&output, &field_issues);
    // Should be less than 8.0 due to penalty from issues
    assert!(score < 8.0);
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
    let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 0.0); // 0% savings -> 0.0
}

#[test]
fn test_tes_expansion() {
    let output = AlgorithmOutput {
        input_token_count: 100,
        output_token_count: 150,
        ..Default::default()
    };
    let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 0.0); // Expansion -> 0.0
}

#[test]
fn test_tes_zero_input() {
    let output = AlgorithmOutput {
        input_token_count: 0,
        output_token_count: 50,
        ..Default::default()
    };
    let (score, _) = TokenEfficiencyScorer::score(&output, &[]);
    assert_eq!(score, 0.0); // Handle division by zero
}
