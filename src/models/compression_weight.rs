use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct CompressionWeight {
    pub id:         String,
    pub user_id:    String,
    pub entity:     String,
    pub weight:     f64,
    pub use_case:   String,
    pub updated_at: String,
}
