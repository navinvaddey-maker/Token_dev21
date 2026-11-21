//! User data models and authentication requests/responses.
//!
//! Implements user authentication contracts, credential exchange,
//! and multi-tenant user representations per ARCHITECTURE.md §2 and nrule.md §5.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Default role assigned to standard users upon registration.
fn default_role() -> String {
    "user".to_string()
}

/// Default tenant ID assigned to standard single-tenant or default-workspace users.
fn default_tenant_id() -> String {
    "default".to_string()
}

/// User entity representing a registered user account in the database.
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub email: String,
    pub business_type: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_tenant_id")]
    pub tenant_id: String,
    pub license: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Request payload for user registration.
#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
    pub business_type: Option<String>,
}

impl std::fmt::Debug for RegisterRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("email", &self.email)
            .field("business_type", &self.business_type)
            .finish()
    }
}

/// Request payload for user authentication.
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Response payload containing authentication token and user profile metadata.
#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub business_type: String,
    pub role: String,
    pub tenant_id: String,
}

impl std::fmt::Debug for LoginResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginResponse")
            .field("token", &"[REDACTED]")
            .field("user_id", &self.user_id)
            .field("username", &self.username)
            .field("business_type", &self.business_type)
            .field("role", &self.role)
            .field("tenant_id", &self.tenant_id)
            .finish()
    }
}
