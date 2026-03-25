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
        assert!(sparse_coding::run(text, "aggressive").items_removed
             >= sparse_coding::run(text, "gentle").items_removed);
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
        let r = selective_attention::run(&chunks, "quantum physics", "aggressive", &["TXN-9921".into()]);
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
        let r    = working_memory::run(&long, 100);
        assert!(r.text.split_whitespace().count() <= 80);
    }
}

#[cfg(test)]
mod hebbian_binding_tests {
    use token_compress_engine::engine::hebbian_binding;

    #[test]
    fn crisp_order_preserved() {
        let r = hebbian_binding::run("Role: analyst.\n\n---\n\nContent.", "summarize", "generic");
        let role_pos = r.text.find("Role:").unwrap();
        let task_pos = r.text.find("Task:").unwrap();
        let div_pos  = r.text.find("---").unwrap();
        assert!(role_pos < task_pos && task_pos < div_pos);
    }

    #[test]
    fn output_format_injected() {
        let r = hebbian_binding::run("Role: x.\n\n---\n\nContent.", "x", "ticket");
        assert!(r.text.contains("Output:"));
    }
}
