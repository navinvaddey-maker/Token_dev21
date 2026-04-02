use bcrypt::{hash, verify, DEFAULT_COST};
use db::DbPool;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data::Repository,
    engine::pipeline::{self, PipelineInput},
    errors::AppError,
    models::{
        feedback_signal::ExplicitFeedback,
        token_history::TokenHistory,
        user::{LoginRequest, LoginResponse, RegisterRequest, User},
    },
    AppState,
};

const ENGINE_VERSION: &str = "1.0.0";
const JWT_SECRET: &str = "change_this_in_production";

// ── JWT ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

pub fn create_jwt(user_id: &str) -> Result<String, AppError> {
    let claims = Claims {
        sub: user_id.to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .map_err(|_| AppError::Internal("JWT signing failed".into()))
}

pub fn verify_jwt(token: &str) -> Result<String, AppError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims.sub)
}

// ── User auth ──────────────────────────────────────────────────────────────

pub async fn register_user(pool: &DbPool, req: RegisterRequest) -> Result<User, AppError> {
    let id = Uuid::new_v4().to_string();
    let pw_hash = hash(&req.password, DEFAULT_COST)
        .map_err(|_| AppError::Internal("bcrypt failed".into()))?;
    Repository::create_user(
        pool,
        &id,
        &req.username,
        &pw_hash,
        &req.email,
        req.business_type.as_deref().unwrap_or("Developer"),
    )
    .await
    .map_err(AppError::Database)
}

pub async fn login_user(pool: &DbPool, req: LoginRequest) -> Result<LoginResponse, AppError> {
    let user = Repository::find_user_by_username(pool, &req.username)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::InvalidCredentials)?;
    if !verify(&req.password, &user.password_hash).unwrap_or(false) {
        return Err(AppError::InvalidCredentials);
    }
    let token = create_jwt(&user.id)?;
    Ok(LoginResponse {
        token,
        user_id: user.id,
        username: user.username,
        business_type: user.business_type,
    })
}

// ── Compression ────────────────────────────────────────────────────────────

pub async fn compress_new(
    state: &AppState,
    user_id_str: &str,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    let user_id =
        Uuid::parse_str(user_id_str).map_err(|_| AppError::Internal("Invalid user_id".into()))?;

    // 1. Process via Learning Engine (Hebbian + Competitive)
    let engine_result = {
        let mut engine = state.engine.lock().await;
        engine
            .process(user_id, &req.raw_text)
            .await
            .map_err(|e| AppError::Database(e))?
    };

    if engine_result.blocked {
        return Err(AppError::Engine("Prompt blocked by constraints".into()));
    }

    // 2. Pre-load domain vocabulary for prompt enrichment
    let context_prefix = engine_result
        .cluster_vocab
        .iter()
        .map(|(token, _): &(String, f32)| token.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let enriched_prompt = if !context_prefix.is_empty() {
        format!("Context: [{}]\n\n{}", context_prefix, req.raw_text)
    } else {
        req.raw_text.clone()
    };

    // 3. Run existing CRISP pipeline
    let use_case = req.use_case.unwrap_or_else(|| "auto".into());
    let mode = req.mode.unwrap_or_else(|| "balanced".into());
    let model = req.model.unwrap_or_else(|| "Claude".into());
    let max_tokens = req.max_tokens.unwrap_or(800);
    // Use the explicit task if provided, otherwise leave it empty for the engine to auto-infer
    let task = req.task.unwrap_or_default();

    let deliverables = req.deliverables.unwrap_or_default();
    let constraints = req.constraints.unwrap_or_default();
    let reproducibility = req.reproducibility.unwrap_or_default();

    let output = pipeline::run(&PipelineInput {
        raw_text: enriched_prompt.clone(),
        task,
        deliverables,
        constraints,
        reproducibility,
        model,
        use_case: use_case.clone(),
        mode: mode.clone(),
        max_tokens,
        engine_version: ENGINE_VERSION.to_string(),
        protected_entities: vec![], // Learning engine handles patterns now
    })
    .map_err(|e| AppError::Engine(e.to_string()))?;

    // 4. Persist to DB
    let history_id = Uuid::new_v4().to_string();
    let mut verbose = crate::models::verbose::SqlxVerbose::default();
    Repository::insert_history_verbose(
        &state.pool,
        &history_id,
        user_id_str,
        &enriched_prompt,
        &output,
        &mut verbose,
    )
    .await
    .map_err(AppError::Database)?;

    Ok(serde_json::json!({
        "id":               history_id,
        "optimized_prompt": output.optimized_prompt,
        "token_original":   output.token_original,
        "token_final":      output.token_final,
        "token_saved":      output.token_saved,
        "warnings":         output.warnings,
        "evaluation":       output.evaluation,
        "engine_version":   ENGINE_VERSION,
        "cluster_id":       engine_result.cluster_id,
        "is_new_domain":    engine_result.is_new_domain,
    }))
}

pub async fn get_history_record(
    pool: &DbPool,
    user_id: &str,
    id: &str,
) -> Result<TokenHistory, AppError> {
    Repository::get_history(pool, user_id, id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::NotFound(id.into()))
}

pub async fn list_history(pool: &DbPool, user_id: &str) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history(pool, user_id, 50)
        .await
        .map_err(AppError::Database)
}

