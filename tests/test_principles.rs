#[cfg(test)]
mod sparse_tests {
    use token_compress_engine::engine::sparse_coding;

    #[test]
    fn removes_fillers() {
        let r = sparse_coding::run("I just basically need help.", "balanced");
        assert!(!r.text.contains("just"));
        assert!(!r.text.contains("basically"));
        assert!(r.items_removed >= 2);
    }

    #[test]
    fn preserves_signal() {
        let r = sparse_coding::run("Analyze the revenue report.", "balanced");
        assert!(r.text.contains("revenue"));
    }

    #[test]
    fn aggressive_removes_more() {
        let text = "Maybe you could possibly help me.";
        assert!(
            sparse_coding::run(text, "aggressive").items_removed
                >= sparse_coding::run(text, "gentle").items_removed
        );
    }

    #[test]
    fn thanks_in_advance_removed() {
        let r = sparse_coding::run("Thanks in advance for your help.", "balanced");
        assert!(!r.text.to_lowercase().contains("thanks in advance"));
    }
}

#[cfg(test)]
mod chunking_tests {
    use token_compress_engine::engine::chunking;

    #[test]
    fn paragraph_split_for_legal() {
        let r = chunking::run("A.\n\nB.\n\nC.", "legal");
        assert_eq!(r.chunks.len(), 3);
    }

    #[test]
    fn newline_split_for_ticket() {
        let r = chunking::run("T1\nT2\nT3", "ticket");
        assert_eq!(r.chunks.len(), 3);
    }

    #[test]
    fn sentence_cluster_generic() {
        let r = chunking::run("One. Two. Three. Four. Five. Six.", "generic");
        assert_eq!(r.chunks.len(), 2);
    }
}

#[cfg(test)]
mod selective_attention_tests {
    use token_compress_engine::engine::selective_attention;

    #[test]
    fn keeps_relevant() {
        let chunks = vec![
            "payment overdue invoice".into(),
            "weather today sunny".into(),
        ];
        let r = selective_attention::run(&chunks, "find overdue payment", "balanced", &[]);
        assert!(r.chunks.iter().any(|c| c.contains("overdue")));
    }

    #[test]
    fn protects_entity_regardless_of_score() {
        let chunks = vec!["TXN-9921 amount 1499".into(), "unrelated content".into()];
        let r = selective_attention::run(
            &chunks,
            "quantum physics",
            "aggressive",
            &["TXN-9921".into()],
        );
        assert!(r.chunks.iter().any(|c| c.contains("TXN-9921")));
    }

    #[test]
    fn fallback_never_empty() {
        let chunks = vec!["xyz irrelevant".into()];
        let r = selective_attention::run(&chunks, "quantum", "aggressive", &[]);
        assert!(!r.chunks.is_empty());
    }
}

#[cfg(test)]
mod working_memory_tests {
    use token_compress_engine::engine::working_memory;

    #[test]
    fn deduplicates() {
        let r = working_memory::run("Use JSON.\nUse JSON.\nBe concise.", 800);
        assert_eq!(r.text.matches("Use JSON.").count(), 1);
    }

    #[test]
    fn normalizes_whitespace() {
        let r = working_memory::run("Hello   world  foo.", 800);
        assert!(!r.text.contains("  "));
    }

    #[test]
    fn truncates_to_budget() {
        let long = vec!["word"; 2000].join(" ");
        let r = working_memory::run(&long, 100);
        assert!(r.text.split_whitespace().count() <= 80);
    }

    #[test]
    fn truncates_body_after_separator_not_headers() {
        let header = [
            "Role: analyst",
            "Audience: expert reader",
            "Schema: X",
            "Constraints: Y",
            "Synthesis: Z",
            "Task: summarize",
            "---",
        ]
        .join("\n");

        let body = vec!["word"; 200].join(" ");
        let text = format!("{}\n\n{}", header, body);

        let max_tokens = 100;
        let expected_word_limit = (max_tokens as f64 / 1.3) as usize;
        let r = working_memory::run(&text, max_tokens);

        // Count only words after the `---` separator.
        let mut after_sep = false;
        let mut body_words = 0usize;
        for line in r.text.lines() {
            if line.trim() == "---" {
                after_sep = true;
                continue;
            }
            if after_sep {
                body_words += line.split_whitespace().count();
            }
        }

        assert!(body_words <= expected_word_limit);
        // Header must remain present.
        assert!(r.text.contains("Role:"));
        assert!(r.text.contains("Audience:"));
    }
}

#[cfg(test)]
mod predictive_coding_tests {
    use token_compress_engine::engine::predictive_coding;

    #[test]
    fn includes_role_frame_details_and_task() {
        let chunks = vec!["Content about payment overdue invoices.".to_string()];
        let r = predictive_coding::run(&chunks, "summarize", "", "", "", "generic", "Claude");

        assert!(r.text.contains("Role:"));
        assert!(r.text.contains("Audience:"));
        assert!(r.text.contains("Task:"));
        assert!(r.text.contains("---"));

        // Schema, Constraints, and Synthesis should NOT be present.
        assert!(!r.text.contains("Schema:"));
        assert!(!r.text.contains("Constraints:"));
        assert!(!r.text.contains("Synthesis:"));

        // Compact formatting should not contain empty-line gaps.
        assert!(!r.text.contains("\n\n"));
    }

    #[test]
    fn rewrites_task_verbs() {
        let r = predictive_coding::run(
            &vec!["Content.".to_string()],
            "Compare and contrast the two frameworks",
            "",
            "",
            "",
            "generic",
            "Claude",
        );
        assert!(!r.text.to_lowercase().contains("compare and contrast"));
        assert!(r.text.contains("Task: map the two frameworks"));
    }
}
