use bcrypt::{hash, verify, DEFAULT_COST};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    behavior::weight_matrix,
    data::Repository,
    engine::pipeline::{self, PipelineInput},
    errors::AppError,
    models::{
        feedback_signal::ExplicitFeedback,
        token_history::TokenHistory,
        user::{LoginRequest, LoginResponse, RegisterRequest, User},
    },
};

const ENGINE_VERSION: &str = "1.0.0";
const JWT_SECRET: &str     = "change_this_in_production";

// ── JWT ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct Claims { sub: String, exp: usize }

pub fn create_jwt(user_id: &str) -> Result<String, AppError> {
    let claims = Claims {
        sub: user_id.to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(JWT_SECRET.as_bytes()))
        .map_err(|_| AppError::Internal("JWT signing failed".into()))
}

pub fn verify_jwt(token: &str) -> Result<String, AppError> {
    let data = decode::<Claims>(token, &DecodingKey::from_secret(JWT_SECRET.as_bytes()), &Validation::default())
        .map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims.sub)
}

// ── User auth ──────────────────────────────────────────────────────────────

pub async fn register_user(pool: &SqlitePool, req: RegisterRequest) -> Result<User, AppError> {
    let id   = Uuid::new_v4().to_string();
    let pw_hash = hash(&req.password, DEFAULT_COST).map_err(|_| AppError::Internal("bcrypt failed".into()))?;
    Repository::create_user(pool, &id, &req.username, &pw_hash, &req.email, req.business_type.as_deref().unwrap_or("Developer")).await
        .map_err(AppError::Database)
}

pub async fn login_user(pool: &SqlitePool, req: LoginRequest) -> Result<LoginResponse, AppError> {
    let user = Repository::find_user_by_username(pool, &req.username).await
        .map_err(AppError::Database)?
        .ok_or(AppError::InvalidCredentials)?;
    if !verify(&req.password, &user.password_hash).unwrap_or(false) {
        return Err(AppError::InvalidCredentials);
    }
    let token = create_jwt(&user.id)?;
    Ok(LoginResponse { token, user_id: user.id, username: user.username })
}

// ── Compression ────────────────────────────────────────────────────────────

pub async fn compress(
    pool:    &SqlitePool,
    user_id: &str,
    req:     crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    let use_case   = req.use_case.unwrap_or_else(|| "generic".into());
    let mode       = req.mode.unwrap_or_else(|| "balanced".into());
    let max_tokens = req.max_tokens.unwrap_or(800);

    // fetch user's protected entities from weight matrix
    let protected = weight_matrix::get_protected_entities(pool, user_id, &use_case)
        .await.map_err(AppError::Database)?;

    let output = pipeline::run(&PipelineInput {
        raw_text:       req.raw_text.clone(),
        task:           req.task.clone(),
        use_case:       use_case.clone(),
        mode:           mode.clone(),
        max_tokens,
        engine_version: ENGINE_VERSION.into(),
        protected_entities: protected,
    })
    .map_err(|e| AppError::Engine(e.to_string()))?;

    // persist to history
    let id = Uuid::new_v4().to_string();
    Repository::insert_history(pool, &id, user_id, &req.raw_text, &output).await
        .map_err(AppError::Database)?;

    Ok(serde_json::json!({
        "id":               id,
        "optimized_prompt": output.optimized_prompt,
        "token_original":   output.token_original,
        "token_final":      output.token_final,
        "token_saved":      output.token_saved,
        "warnings":         output.warnings,
        "engine_version":   ENGINE_VERSION,
    }))
}

pub async fn get_history_record(pool: &SqlitePool, user_id: &str, id: &str) -> Result<TokenHistory, AppError> {
    Repository::get_history(pool, user_id, id).await
        .map_err(AppError::Database)?
        .ok_or(AppError::NotFound(id.into()))
}

pub async fn list_history(pool: &SqlitePool, user_id: &str) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history(pool, user_id, 50).await.map_err(AppError::Database)
}

// ── Feedback ───────────────────────────────────────────────────────────────

pub async fn record_explicit_feedback(
    pool:    &SqlitePool,
    user_id: &str,
    req:     ExplicitFeedback,
) -> Result<(), AppError> {
    let id = Uuid::new_v4().to_string();
    let sig = crate::models::feedback_signal::FeedbackSignal {
        id,
        user_id:      user_id.to_string(),
        history_id:   req.history_id.clone(),
        signal_type:  req.rating.clone(),
        signal_layer: 5,
        value:        if req.rating == "thumbs_up" { 1.0 } else { -1.0 },
        meta:         "{}".into(),
        detected_at:  chrono::Utc::now().to_rfc3339(),
    };
    Repository::insert_feedback(pool, &sig).await.map_err(AppError::Database)?;
    // adjust weights using explicit feedback
    weight_matrix::adjust(pool, user_id, "generic", &[], &req.rating).await
        .map_err(AppError::Database)
}

// ── Dev config ─────────────────────────────────────────────────────────────

pub async fn get_dev_config(pool: &SqlitePool) -> Result<serde_json::Value, AppError> {
    let use_cases = Repository::list_use_cases(pool).await.map_err(AppError::Database)?;
    Ok(serde_json::json!({
        "engine_version": ENGINE_VERSION,
        "use_cases":      use_cases,
        "modes":          ["gentle", "balanced", "aggressive"],
    }))
}

pub async fn get_stats(pool: &SqlitePool, user_id: &str) -> Result<serde_json::Value, AppError> {
    let stats = Repository::get_user_stats(pool, user_id).await.map_err(AppError::Database)?;
    Ok(serde_json::json!(stats))
}
