//! Shared `Oryx*` foundational utilities across `token_compress_engine`.
//!
//! // ARCH: ARCHITECTURE.md §6 — Canonical single-source utilities for hashing, vector math,
//! // XML/injection security, poison-safe sync guards, boundary-safe text matching, and token estimation.

pub mod hash;
pub mod math;
pub mod security;
pub mod sync;
pub mod text;
pub mod tokens;

pub use hash::OryxHasher;
pub use math::OryxVectorMath;
pub use security::OryxSecuritySanitizer;
pub use sync::OryxSyncGuard;
pub use text::OryxTextMatcher;
pub use tokens::OryxTokenEstimator;
