//! HTTP API Router, Authentication/RBAC Middleware, Rate Limiting & Validation Boundaries.
//!
//! ARCH: ARCHITECTURE.md §2 Full Component Map (`User → API (Axum) → Auth+RateLimit → Domain`).

use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use dashmap::DashMap;

use crate::{
    domain::{self, AuthContext},
    errors::AppError,
    models::user::{LoginRequest, RegisterRequest},
    npae::schema::types::{AggressiveRequest, HallucinationCheckRequest},
    AppState,
};

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_PROMPT_WORDS: usize = 8_000;
const MAX_PDF_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
const MAX_LOCAL_CHUNKS: usize = 10;
const MAX_LOCAL_CHUNK_BYTES: usize = 8 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 15;
const MAX_RATE_BUCKETS: usize = 50_000;

/// In-memory per-key token-bucket rate limiter (`DashMap`-backed, bounded capacity).
#[derive(Clone, Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

#[derive(Clone, Debug)]
struct RateLimiter {
    buckets: Arc<DashMap<String, TokenBucket>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            capacity,
            refill_per_sec,
        }
    }

    fn check(&self, key: &str) -> Result<(), AppError> {
        let now = Instant::now();
        if !self.buckets.contains_key(key) && self.buckets.len() >= MAX_RATE_BUCKETS {
            // Prune oldest/idle buckets first to bound memory at 1M-user scale.
            self.buckets
                .retain(|_, b| now.duration_since(b.last_refill).as_secs() < 300);
            if self.buckets.len() >= MAX_RATE_BUCKETS {
                let oldest_key = self
                    .buckets
                    .iter()
                    .min_by_key(|entry| entry.value().last_refill)
                    .map(|entry| entry.key().clone());
                if let Some(evict_key) = oldest_key {
                    self.buckets.remove(&evict_key);
                }
            }
        }

        let mut entry = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket {
                tokens: self.capacity,
                last_refill: now,
            });

        let elapsed = now.duration_since(entry.last_refill).as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        entry.last_refill = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            Ok(())
        } else {
            Err(AppError::TooManyRequests(
                "Rate limit exceeded; please retry shortly".into(),
            ))
        }
    }
}

static TRUSTED_PROXIES: std::sync::OnceLock<Vec<std::net::IpAddr>> = std::sync::OnceLock::new();

fn get_trusted_proxies() -> &'static [std::net::IpAddr] {
    TRUSTED_PROXIES.get_or_init(|| {
        std::env::var("TRUSTED_PROXIES")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<std::net::IpAddr>().ok())
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn extract_client_ip(headers: &HeaderMap, req_exts: &axum::http::Extensions) -> String {
    let peer_ip = req_exts
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());

    if let Some(peer_ip) = peer_ip {
        let trusted = get_trusted_proxies();
        if trusted.contains(&peer_ip) {
            if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
                if let Some(first) = xff
                    .split(',')
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    return first.to_string();
                }
            }
            if let Some(xri) = headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return xri.to_string();
            }
        }
        return peer_ip.to_string();
    }

    "unknown".to_string()
}

fn validate_prompt_bounds(text: &str) -> Result<(), AppError> {
    if text.trim().is_empty() {
        return Err(AppError::EmptyInput);
    }
    if text.len() > MAX_PROMPT_BYTES || text.split_whitespace().count() > MAX_PROMPT_WORDS {
        return Err(AppError::InputTooLarge);
    }
    Ok(())
}

