//! Deterministic grounding harness support and downstream LLM answer verification.
//!
//! Implements `ARCHITECTURE.md` §5 (`GAP-42`) and §7.2 (`Layer 4 — Grounding Harness`):
//! - Fact numbering, XML-safe untrusted fencing, and decimal-preserving sentence extraction
//!   used by `npae::aggressive::structurer::render_harness_prompt`.
//! - Deterministic, LLM-free verification of downstream LLM answers against supplied `[F#]`
//!   facts (`FOUND`, `PARTIAL_MATCH`, `NOT_FOUND`, citation validity, lexical overlap,
//!   verbatim numeric/date checks, and prompt-injection quarantine).

pub mod verifier;
