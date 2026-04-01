use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct UseCase {
    pub id: String,
    pub key: String,
    pub version: String,
    pub role_frame: String,
    pub output_format: String,
    pub chunk_strategy: String,
    pub description: Option<String>,
    pub active: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
