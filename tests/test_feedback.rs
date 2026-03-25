#[cfg(test)]
mod feedback_tests {
    use token_compress_engine::behavior::feedback_detector::{detect, DetectionContext};

    #[test]
    fn repetition_detected() {
        let ctx = DetectionContext {
            user_id:          "u1".into(),
            history_id:       "h1".into(),
            prev_prompt:      Some("fix the login bug in auth module".into()),
            curr_prompt:      "fix login bug in the auth module please".into(),
            response_time_ms: 5_000,
            engagement_ms:    10_000,
        };
        let signals = detect(&ctx);
        assert!(signals.iter().any(|s| s.signal_type == "repetition"));
    }

    #[test]
    fn forgot_detected() {
        let ctx = DetectionContext {
            user_id:          "u1".into(),
            history_id:       "h1".into(),
            prev_prompt:      Some("summarize this".into()),
            curr_prompt:      "as i said, summarize in bullet points".into(),
            response_time_ms: 20_000,
            engagement_ms:    10_000,
        };
        let signals = detect(&ctx);
        assert!(signals.iter().any(|s| s.signal_type == "forgot"));
    }

    #[test]
    fn fast_reprompt_detected() {
        let ctx = DetectionContext {
            user_id:          "u1".into(),
            history_id:       "h1".into(),
            prev_prompt:      None,
            curr_prompt:      "something else".into(),
            response_time_ms: 8_000,
            engagement_ms:    10_000,
        };
        let signals = detect(&ctx);
        assert!(signals.iter().any(|s| s.signal_type == "fast_reprompt"));
    }

    #[test]
    fn long_engagement_positive_signal() {
        let ctx = DetectionContext {
            user_id:          "u1".into(),
            history_id:       "h1".into(),
            prev_prompt:      None,
            curr_prompt:      "new question".into(),
            response_time_ms: 60_000,
            engagement_ms:    400_000,
        };
        let signals = detect(&ctx);
        assert!(signals.iter().any(|s| s.signal_type == "long_engagement"));
    }

    #[test]
    fn no_false_signals_on_clean_input() {
        let ctx = DetectionContext {
            user_id:          "u1".into(),
            history_id:       "h1".into(),
            prev_prompt:      None,
            curr_prompt:      "summarize the report".into(),
            response_time_ms: 30_000,
            engagement_ms:    60_000,
        };
        let signals = detect(&ctx);
        assert!(signals.is_empty());
    }
}
