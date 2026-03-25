use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct TokenHistory {
    pub id:               String,
    pub user_id:          String,
    pub original_prompt:  String,
    pub optimized_prompt: String,
    pub tokens_saved:     i64,
    pub token_original:   i64,
    pub token_final:      i64,
    pub use_case:         String,
    pub mode:             String,
    pub engine_version:   String,
    pub principle_logs:   String,   // JSON string
    pub warnings:         String,   // JSON string
    pub created_at:       String,
}