/// Builds the Axum HTTP router with per-IP auth limits, per-tenant API limits, body caps, and RBAC.
pub fn router(state: AppState) -> Router {
    let auth_limiter = RateLimiter::new(15.0, 0.5);
    let user_limiter = RateLimiter::new(30.0, 2.0);

    let auth = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/demo/compress", post(demo_compress))
        .layer(middleware::from_fn_with_state(
            auth_limiter,
            auth_rate_limit_middleware,
        ));

    let public = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/rag/health", get(rag_health))
        .route("/api/scenario/registry", get(scenario_get_registry))
        .route("/api/v1/schema", get(npae_schema))
        .route("/api/v1/health", get(npae_health))
        .merge(auth);

    let rag_upload_route = Router::new()
        .route("/api/rag/upload", post(rag_upload))
        .layer(DefaultBodyLimit::max(
            MAX_PDF_UPLOAD_BYTES + 2 * 1024 * 1024,
        ));

    let protected = Router::new()
        .route("/api/compress", post(compress))
        .route("/api/tokens/:id/decompress", get(decompress))
        .route("/api/history", get(history))
        .route("/api/feedback", post(feedback))
        .route("/api/dev/config", get(dev_config))
        .route("/api/stats", get(stats))
        .route("/api/scenario/ask", post(scenario_ask))
        .route("/api/rag/documents", get(rag_list_documents))
        .route(
            "/api/rag/documents/:id",
            get(rag_get_document).delete(rag_delete_document),
        )
        .route("/api/rag/search", get(rag_search))
        .route("/api/v1/compress", post(npae_compress))
        .route("/api/v1/aggressive", post(npae_aggressive))
        .route("/api/npae/verify", post(npae_verify_grounding))
        .route(
            "/api/v1/hallucination-check",
            post(npae_hallucination_check),
        )
        .merge(rag_upload_route)
        .layer(middleware::from_fn_with_state(
            user_limiter.clone(),
            user_rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let admin_rag_upload_route = Router::new()
        .route("/api/admin/rag/upload", post(admin_rag_upload))
        .layer(DefaultBodyLimit::max(
            MAX_PDF_UPLOAD_BYTES + 2 * 1024 * 1024,
        ));

    let admin = Router::new()
        .route("/api/admin/stats", get(admin_stats))
        .route("/api/admin/users", get(admin_users))
        .route(
            "/api/admin/users/:id",
            put(admin_update_user).delete(admin_delete_user),
        )
        .route("/api/admin/history", get(admin_history))
        .route(
            "/api/admin/system-performance",
            get(admin_get_system_performance),
        )
        .route(
            "/api/admin/learning-engine-stats",
            get(admin_get_learning_engine_stats),
        )
        .route(
            "/api/admin/compression-analytics",
            get(admin_get_compression_analytics),
        )
        .route(
            "/api/admin/user-engagement",
            get(admin_get_user_engagement_stats),
        )
        .route(
            "/api/admin/feedback-analysis",
            get(admin_get_feedback_analysis),
        )
        .route("/api/admin/error-metrics", get(admin_get_error_metrics))
        .route("/api/admin/rag/documents", get(admin_rag_list_documents))
        .route(
            "/api/admin/rag/documents/:id",
            delete(admin_rag_delete_document),
        )
        .merge(admin_rag_upload_route)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            user_limiter,
            user_rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin)
        .layer(middleware::from_fn(global_timeout_middleware))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}

async fn global_timeout_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    tokio::time::timeout(std::time::Duration::from_secs(60), next.run(req))
        .await
        .map_err(|_| AppError::Timeout("Request timed out".into()))
}

/// Injects standard defense-in-depth HTTP security headers into all responses.
pub async fn security_headers_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue};
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' https://cdn.jsdelivr.net 'unsafe-inline'; style-src 'self' https://fonts.googleapis.com 'unsafe-inline'; font-src 'self' https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self' http://127.0.0.1:49152; frame-ancestors 'none'; object-src 'none'",
        ),
    );
    resp
}

// ── Rate-Limit & Auth Middlewares ──────────────────────────────────────────

async fn auth_rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let ip = extract_client_ip(req.headers(), req.extensions());
    limiter.check(&format!("auth:{ip}"))?;
    Ok(next.run(req).await)
}

async fn user_rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    Extension(auth): Extension<AuthContext>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    limiter.check(&format!("user:{}:{}", auth.tenant_id, auth.user_id))?;
    Ok(next.run(req).await)
}

async fn admin_middleware(
    Extension(auth): Extension<AuthContext>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    // auth_middleware already reloaded authoritative `role` and `tenant_id` from SQLite.
    if !auth.role.eq_ignore_ascii_case("admin") {
        return Err(AppError::Unauthorized);
    }
    Ok(next.run(req).await)
}