// ── Feedback ───────────────────────────────────────────────────────────────

pub async fn record_explicit_feedback(
    pool: &DbPool,
    user_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    let id = Uuid::new_v4().to_string();
    let sig = crate::models::feedback_signal::FeedbackSignal {
        id,
        user_id: user_id.to_string(),
        history_id: req.history_id.clone(),
        signal_type: req.rating.clone(),
        signal_layer: 5,
        value: if req.rating == "thumbs_up" { 1.0 } else { -1.0 },
        meta: serde_json::json!({}),
        detected_at: chrono::Utc::now(),
    };
    Repository::insert_feedback(pool, &sig)
        .await
        .map_err(AppError::Database)?;
    Ok(())
}

// ── Dev config ─────────────────────────────────────────────────────────────

pub async fn get_dev_config(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    let use_cases = Repository::list_use_cases(pool)
        .await
        .map_err(AppError::Database)?;
    Ok(serde_json::json!({
        "engine_version": ENGINE_VERSION,
        "use_cases":      use_cases,
        "modes":          ["gentle", "balanced", "aggressive"],
    }))
}

pub async fn get_stats(pool: &DbPool, user_id: &str) -> Result<serde_json::Value, AppError> {
    let stats = Repository::get_user_stats(pool, user_id)
        .await
        .map_err(AppError::Database)?;
    Ok(serde_json::json!(stats))
}

// ── Admin Domain ───────────────────────────────────────────────────────────

pub async fn is_admin(pool: &DbPool, user_id: &str) -> Result<bool, AppError> {
    let user = Repository::find_user_by_id(pool, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;
    Ok(user.business_type.to_lowercase() == "admin")
}

pub async fn admin_list_users(pool: &DbPool) -> Result<Vec<User>, AppError> {
    Repository::list_all_users(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_delete_user(pool: &DbPool, id: &str) -> Result<(), AppError> {
    Repository::delete_user(pool, id)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_update_user(
    pool: &DbPool,
    id: &str,
    business_type: &str,
) -> Result<(), AppError> {
    Repository::update_user_business_type(pool, id, business_type)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_stats(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_global_stats(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_list_history(pool: &DbPool) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_all_history(pool, 100)
        .await
        .map_err(AppError::Database)
}

// ── Admin Monitoring ───────────────────────────────────────────────────────────

pub async fn admin_get_system_performance(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_system_performance(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_learning_engine_stats(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_learning_engine_stats(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_compression_analytics(
    pool: &DbPool,
    time_window: Option<String>,
) -> Result<serde_json::Value, AppError> {
    Repository::get_compression_analytics(pool, time_window)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_user_engagement_stats(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_user_engagement_stats(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_feedback_analysis(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_feedback_analysis(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_error_metrics(pool: &DbPool) -> Result<serde_json::Value, AppError> {
    Repository::get_error_metrics(pool)
        .await
        .map_err(AppError::Database)
}
