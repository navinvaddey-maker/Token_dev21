//! Stateful Parent → Continuation subsystem for multi-turn prompt evolution.
//!
//! // ARCH: ARCHITECTURE.md §6.7 — provides deterministic parent-delta matching (`diff`),
//! // continuation intent & constraint-operation routing (`router`), and persistent
//! // turn-by-turn state tracking (`ledger` with `SL-2` domain hysteresis and `SL-3`
//! // monotonic SHA-256 deduplicated `[F#]` facts).

pub mod diff;
pub mod ledger;
pub mod router;

pub use diff::{ContinuationPath, DeltaExtraction, ParentMatcher};
pub use ledger::{
    ConstraintEntry, ConstraintKind, ConstraintStatus, FactEntry, LockedAxis, PhaseEntry,
    PhaseStatus, PromptStateLedger,
};
pub use router::{ConstraintOp, ContinuationKind, ContinuationRoute, ContinuationRouter};