async fn auth_middleware(
    State(state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let mut auth_ctx = domain::verify_jwt_claims(token)?;

    let user = crate::data::Repository::find_user_by_id(&state.pool, &auth_ctx.user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;

    // Authoritative DB check ensures revoked/demoted accounts or stale claims cannot escalate.
    auth_ctx.role = user.role;
    auth_ctx.tenant_id = user.tenant_id;

    req.extensions_mut().insert(auth_ctx);
    Ok(next.run(req).await)
}

// ── Auth endpoints ─────────────────────────────────────────────────────────

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<axum::response::Response, AppError> {
    if req.username.len() < 3 || req.username.len() > 50 {
        return Err(AppError::Validation(
            "Username must be 3-50 characters".into(),
        ));
    }
    if !req
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Username must contain only alphanumeric characters, underscores, or hyphens".into(),
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation(
            "Password must be at least 8 characters".into(),
        ));
    }
    if !req.email.contains('@') || !req.email.contains('.') {
        return Err(AppError::Validation("Invalid email format".into()));
    }

    let user = domain::register_user(&state.pool, req).await?;
    Ok((StatusCode::CREATED, Json(user)).into_response())
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<axum::response::Response, AppError> {
    let resp = domain::login_user(&state.pool, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

// ── Health check ───────────────────────────────────────────────────────────

async fn health_check() -> Result<axum::response::Response, AppError> {
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy",
            "engine_version": "2.0.0",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
        .into_response())
}

// ── Compression endpoint ───────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct CompressRequest {
    pub raw_text: String,
    pub task: Option<String>,
    pub use_case: Option<String>,
    pub mode: Option<String>,
    pub max_tokens: Option<usize>,
    pub verbose: Option<bool>,
    pub rag_enabled: Option<bool>,
    pub rag_document_ids: Option<Vec<String>>,
    pub rag_top_k: Option<usize>,
    pub npae_ambiguity_threshold: Option<f32>,
    pub npae_max_questions: Option<u8>,
    pub npae_confidence_threshold: Option<f32>,
    #[serde(default)]
    pub npae_render_style: Option<crate::npae::schema::types::RenderStyle>,
}

async fn demo_compress(
    State(state): State<AppState>,
    Json(mut req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    const MAX_DEMO_BYTES: usize = 8 * 1024;
    const MAX_DEMO_WORDS: usize = 500;

    if req.raw_text.trim().is_empty() {
        return Err(AppError::EmptyInput);
    }
    if req.raw_text.len() > MAX_DEMO_BYTES
        || req.raw_text.split_whitespace().count() > MAX_DEMO_WORDS
    {
        return Err(AppError::InputTooLarge);
    }

    // Never allow unauthenticated demo requests to query server-side RAG.
    req.rag_enabled = Some(false);
    req.rag_document_ids = None;

    let is_aggressive = req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"));
    let _permit = if is_aggressive {
        state
            .aggressive_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::TooManyRequests(
                    "Deep Blueprint capacity reached; please retry shortly".into(),
                )
            })?
    } else {
        state
            .compress_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::TooManyRequests(
                    "Compression capacity reached; please retry shortly".into(),
                )
            })?
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        domain::compress_demo(&state, req),
    )
    .await
    .map_err(|_| AppError::Timeout("Demo compression request timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn compress(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.raw_text)?;

    let is_aggressive = req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"));
    let _permit = if is_aggressive {
        state
            .aggressive_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::TooManyRequests(
                    "Deep Blueprint capacity reached; please retry shortly".into(),
                )
            })?
    } else {
        state
            .compress_sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                AppError::TooManyRequests(
                    "Compression capacity reached; please retry shortly".into(),
                )
            })?
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        domain::compress_new_scoped(&state, &auth.user_id, &auth.tenant_id, req),
    )
    .await
    .map_err(|_| AppError::Timeout("Compression request timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn decompress(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let record =
        domain::get_history_record_scoped(&state.pool, &auth.user_id, &auth.tenant_id, &id).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "original_prompt": record.original_prompt })),
    )
        .into_response())
}

#[derive(serde::Deserialize)]
struct PaginationParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records =
        domain::list_history_scoped(&state.pool, &auth.user_id, &auth.tenant_id, limit, offset)
            .await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

