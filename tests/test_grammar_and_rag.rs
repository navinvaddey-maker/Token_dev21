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
