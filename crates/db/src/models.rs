use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct UserPrompt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub raw_text: String,
    pub schema_json: serde_json::Value,
    pub token_count: i32,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, FromRow)]
pub struct LearnedContext {
    pub id: Uuid,
    pub user_id: Uuid,
    pub domain_vocab: Vec<String>,
    pub top_pairs: serde_json::Value,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, FromRow)]
pub struct UserDomainProfile {
    pub id: Uuid,
    pub user_id: Uuid,
    pub cluster_id: i32,
    pub cluster_label: Option<String>,
    pub top_tokens: serde_json::Value,
    pub hits: i32,
    pub updated_at: OffsetDateTime,
}