async fn feedback(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<crate::models::feedback_signal::ExplicitFeedback>,
) -> Result<axum::response::Response, AppError> {
    domain::record_explicit_feedback_scoped(&state, &auth.user_id, &auth.tenant_id, req).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn dev_config(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let config = domain::get_dev_config(&state.pool).await?;
    Ok((StatusCode::OK, Json(config)).into_response())
}

async fn stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::get_stats_scoped(&state.pool, &auth.user_id, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

// ── Admin endpoints ────────────────────────────────────────────────────────

async fn admin_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::admin_get_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

async fn admin_users(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let users = domain::admin_list_users(&state.pool, &auth.tenant_id, limit, offset).await?;
    Ok((StatusCode::OK, Json(users)).into_response())
}

#[derive(serde::Deserialize)]
struct AdminUpdateUserRequest {
    business_type: String,
}

async fn admin_update_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_update_user(&state.pool, &auth.tenant_id, &id, &req.business_type).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_delete_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_delete_user(&state.pool, &auth.tenant_id, &id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records = domain::admin_list_history(&state.pool, &auth.tenant_id, limit, offset).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

async fn admin_get_system_performance(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_system_performance(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_learning_engine_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_learning_engine_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

#[derive(serde::Deserialize)]
struct CompressionAnalyticsParams {
    time_window: Option<String>,
}

async fn admin_get_compression_analytics(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<CompressionAnalyticsParams>,
) -> Result<axum::response::Response, AppError> {
    let stats =
        domain::admin_get_compression_analytics(&state.pool, &auth.tenant_id, params.time_window)
            .await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_user_engagement_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_user_engagement_stats(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_feedback_analysis(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_feedback_analysis(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_error_metrics(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_error_metrics(&state.pool, &auth.tenant_id).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

// ── NPAE Handlers ─────────────────────────────────────────────────────────

async fn npae_compress(
    State(state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.prompt)?;
    let _permit = state
        .compress_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("Compression capacity reached; please retry shortly".into())
        })?;
    let prompt_owned = req.prompt.clone();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            crate::npae::compression::pipeline::run_parallel_pipeline(&prompt_owned)
        }),
    )
    .await
    .map_err(|_| AppError::Timeout("NPAE compression timed out".into()))?
    .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
    .map_err(AppError::Engine)?;

    Ok((StatusCode::OK, Json(result)).into_response())
}

/// Executes Aggressive Mode compression and structuring via `POST /api/v1/aggressive`.
///
/// # Errors
/// Returns [`AppError`] on validation, concurrency limit, timeout, or engine error.
async fn npae_aggressive(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.prompt)?;

    let _permit = state
        .aggressive_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests(
                "Deep Blueprint capacity reached; please retry shortly".into(),
            )
        })?;

    let fut = async {
        let empty_cfg = crate::npae::schema::types::NpaeConfig::default();
        let cfg = req.config.as_ref().unwrap_or(&empty_cfg);

        let mut enrichment = crate::types::EnrichmentContext::default();
        if req.rag_enabled.unwrap_or(false) {
            let history_id = uuid::Uuid::new_v4().to_string();
            let query = crate::rag::types::RetrievalQuery {
                query_text: req.prompt.clone(),
                user_id: auth.user_id.clone(),
                document_ids: req.rag_document_ids.clone(),
                top_k: req.rag_top_k.unwrap_or(5).clamp(1, 20),
                min_similarity: 0.45,
            };
            let retriever =
                crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
            if let Ok(retrieved_chunks) = retriever
                .retrieve_scoped(&auth.tenant_id, &query, Some(&history_id))
                .await
            {
                if !retrieved_chunks.is_empty() {
                    let chunks = retrieved_chunks
                        .into_iter()
                        .map(|res| crate::types::RagChunk {
                            domain_tag: res
                                .document_domain
                                .unwrap_or_else(|| "general".to_string()),
                            content: res.chunk.content,
                            metadata: Some(crate::types::ChunkMetadata {
                                page_number: res
                                    .chunk
                                    .metadata
                                    .as_ref()
                                    .and_then(|m| m.page_number)
                                    .map(|p| p as u32),
                                source_file: res.document_filename,
                            }),
                        })
                        .collect();
                    enrichment.rag_chunks = Some(chunks);
                }
            }
        }

        let prompt_for_repr = req.prompt.clone();
        let repr = tokio::task::spawn_blocking(move || {
            crate::npae::compression::pipeline::run_parallel_pipeline(&prompt_for_repr)
        })
        .await
        .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
        .map_err(AppError::Engine)?;

        let structurer = crate::npae::aggressive::structurer::HttpStructurer {
            route: "/api/v1/aggressive".to_string(),
            remote_addr: "authenticated".to_string(),
            body: req.prompt.clone(),
        };

        let reconstructed = state.pipeline.reconstruct_input(&req.prompt);

        crate::npae::aggressive::engine::AggressiveEngine::run(
            &req.prompt,
            &repr,
            cfg,
            state.npae_config.clone(),
            state.ory_engine.clone(),
            &structurer,
            &reconstructed,
            Some(&enrichment),
        )
        .await
        .map_err(AppError::Engine)
    };

    let resp = tokio::time::timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS), fut)
        .await
        .map_err(|_| AppError::Timeout("Aggressive compression timed out".into()))??;

    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn npae_hallucination_check(
    State(_state): State<AppState>,
    Json(req): Json<HallucinationCheckRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.original_prompt)?;
    validate_prompt_bounds(&req.output)?;
    let default_cfg = crate::npae::schema::types::HallucinationGuardConfig {
        self_critique_enabled: true,
        confidence_threshold: 0.75,
        contradiction_check: true,
        claim_verification_rules: vec![],
        uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
    };
    let cfg = req.guard_config.as_ref().unwrap_or(&default_cfg);
    let report =
        crate::npae::hallucination::guard::run_tri_layer(&req.output, &req.original_prompt, cfg)
            .map_err(AppError::Engine)?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

/// Verifies a downstream LLM response against numbered `[F#]` grounding facts (`GAP-42`).
async fn npae_verify_grounding(
    State(state): State<AppState>,
    Json(req): Json<crate::npae::grounding::verifier::VerifyGroundingRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.llm_answer)?;
    let _permit = state
        .compress_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("Verification capacity reached; please retry shortly".into())
        })?;
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            crate::npae::grounding::verifier::verify_grounding(&req)
        }),
    )
    .await
    .map_err(|_| AppError::Timeout("Grounding verification timed out".into()))?
    .map_err(|e| AppError::Internal(format!("Worker join error: {e}")))?
    .map_err(|e| AppError::Validation(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

async fn npae_schema() -> Result<axum::response::Response, AppError> {
    let schema_json = serde_json::json!({
        "version": "2.0.0",
        "schema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "NpaeGroundedResponse",
            "type": "object",
            "additionalProperties": false,
            "required": [
                "schema_version",
                "status",
                "items",
                "clarification_needed"
            ],
            "properties": {
                "schema_version": { "type": "string", "const": "2.0.0" },
                "status": {
                    "type": "string",
                    "enum": ["GROUNDED", "PARTIAL", "ABSTAIN", "CLARIFICATION_REQUIRED"]
                },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["requested_item", "status", "answer", "cited_facts"],
                        "properties": {
                            "requested_item": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["FOUND", "PARTIAL_MATCH", "NOT_FOUND"]
                            },
                            "answer": { "type": "string" },
                            "cited_facts": {
                                "type": "array",
                                "items": { "type": "string", "pattern": "^F[0-9]+$" }
                            }
                        }
                    }
                },
                "clarification_needed": { "type": "boolean" },
                "clarifying_questions": { "type": "array", "items": { "type": "string" } },
                "unverified_claims": { "type": "array", "items": { "type": "string" } }
            }
        }
    });
    Ok((StatusCode::OK, Json(schema_json)).into_response())
}

