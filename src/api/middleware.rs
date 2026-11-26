//! HTTP security boundaries: rate limiting, JWT/DB authentication, RBAC, timeouts, and security headers.
//!
//! // ARCH: ARCHITECTURE.md §2 Full Component Map — API & Security Boundary.

use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, HeaderValue},
    middleware, Extension,
};
use dashmap::DashMap;

use crate::{
    data::Repository,
    domain::{self, AuthContext},
    errors::AppError,
    AppState,
};

/// Maximum accepted raw prompt size in bytes (64 KiB).
pub(crate) const MAX_PROMPT_BYTES: usize = 64 * 1024;
/// Maximum accepted raw prompt length in whitespace-delimited words.
pub(crate) const MAX_PROMPT_WORDS: usize = 8_000;
/// Maximum accepted PDF upload size in bytes (50 MiB).
pub(crate) const MAX_PDF_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
/// Maximum number of local chunks accepted by Own Data scenario queries.
pub(crate) const MAX_LOCAL_CHUNKS: usize = 10;
/// Maximum size in bytes of an individual local chunk.
pub(crate) const MAX_LOCAL_CHUNK_BYTES: usize = 8 * 1024;
/// Per-handler execution timeout in seconds.
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 15;
/// Maximum number of tracked rate-limiter buckets before idle pruning/eviction.
pub(crate) const MAX_RATE_BUCKETS: usize = 50_000;

/// Shared query pagination parameters across user and admin list endpoints.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct PaginationParams {
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

/// Token-bucket state for a single rate-limit key.
#[derive(Clone, Debug)]
pub(crate) struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

/// In-memory per-key token-bucket rate limiter (`DashMap`-backed, bounded capacity).
#[derive(Clone, Debug)]
pub(crate) struct RateLimiter {
    buckets: Arc<DashMap<String, TokenBucket>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    /// Creates a new bounded token-bucket [`RateLimiter`].
    pub(crate) fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            capacity,
            refill_per_sec,
        }
    }

    /// Consumes one token for `key`, pruning stale buckets if capacity is reached.
    ///
    /// # Errors
    /// Returns [`AppError::TooManyRequests`] if the bucket has fewer than `1.0` token.
    pub(crate) fn try_consume_rate_token(&self, key: &str) -> Result<(), AppError> {
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

/// Extracts the client IP address, honoring `X-Forwarded-For` / `X-Real-IP` only when
/// the direct peer IP is listed in `TRUSTED_PROXIES`.
pub(crate) fn extract_trusted_client_ip(
    headers: &HeaderMap,
    req_exts: &axum::http::Extensions,
) -> String {
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

/// Validates that `text` is non-empty and within [`MAX_PROMPT_BYTES`] and [`MAX_PROMPT_WORDS`].
///
/// # Errors
/// Returns [`AppError::EmptyInput`] if blank or [`AppError::InputTooLarge`] if bounds are exceeded.
pub(crate) fn validate_prompt_byte_and_word_bounds(text: &str) -> Result<(), AppError> {
    if text.trim().is_empty() {
        return Err(AppError::EmptyInput);
    }
    if text.len() > MAX_PROMPT_BYTES || text.split_whitespace().count() > MAX_PROMPT_WORDS {
        return Err(AppError::InputTooLarge);
    }
    Ok(())
}

/// Enforces the 60-second global request timeout across all routes.
pub(crate) async fn global_timeout_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    tokio::time::timeout(std::time::Duration::from_secs(60), next.run(req))
        .await
        .map_err(|_| AppError::Timeout("Request timed out".into()))
}

/// Injects standard defense-in-depth HTTP security headers into all responses.
///
/// # Panics
/// Never.
pub async fn security_headers_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
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
            "default-src 'self'; script-src 'self' https://cdn.jsdelivr.net; style-src 'self' https://fonts.googleapis.com 'unsafe-inline'; font-src 'self' https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self' http://127.0.0.1:49152; frame-ancestors 'none'; object-src 'none'",
        ),
    );
    resp
}

/// Enforces per-IP rate limiting on public auth and demo endpoints.
pub(crate) async fn auth_rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let ip = extract_trusted_client_ip(req.headers(), req.extensions());
    limiter.try_consume_rate_token(&format!("auth:{ip}"))?;
    Ok(next.run(req).await)
}

/// Enforces per-`(tenant_id, user_id)` rate limiting on authenticated routes.
pub(crate) async fn user_rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    Extension(auth): Extension<AuthContext>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    limiter.try_consume_rate_token(&format!("user:{}:{}", auth.tenant_id, auth.user_id))?;
    Ok(next.run(req).await)
}

/// Restricts administrative routes to users whose authoritative DB role is `"admin"`.
pub(crate) async fn admin_middleware(
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

/// Authenticates `Bearer` or `tce_session` cookie JWTs and reloads authoritative DB user claims.
pub(crate) async fn auth_middleware(
    State(state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let bearer_token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);

    let cookie_token = if bearer_token.is_none() {
        req.headers()
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|part| {
                    let (k, v) = part.trim().split_once('=')?;
                    (k.trim() == "tce_session" && !v.trim().is_empty())
                        .then(|| v.trim().to_string())
                })
            })
    } else {
        None
    };

    let token = bearer_token
        .or(cookie_token)
        .ok_or(AppError::Unauthorized)?;

    let mut auth_ctx = domain::verify_jwt_claims(&token)?;

    let user = Repository::find_user_by_id(&state.pool, &auth_ctx.user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;

    // Authoritative DB check ensures revoked/demoted accounts or stale claims cannot escalate.
    auth_ctx.role = user.role;
    auth_ctx.tenant_id = user.tenant_id;

    req.extensions_mut().insert(auth_ctx);
    Ok(next.run(req).await)
}
