//! HTTP API Router, Authentication/RBAC Middleware, Rate Limiting, and Validation Boundaries.
//!
//! // ARCH: ARCHITECTURE.md §2 Full Component Map (`User → API (Axum) → Auth+RateLimit → Domain`).

mod admin;
mod auth;
mod compress;
pub mod middleware;
mod npae;
mod rag;
mod scenario;

use axum::{
    extract::DefaultBodyLimit,
    middleware as axum_middleware,
    routing::{delete, get, post, put},
    Router,
};

pub use compress::CompressRequest;
pub use middleware::security_headers_middleware;
pub use scenario::ScenarioAskRequest;

use crate::AppState;
use middleware::{
    admin_middleware, auth_middleware, auth_rate_limit_middleware, global_timeout_middleware,
    user_rate_limit_middleware, RateLimiter, MAX_PDF_UPLOAD_BYTES,
};

/// Builds authenticated user routes with user rate-limiting and PDF upload body limit.
fn build_protected_routes(state: &AppState, user_limiter: RateLimiter) -> Router<AppState> {
    let rag_upload_route = Router::new()
        .route("/api/rag/upload", post(rag::rag_upload))
        .layer(DefaultBodyLimit::max(
            MAX_PDF_UPLOAD_BYTES + 2 * 1024 * 1024,
        ));

    Router::new()
        .route("/api/me", get(auth::me))
        .route("/api/compress", post(compress::compress))
        .route("/api/tokens/:id/decompress", get(compress::decompress))
        .route("/api/history", get(compress::history))
        .route("/api/feedback", post(compress::feedback))
        .route("/api/dev/config", get(compress::dev_config))
        .route("/api/stats", get(compress::stats))
        .route("/api/scenario/ask", post(scenario::scenario_ask))
        .route("/api/rag/documents", get(rag::rag_list_documents))
        .route(
            "/api/rag/documents/:id",
            get(rag::rag_get_document).delete(rag::rag_delete_document),
        )
        .route("/api/rag/search", get(rag::rag_search))
        .route("/api/v1/compress", post(npae::npae_compress))
        .route("/api/v1/aggressive", post(npae::npae_aggressive))
        .route("/api/npae/verify", post(npae::npae_verify_grounding))
        .route(
            "/api/v1/hallucination-check",
            post(npae::npae_hallucination_check),
        )
        .merge(rag_upload_route)
        .layer(axum_middleware::from_fn_with_state(
            user_limiter,
            user_rate_limit_middleware,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
}

/// Builds tenant-admin routes protected by `auth_middleware`, `user_rate_limit_middleware`, and `admin_middleware`.
fn build_admin_routes(state: &AppState, user_limiter: RateLimiter) -> Router<AppState> {
    let admin_rag_upload_route = Router::new()
        .route("/api/admin/rag/upload", post(rag::admin_rag_upload))
        .layer(DefaultBodyLimit::max(
            MAX_PDF_UPLOAD_BYTES + 2 * 1024 * 1024,
        ));

    Router::new()
        .route("/api/admin/stats", get(admin::admin_stats))
        .route("/api/admin/users", get(admin::admin_users))
        .route(
            "/api/admin/users/:id",
            put(admin::admin_update_user).delete(admin::admin_delete_user),
        )
        .route("/api/admin/history", get(admin::admin_history))
        .route(
            "/api/admin/system-performance",
            get(admin::admin_get_system_performance),
        )
        .route(
            "/api/admin/learning-engine-stats",
            get(admin::admin_get_learning_engine_stats),
        )
        .route(
            "/api/admin/compression-analytics",
            get(admin::admin_get_compression_analytics),
        )
        .route(
            "/api/admin/user-engagement",
            get(admin::admin_get_user_engagement_stats),
        )
        .route(
            "/api/admin/feedback-analysis",
            get(admin::admin_get_feedback_analysis),
        )
        .route(
            "/api/admin/error-metrics",
            get(admin::admin_get_error_metrics),
        )
        .route(
            "/api/admin/rag/documents",
            get(rag::admin_rag_list_documents),
        )
        .route(
            "/api/admin/rag/documents/:id",
            delete(rag::admin_rag_delete_document),
        )
        .merge(admin_rag_upload_route)
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            admin_middleware,
        ))
        .layer(axum_middleware::from_fn_with_state(
            user_limiter,
            user_rate_limit_middleware,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
}

/// Builds the Axum HTTP router with per-IP auth limits, per-tenant API limits, body caps, and RBAC.
///
/// # Panics
/// Never.
pub fn router(state: AppState) -> Router {
    let auth_limiter = RateLimiter::new(15.0, 0.5);
    let user_limiter = RateLimiter::new(30.0, 2.0);

    let auth_routes = Router::new()
        .route("/api/register", post(auth::register))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/demo/compress", post(compress::demo_compress))
        .layer(axum_middleware::from_fn_with_state(
            auth_limiter,
            auth_rate_limit_middleware,
        ));

    let public = Router::new()
        .route("/api/health", get(auth::health_check))
        .route("/api/rag/health", get(rag::rag_health))
        .route(
            "/api/scenario/registry",
            get(scenario::scenario_get_registry),
        )
        .route("/api/v1/schema", get(npae::npae_schema))
        .route("/api/v1/health", get(npae::npae_health))
        .merge(auth_routes);

    let protected = build_protected_routes(&state, user_limiter.clone());
    let admin_routes = build_admin_routes(&state, user_limiter);

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin_routes)
        .layer(axum_middleware::from_fn(global_timeout_middleware))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(state)
}