async fn npae_health() -> Result<axum::response::Response, AppError> {
    let health_json = serde_json::json!({
        "status": "ok",
        "version": "npae-1.0.0",
        "uptime_ms": 0,
    });
    Ok((StatusCode::OK, Json(health_json)).into_response())
}

// ── RAG Handlers ─────────────────────────────────────────────────────────────

async fn read_bounded_pdf_upload(
    multipart: &mut axum::extract::Multipart,
) -> Result<(String, String, Vec<u8>), AppError> {
    let mut raw_filename = String::new();
    let mut content_type: Option<String> = None;
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut found_file = false;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?
    {
        if field.name() == Some("file") {
            found_file = true;
            raw_filename = field.file_name().unwrap_or("document.pdf").to_string();
            content_type = field.content_type().map(str::to_string);

            // Stream chunks with incremental cap so oversized payloads abort before exhausting RAM.
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::Validation(e.to_string()))?
            {
                if file_bytes.len().saturating_add(chunk.len()) > MAX_PDF_UPLOAD_BYTES {
                    return Err(AppError::Validation(format!(
                        "File exceeds maximum allowed size of {} bytes",
                        MAX_PDF_UPLOAD_BYTES
                    )));
                }
                file_bytes.extend_from_slice(&chunk);
            }
            break;
        }
    }

    if !found_file || file_bytes.is_empty() {
        return Err(AppError::Validation(
            "No file uploaded or file is empty".into(),
        ));
    }

    let sanitized_filename = crate::rag::ingest::validate_pdf_upload(
        content_type.as_deref(),
        &raw_filename,
        &file_bytes,
        MAX_PDF_UPLOAD_BYTES,
    )
    .map_err(AppError::Validation)?;

    Ok((sanitized_filename, raw_filename, file_bytes))
}

