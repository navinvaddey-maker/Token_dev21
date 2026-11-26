//! Database repository and multi-tenant persistence layer.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules, §8 Vector DB Rules.

pub mod analytics;
pub mod feedback;
pub mod history;
pub mod users;

pub use users::DeleteUserOutcome;

/// Central data repository providing CRUD operations for users, token history,
/// feedback signals, use cases, and tenant-scoped analytics.
#[derive(Debug)]
pub struct Repository;
