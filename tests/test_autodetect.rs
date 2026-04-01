use token_compress_engine::engine::predictive_coding;

#[test]
fn test_detect_code() {
    let text = "I have a rust function that needs optimization and a clean api.";
    let detected = predictive_coding::detect_use_case(text);
    assert_eq!(detected, "code");
}

#[test]
fn test_detect_legal() {
    let text = "Please review the risk assessment in this contract clause.";
    let detected = predictive_coding::detect_use_case(text);
    assert_eq!(detected, "legal");
}

#[test]
fn test_detect_ticket() {
    let text = "We have a bug in production, please triage this issue.";
    let detected = predictive_coding::detect_use_case(text);
    assert_eq!(detected, "ticket");
}

#[test]
fn test_detect_resume() {
    let text = "The candidate has a strong CV and is asking for a higher salary.";
    let detected = predictive_coding::detect_use_case(text);
    assert_eq!(detected, "resume");
}

#[test]
fn test_detect_generic_fallback() {
    let text = "Just some random text without keywords.";
    let detected = predictive_coding::detect_use_case(text);
    assert_eq!(detected, "generic");
}

#[test]
fn test_run_with_auto_detect() {
    let chunks = vec!["I need to fix an api bug in my rust code.".to_string()];
    let result = predictive_coding::run(&chunks, "optimize", "", "", "", "auto", "Claude");

    // Should detect "code" or "ticket" (code has 'rust', 'api', 'code'; ticket has 'bug')
    // Based on the code order, "ticket" comes first if "bug" is found.
    assert!(
        result.text.contains("senior software engineer") || result.text.contains("data analyst")
    );
    assert!(result.detail.contains("detected: true"));
}