async fn rag_health(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let db_healthy = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": if db_healthy { "healthy" } else { "unhealthy" },
            "subsystem": "rag",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
        .into_response())
}

async fn rag_upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Response, AppError> {
    let permit = state
        .pdf_ingest_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("PDF ingestion queue is busy; please retry shortly".into())
        })?;

    let (sanitized_filename, _, file_bytes) = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        read_bounded_pdf_upload(&mut multipart),
    )
    .await
    .map_err(|_| AppError::Timeout("Upload read timed out".into()))??;

    let ingester = crate::rag::ingest::DocumentIngester::with_config(
        state.pool.clone(),
        state.npae_config.clone(),
    );
    let config = crate::rag::types::ChunkingConfig::default();

    let user_id_clone = auth.user_id.clone();
    let tenant_id_clone = auth.tenant_id.clone();
    let filename_clone = sanitized_filename.clone();

    tokio::spawn(async move {
        let _permit = permit;
        let ingest_fut = ingester.ingest_pdf_scoped(
            &tenant_id_clone,
            &user_id_clone,
            &filename_clone,
            &file_bytes,
            &config,
        );
        match tokio::time::timeout(std::time::Duration::from_secs(65), ingest_fut).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::error!(
                    tenant_id = %tenant_id_clone,
                    user_id = %user_id_clone,
                    error = %e,
                    "Background RAG PDF ingestion failed"
                );
            }
            Err(_) => {
                tracing::error!(
                    tenant_id = %tenant_id_clone,
                    user_id = %user_id_clone,
                    "Background RAG PDF ingestion timed out"
                );
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "processing",
            "filename": sanitized_filename,
            "message": "File upload accepted. Processing started in the background."
        })),
    )
        .into_response())
}

async fn rag_list_documents(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    let docs = state
        .rag_store
        .list_documents_scoped(&auth.tenant_id, &auth.user_id, limit, offset)
        .await
        .map_err(AppError::Database)?;

    Ok((StatusCode::OK, Json(docs)).into_response())
}

async fn rag_get_document(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let doc = state
        .rag_store
        .get_document_scoped(&auth.tenant_id, &auth.user_id, &id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

    let chunk_count = state
        .rag_store
        .get_chunk_count(&id)
        .await
        .map_err(AppError::Database)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "document": doc,
            "chunk_count": chunk_count,
        })),
    )
        .into_response())
}

async fn rag_delete_document(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let deleted = state
        .rag_store
        .delete_document_scoped(&auth.tenant_id, &auth.user_id, &id)
        .await
        .map_err(AppError::Database)?;

    if !deleted {
        return Err(AppError::NotFound(
            "Document not found or access denied".into(),
        ));
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": "Document and all related chunks deleted"
        })),
    )
        .into_response())
}

async fn rag_search(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<crate::rag::types::RagSearchQuery>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&params.query)?;

    let doc_ids: Option<Vec<String>> = params.document_ids.map(|s| {
        s.split(',')
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect()
    });

    let query = crate::rag::types::RetrievalQuery {
        query_text: params.query,
        user_id: auth.user_id.clone(),
        document_ids: doc_ids,
        top_k: params.top_k.unwrap_or(5).clamp(1, 20),
        min_similarity: 0.25,
    };

    let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        retriever.retrieve_scoped(&auth.tenant_id, &query, None),
    )
    .await
    .map_err(|_| AppError::Timeout("RAG search timed out".into()))?
    .map_err(AppError::Engine)?;

    Ok((StatusCode::OK, Json(results)).into_response())
}

// ── Admin RAG Handlers ─────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct AdminRagUploadParams {
    target_user_id: Option<String>,
}

