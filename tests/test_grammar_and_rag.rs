#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

use token_compress_engine::npae::aggressive::grammar::GrammarCorrector;
use token_compress_engine::npae::aggressive::rag_context::DerivedRagContext;
use token_compress_engine::types::{ChunkMetadata, RagChunk};

#[test]
fn test_grammar_corrector_preserves_numbers_dates_units() {
    let raw = "how i can scale my pharma startup with $12M series A in Q3 on 2026-10-03 for 18-month timeline with 10.5% growth";
    let (corrected, _edits) = GrammarCorrector::correct(raw);

    // Protected patterns must be 100% preserved
    assert!(corrected.contains("$12M"), "USD currency must be preserved");
    assert!(corrected.contains("Q3"), "Quarter must be preserved");
    assert!(
        corrected.contains("2026-10-03"),
        "ISO date must be preserved"
    );
    assert!(
        corrected.contains("18-month"),
        "Duration unit must be preserved"
    );
    assert!(corrected.contains("10.5%"), "Percentage must be preserved");
}

#[test]
fn test_derived_rag_context_facts_and_deduplication() {
    let chunks = vec![
        RagChunk {
            domain_tag: "pharma".into(),
            content: "[Source: company_plan.pdf, Page 4]\nSeries A closed at $12M. Phase II trial for XR-21 starts in Q3. Manufacturing is outsourced.".into(),
            metadata: Some(ChunkMetadata { page_number: Some(4), source_file: "company_plan.pdf".into() }),
        },
        RagChunk {
            domain_tag: "pharma".into(),
            content: "[Source: company_plan.pdf, Page 5]\nSeries A closed at $12M. Phase II trial for XR-21 starts in Q3. CDMO agreement spans 18-month duration.".into(),
            metadata: Some(ChunkMetadata { page_number: Some(5), source_file: "company_plan.pdf".into() }),
        }
    ];

    let derived = DerivedRagContext::derive(&chunks, "how do I scale my pharma startup?", 250);

    // No raw citation tags in output facts
    for fact in &derived.facts {
        assert!(
            !fact.contains("[Source:"),
            "Facts must not contain source citation headers"
        );
    }

    // Deduplication should merge exact repeating sentences from the overlapping chunks
    let series_a_count = derived
        .facts
        .iter()
        .filter(|f| f.contains("Series A closed at $12M"))
        .count();
    assert_eq!(
        series_a_count, 1,
        "Duplicate chunk overlap sentence must be deduplicated to 1 instance"
    );

    // Gap coverage
    assert!(
        derived.covered_gaps.contains("budget"),
        "Budget gap zone must be flagged as covered by RAG facts"
    );
    assert!(
        derived.covered_gaps.contains("timeline"),
        "Timeline gap zone must be flagged as covered by RAG facts"
    );
}

#[test]
fn test_g03_derived_rag_context_preserves_multi_chunk_provenance() {
    let chunks = vec![
        RagChunk {
            domain_tag: "finance".into(),
            content: "Unrelated marketing overview about office snacks.".into(),
            metadata: Some(ChunkMetadata {
                page_number: Some(1),
                source_file: "brochure.pdf".into(),
            }),
        },
        RagChunk {
            domain_tag: "finance".into(),
            content: "Q3 enterprise revenue reached $4.2M with 18% operating margin. Annual recurring revenue grew to $16.8M across 420 enterprise accounts.".into(),
            metadata: Some(ChunkMetadata {
                page_number: Some(7),
                source_file: "q3_audit_report.pdf".into(),
            }),
        },
        RagChunk {
            domain_tag: "legal".into(),
            content: "Master SLA guarantees 99.95% monthly uptime under EU GDPR compliance.".into(),
            metadata: Some(ChunkMetadata {
                page_number: Some(12),
                source_file: "master_sla.pdf".into(),
            }),
        },
    ];

    let derived = DerivedRagContext::derive(
        &chunks,
        "What was Q3 enterprise revenue, annual recurring revenue, and master SLA monthly uptime?",
        250,
    );

    assert!(
        !derived.detailed_facts.is_empty(),
        "G-03: detailed_facts must be populated with provenance metadata"
    );
    assert_eq!(
        derived.facts.len(),
        derived.detailed_facts.len(),
        "G-03: facts and detailed_facts must remain 1:1 aligned"
    );

    for df in &derived.detailed_facts {
        if df.text.contains("$4.2M") || df.text.contains("$16.8M") {
            assert_eq!(
                df.source_file, "q3_audit_report.pdf",
                "G-03: Fact '{}' must preserve exact source_file 'q3_audit_report.pdf'",
                df.text
            );
            assert_eq!(
                df.page_number,
                Some(7),
                "G-03: Fact '{}' must preserve exact page_number Some(7)",
                df.text
            );
        } else if df.text.contains("99.95%") {
            assert_eq!(
                df.source_file, "master_sla.pdf",
                "G-03: Fact '{}' must preserve exact source_file 'master_sla.pdf'",
                df.text
            );
            assert_eq!(
                df.page_number,
                Some(12),
                "G-03: Fact '{}' must preserve exact page_number Some(12)",
                df.text
            );
        }
    }
}
