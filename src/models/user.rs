use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct User {
    pub id:            String,
    pub username:      String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub email:         String,
    pub business_type: String,
    pub license:       Option<String>,
    pub created_at:    String,
    pub updated_at:    String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username:      String,
    pub password:      String,
    pub email:         String,
    pub business_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token:    String,
    pub user_id:  String,
    pub username: String,
}
