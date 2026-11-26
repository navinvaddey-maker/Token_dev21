//! Domain authentication, JWT issuance/verification, and user registration/login.
//!
//! // ARCH: ARCHITECTURE.md §2 Component Map, §7 sqlx Rules — Auth boundary and RBAC claims.

use bcrypt::{hash, verify, DEFAULT_COST};
use db::DbPool;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data::Repository,
    errors::AppError,
    models::user::{LoginRequest, LoginResponse, RegisterRequest, User},
};

/// Allowed non-privileged business types for public user registration.
pub(crate) const ALLOWED_BUSINESS_TYPES: &[&str] = &[
    "developer",
    "business",
    "researcher",
    "legal",
    "finance",
    "pro",
    "enterprise",
];

/// Validates that security environment variables satisfy minimum security constraints.
///
/// # Errors
/// Returns an error string if `JWT_SECRET` is unset or fewer than 32 bytes.
///
/// # Panics
/// Never.
pub fn validate_security_env() -> Result<(), String> {
    match std::env::var("JWT_SECRET") {
        Ok(secret) if secret.len() >= 32 => Ok(()),
        Ok(_) => Err("JWT_SECRET environment variable must be at least 32 bytes".to_string()),
        Err(_) => Err("JWT_SECRET environment variable is not set".to_string()),
    }
}

#[cfg(not(test))]
static JWT_CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Reads and validates `JWT_SECRET` from environment variables without hardcoded fallbacks.
/// Caches the secret in non-test production builds for performance.
///
/// # Errors
/// Returns [`AppError::Internal`] if missing or fewer than 32 bytes.
///
/// # Panics
/// Never.
pub fn jwt_secret() -> Result<String, AppError> {
    #[cfg(not(test))]
    {
        if let Some(cached) = JWT_CACHE.get() {
            return Ok(cached.clone());
        }
    }

    let secret = std::env::var("JWT_SECRET").map_err(|_| {
        AppError::Internal("JWT_SECRET environment variable is not configured".into())
    })?;
    if secret.len() < 32 {
        return Err(AppError::Internal(
            "JWT_SECRET environment variable must be at least 32 bytes".into(),
        ));
    }

    #[cfg(not(test))]
    {
        let _ = JWT_CACHE.set(secret.clone());
    }

    Ok(secret)
}

/// Context extracted from verified JWT authentication tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Authenticated user UUID.
    pub user_id: String,
    /// Authenticated tenant identifier.
    pub tenant_id: String,
    /// Authoritative user role (`"user"` or `"admin"`).
    pub role: String,
}

/// Claims carried inside issued JWT tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject user UUID.
    pub sub: String,
    /// Tenant identifier claim.
    pub tenant_id: String,
    /// Role claim.
    pub role: String,
    /// Expiration timestamp (Unix epoch seconds).
    pub exp: usize,
    /// Token issuer (`"token_compress_engine"`).
    pub iss: String,
    /// Intended audience (`"tce_api"`).
    pub aud: String,
}

/// Issues a JWT token for a specific `user_id`, `tenant_id`, and `role` with 1-hour expiration.
///
/// # Errors
/// Returns [`AppError::Internal`] if signing fails or `JWT_SECRET` is misconfigured.
///
/// # Panics
/// Never.
pub fn create_jwt_for_user(user_id: &str, tenant_id: &str, role: &str) -> Result<String, AppError> {
    let secret = jwt_secret()?;
    let exp_secs = (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        tenant_id: tenant_id.to_string(),
        role: role.to_string(),
        exp: usize::try_from(exp_secs.max(0)).unwrap_or(usize::MAX),
        iss: "token_compress_engine".to_string(),
        aud: "tce_api".to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AppError::Internal("JWT signing failed".into()))
}

/// Backward-compatible JWT issuer delegating to `("default", "user")`.
///
/// # Errors
/// Returns [`AppError::Internal`] if signing fails.
///
/// # Panics
/// Never.
pub fn create_jwt(user_id: &str) -> Result<String, AppError> {
    create_jwt_for_user(user_id, "default", "user")
}

