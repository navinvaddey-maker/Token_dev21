---
trigger: glob
globs: "src/**/*.rs,crates/**/*.rs,static/**/*.js"
---

# Shared Utilities Rule (`Oryx*` Standard)

When implementing or modifying utilities across the engine, always reuse established shared helpers instead of reimplementing them.

## Key Shared Helpers & Gotchas

- **`OryxTokenEstimator::estimate_bpe_tokens`** (`crate::utils::tokens::estimate_tokens`)
  - Mandatory for all token budgets & TES comparisons. Never multiply raw words by `1.3` inline or mix raw word counts with BPE estimates.
- **`OryxTextMatcher`** (`crate::utils::text`)
  - `contains_whole_word_or_phrase` (prevents `"app"` matching `"appraisal"` / `"earn"` matching `"learn"`).
  - `contains_identifier_bounded_phrase` (rejects `_` boundaries).
  - `tokenize_alphanumeric_words` + `contains_contiguous_token_span` (multi-word constraint matching).
  - `format_slug_as_title_case`.
  - `split_sentences_preserving_decimals` (preserves `$4.2M` / `99.95%`).
  - `compute_jaccard_token_similarity`.
- **`OryxVectorMath`** (`crate::utils::math`)
  - `normalize_l2_in_place`.
  - `compute_cosine_similarity`.
  - `compute_unit_dot_similarity` (for 384-dim unit vectors).
  - `compute_normalized_rrf_score` (`RRF_SMOOTHING_K = 60.0`).
- **`OryxHasher`** (`crate::utils::hash`)
  - `compute_sha256_hex_digest`, `compute_fnv1a_primary_u64`, `compute_fnv1a_secondary_u64`.
  - **Never** use `std::collections::hash_map::DefaultHasher` for fingerprints, caches, or DB keys (non-deterministic across restarts).
- **`OryxSecuritySanitizer`** (`crate::utils::security`)
  - `escape_untrusted_xml_content`.
  - `unescape_xml_entities` (unescapes `&amp;` last to prevent double-unescape).
  - `has_prompt_injection_markers`.
  - Mandatory before placing any RAG chunk, filename, slot, or user fact inside `<facts>`, `<context>`, or `<retrieved_context>`.
- **`OryxSyncGuard`** (`crate::utils::sync`)
  - `lock_mutex_recovering_poison`, `read_rwlock_recovering_poison`, `write_rwlock_recovering_poison`.
  - `insert_into_bounded_dashmap` (drops `DashMap::Iter` shard guard before `.remove()` to prevent shard deadlocks).
- **`crate::types::RagChunk::from(SearchResult)`**
  - Canonical conversion from `rag::types::SearchResult`.
- **`OryxDomUtils.escapeHtmlEntities`** (`escapeHtml` alias in `static/*.js`)
  - Mandatory before interpolating any untrusted string into HTML.