async fn admin_rag_upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<AdminRagUploadParams>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Response, AppError> {
    let (owner_id, owner_tenant_id) = if let Some(target_id) = params.target_user_id {
        let user = crate::data::Repository::find_user_by_id(&state.pool, &target_id)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("Target user not found".into()))?;
        if user.tenant_id != auth.tenant_id {
            return Err(AppError::NotFound("Target user not found".into()));
        }
        (user.id, user.tenant_id)
    } else {
        (auth.user_id.clone(), auth.tenant_id.clone())
    };

    let permit = state
        .pdf_ingest_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::TooManyRequests("PDF ingestion queue is busy; please retry shortly".into())
        })?;

    let (sanitized_filename, _, file_bytes) = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        read_bounded_pdf_upload(&mut multipart),
    )
    .await
    .map_err(|_| AppError::Timeout("Upload read timed out".into()))??;

    let ingester = crate::rag::ingest::DocumentIngester::with_config(
        state.pool.clone(),
        state.npae_config.clone(),
    );
    let config = crate::rag::types::ChunkingConfig::default();

    let owner_id_clone = owner_id.clone();
    let owner_tenant_clone = owner_tenant_id.clone();
    let filename_clone = sanitized_filename.clone();
    let admin_id_clone = auth.user_id.clone();

    tokio::spawn(async move {
        let _permit = permit;
        let ingest_fut = ingester.ingest_pdf_scoped(
            &owner_tenant_clone,
            &owner_id_clone,
            &filename_clone,
            &file_bytes,
            &config,
        );
        match tokio::time::timeout(std::time::Duration::from_secs(65), ingest_fut).await {
            Ok(Ok(result)) => {
                tracing::info!(
                    admin_id = %admin_id_clone,
                    tenant_id = %owner_tenant_clone,
                    owner_id = %owner_id_clone,
                    document_id = %result.document_id,
                    chunks = result.chunks_created,
                    domain = ?result.detected_domain,
                    "Admin RAG upload complete"
                );
            }
            Ok(Err(e)) => {
                tracing::error!(
                    admin_id = %admin_id_clone,
                    tenant_id = %owner_tenant_clone,
                    owner_id = %owner_id_clone,
                    error = %e,
                    "Admin RAG upload failed"
                );
            }
            Err(_) => {
                tracing::error!(
                    admin_id = %admin_id_clone,
                    tenant_id = %owner_tenant_clone,
                    owner_id = %owner_id_clone,
                    "Admin RAG upload timed out"
                );
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "processing",
            "filename": sanitized_filename,
            "owner_user_id": owner_id,
            "uploaded_by": auth.user_id,
            "message": "PDF accepted. Text extraction and embedding running in the background."
        })),
    )
        .into_response())
}

async fn admin_rag_list_documents(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    let docs = state
        .rag_store
        .list_documents_by_tenant(&auth.tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)?;

    Ok((StatusCode::OK, Json(docs)).into_response())
}

async fn admin_rag_delete_document(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let deleted = state
        .rag_store
        .admin_delete_document_scoped(&auth.tenant_id, &id)
        .await
        .map_err(AppError::Database)?;

    if !deleted {
        return Err(AppError::NotFound("Document not found".into()));
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "deleted",
            "document_id": id
        })),
    )
        .into_response())
}

// ── Scenario endpoints ──────────────────────────────────────────────────────

async fn scenario_get_registry(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let domains = &state.scenario_router.domain_registry.domains;
    let styles = &state.scenario_router.style_registry.styles;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "domains": domains,
            "styles": styles,
        })),
    )
        .into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ScenarioAskRequest {
    pub app_mode: String,
    pub domain: Option<String>,
    pub style: Option<String>,
    pub geography: Option<String>,
    pub question: String,
    pub mode: Option<String>,
    #[serde(default)]
    pub data_source: Option<String>,
    #[serde(default)]
    pub local_chunks: Option<Vec<crate::rag::local::LocalRetrievedChunk>>,
}

