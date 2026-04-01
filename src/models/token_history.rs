use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct TokenHistory {
    pub id: String,
    pub user_id: String,
    pub original_prompt: String,
    pub optimized_prompt: String,
    pub tokens_saved: i64,
    pub token_original: i64,
    pub token_final: i64,
    pub use_case: String,
    pub mode: String,
    pub engine_version: String,
    #[serde(default)]
    pub principle_logs: serde_json::Value,
    #[serde(default)]
    pub warnings: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