/// Verifies and decodes a JWT token into its typed [`AuthContext`].
///
/// # Errors
/// Returns [`AppError::Unauthorized`] on invalid, expired, or non-matching tokens.
///
/// # Panics
/// Never.
pub fn verify_jwt_claims(token: &str) -> Result<AuthContext, AppError> {
    let secret = jwt_secret()?;
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.set_issuer(&["token_compress_engine"]);
    validation.set_audience(&["tce_api"]);
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::Unauthorized)?;
    Ok(AuthContext {
        user_id: data.claims.sub,
        tenant_id: data.claims.tenant_id,
        role: data.claims.role,
    })
}

/// Backward-compatible JWT verification returning the subject user ID.
///
/// # Errors
/// Returns [`AppError::Unauthorized`] on invalid token.
///
/// # Panics
/// Never.
pub fn verify_jwt(token: &str) -> Result<String, AppError> {
    verify_jwt_claims(token).map(|ctx| ctx.user_id)
}

/// Registers a new user account with strict `business_type` validation and default role assignment.
///
/// # Errors
/// Returns [`AppError::Validation`] if input is invalid or `business_type` is unapproved/admin,
/// [`AppError::Internal`] on worker/bcrypt failure, or [`AppError::Database`] on DB failure.
///
/// # Panics
/// Never.
pub async fn register_user(pool: &DbPool, req: RegisterRequest) -> Result<User, AppError> {
    let username = req.username.trim();
    if username.len() < 3 || username.len() > 50 {
        return Err(AppError::Validation(
            "Username must be between 3 and 50 characters".into(),
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation(
            "Password must be at least 8 characters".into(),
        ));
    }
    let email = req.email.trim();
    if !email.contains('@') || !email.contains('.') || email.len() < 5 {
        return Err(AppError::Validation(
            "A valid email address is required".into(),
        ));
    }

    // Validate business_type against non-privileged allowlist; reject "admin" explicitly
    let b_type = req
        .business_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("developer");

    let b_type_lower = b_type.to_lowercase();
    if b_type_lower == "admin" || !ALLOWED_BUSINESS_TYPES.contains(&b_type_lower.as_str()) {
        return Err(AppError::Validation(
            "Invalid business type. Must be one of: developer, business, researcher, legal, finance, pro, enterprise".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let password_clone = req.password.clone();
    let pw_hash = tokio::task::spawn_blocking(move || hash(&password_clone, DEFAULT_COST))
        .await
        .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
        .map_err(|_| AppError::Internal("bcrypt failed".into()))?;

    // ARCH: §0 — Always insert standard users with role = "user" and tenant_id = "default"
    Repository::create_user_scoped(
        pool, &id, username, &pw_hash, email, b_type, "user", "default",
    )
    .await
    .map_err(AppError::Database)
}

const DUMMY_BCRYPT_HASH: &str = "$2b$12$CwTycUXWue0Thq9StjUM0uJ8hT8mXKfR3c8sM8r7u3T4gJ2wN5aOa";

/// Authenticates user credentials and issues a JWT token populated with user role and tenant.
/// Executes bcrypt verification in `spawn_blocking` and enforces constant-time checks
/// against a dummy bcrypt hash on non-existent users to eliminate timing attacks.
///
/// # Errors
/// Returns [`AppError::InvalidCredentials`] on bad username or password,
/// [`AppError::Database`] on DB failure, or [`AppError::Internal`] on worker/JWT failure.
///
/// # Panics
/// Never.
pub async fn login_user(pool: &DbPool, req: LoginRequest) -> Result<LoginResponse, AppError> {
    let maybe_user = Repository::find_user_by_username(pool, &req.username)
        .await
        .map_err(AppError::Database)?;

    let password = req.password;

    let (user_opt, hash_to_verify) = match maybe_user {
        Some(u) => {
            let h = u.password_hash.clone();
            (Some(u), h)
        }
        None => (None, DUMMY_BCRYPT_HASH.to_string()),
    };

    let valid =
        tokio::task::spawn_blocking(move || verify(&password, &hash_to_verify).unwrap_or(false))
            .await
            .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?;

    let user = match user_opt {
        Some(u) if valid => u,
        _ => return Err(AppError::InvalidCredentials),
    };

    let token = create_jwt_for_user(&user.id, &user.tenant_id, &user.role)?;
    Ok(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        business_type: user.business_type,
        role: user.role,
        tenant_id: user.tenant_id,
    })
}