async fn scenario_ask(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<ScenarioAskRequest>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_bounds(&req.question)?;

    let app_mode_lower = req.app_mode.trim().to_ascii_lowercase();
    if app_mode_lower != "scenario" && app_mode_lower != "regular" {
        return Err(AppError::Validation(format!(
            "Invalid app_mode '{}'; expected 'scenario' or 'regular'",
            req.app_mode
        )));
    }

    if let Some(ref m) = req.mode {
        let m_lower = m.trim().to_ascii_lowercase();
        const ALLOWED_MODES: &[&str] = &["gentle", "balanced", "aggressive", "concise", "detailed"];
        if !ALLOWED_MODES.contains(&m_lower.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid mode '{}'; expected one of: gentle, balanced, aggressive, concise, detailed",
                m
            )));
        }
    }

    if let Some(ref g) = req.geography {
        let g_trimmed = g.trim();
        if g_trimmed.is_empty()
            || g_trimmed.len() > 32
            || !g_trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(AppError::Validation(
                "Invalid geography identifier: must be non-empty, <= 32 chars, and contain only alphanumeric, '_', or '-'".into(),
            ));
        }
    }

    let source_mode = req
        .data_source
        .as_deref()
        .unwrap_or("cloud")
        .trim()
        .to_ascii_lowercase();

    let scenario_source = match source_mode.as_str() {
        "cloud" => {
            if req.local_chunks.as_ref().is_some_and(|c| !c.is_empty()) {
                return Err(AppError::Validation(
                    "local_chunks cannot be provided when data_source is 'cloud'".to_string(),
                ));
            }
            crate::scenario::ScenarioDataSource::Cloud
        }
        "own" => {
            let raw_chunks = req.local_chunks.unwrap_or_default();
            if raw_chunks.len() > MAX_LOCAL_CHUNKS {
                return Err(AppError::Validation(format!(
                    "local_chunks exceeds maximum of {MAX_LOCAL_CHUNKS} chunks"
                )));
            }
            let mut mapped = Vec::with_capacity(raw_chunks.len());
            for c in raw_chunks {
                if c.content.len() > MAX_LOCAL_CHUNK_BYTES {
                    return Err(AppError::Validation(format!(
                        "local_chunk content exceeds maximum of {MAX_LOCAL_CHUNK_BYTES} bytes"
                    )));
                }
                let trimmed = c.content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                mapped.push(crate::types::RagChunk {
                    domain_tag: "local_own_data".to_string(),
                    content: trimmed.to_string(),
                    metadata: Some(crate::types::ChunkMetadata {
                        page_number: Some(c.page_number),
                        source_file: c.source_file.trim().to_string(),
                    }),
                });
            }
            crate::scenario::ScenarioDataSource::Own {
                local_chunks: mapped,
            }
        }
        other => {
            return Err(AppError::Validation(format!(
                "invalid data_source '{other}'; expected 'cloud' or 'own'"
            )));
        }
    };

    let _permit = if req
        .mode
        .as_deref()
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("aggressive"))
    {
        Some(
            state
                .aggressive_sem
                .clone()
                .try_acquire_owned()
                .map_err(|_| {
                    AppError::TooManyRequests(
                        "Deep Blueprint capacity reached; please retry shortly".into(),
                    )
                })?,
        )
    } else {
        None
    };

    let user = crate::data::Repository::find_user_by_id(&state.pool, &auth.user_id)
        .await
        .map_err(AppError::Database)?;
    let business_type = user
        .map(|u| u.business_type)
        .unwrap_or_else(|| "general".to_string());

    let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
        state.scenario_router.route_and_execute_with_tenant(
            &auth.tenant_id,
            &req.app_mode,
            req.domain.as_deref(),
            req.style.as_deref(),
            req.geography.as_deref().map(str::trim),
            &req.question,
            &auth.user_id,
            &business_type,
            req.mode.as_deref(),
            &retriever,
            Some(&state.pipeline),
            scenario_source,
        ),
    )
    .await
    .map_err(|_| AppError::Timeout("Scenario query timed out".into()))?
    .map_err(|err_msg| {
        if err_msg.starts_with("Validation Error:") {
            AppError::Validation(err_msg)
        } else {
            AppError::Engine(err_msg)
        }
    })?;

    let history_id = uuid::Uuid::new_v4().to_string();
    let original_tokens = crate::utils::tokens::estimate_tokens(&req.question) as i64;
    let final_tokens = crate::utils::tokens::estimate_tokens(&resp.output_text) as i64;
    let saved_tokens = (original_tokens - final_tokens).max(0);
    let use_case = req
        .domain
        .as_deref()
        .unwrap_or_else(|| req.style.as_deref().unwrap_or("scenario"));
    let mode_str = req.mode.as_deref().unwrap_or(&req.app_mode);
    let warnings_json = serde_json::to_string(&resp.warnings).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, warnings)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
    )
    .bind(&history_id)
    .bind(&auth.user_id)
    .bind(&auth.tenant_id)
    .bind(&req.question)
    .bind(&resp.output_text)
    .bind(saved_tokens)
    .bind(original_tokens)
    .bind(final_tokens)
    .bind(use_case)
    .bind(mode_str)
    .bind("3.1.0")
    .bind(&warnings_json)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to record scenario interaction in token_history: {}", e);
        AppError::Database(e)
    })?;

    Ok((StatusCode::OK, Json(resp)).into_response())
}
