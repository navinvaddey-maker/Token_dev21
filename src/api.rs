use crate::{
    domain,
    errors::AppError,
    models::user::{LoginRequest, RegisterRequest},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use axum::error_handling::HandleErrorLayer;
use tower::ServiceBuilder;

pub fn router(state: AppState) -> Router {
    let auth = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        // Limit to 5 requests per second for auth routes to prevent brute force
        // HandleErrorLayer + Buffer are required to make RateLimit cloneable for Axum
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: tower::BoxError| async move {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        format!("Rate limit exceeded: {}", err),
                    )
                }))
                .buffer(100)
                .rate_limit(5, std::time::Duration::from_secs(1))
        );

    let public = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/rag/health", get(rag_health))
        .merge(auth);

    let protected = Router::new()
        .route("/api/compress", post(compress))
        .route("/api/tokens/:id/decompress", get(decompress))
        .route("/api/history", get(history))
        .route("/api/feedback", post(feedback))
        .route("/api/dev/config", get(dev_config))
        .route("/api/stats", get(stats))
        .route("/api/rag/upload", post(rag_upload))
        .route("/api/rag/documents", get(rag_list_documents))
        .route("/api/rag/documents/:id", get(rag_get_document).delete(rag_delete_document))
        .route("/api/rag/search", get(rag_search))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
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
        .route("/api/admin/rag/upload", post(admin_rag_upload))
        .route("/api/admin/rag/documents", get(admin_rag_list_documents))
        .route("/api/admin/rag/documents/:id", axum::routing::delete(admin_rag_delete_document))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let npae_routes = Router::new()
        .route("/api/v1/compress", post(npae_compress))
        .route("/api/v1/aggressive", post(npae_aggressive))
        .route("/api/v1/hallucination-check", post(npae_hallucination_check))
        .route("/api/v1/schema", get(npae_schema))
        .route("/api/v1/health", get(npae_health));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin)
        .merge(npae_routes)
        .with_state(state)
}

// ── Auth endpoints ─────────────────────────────────────────────────────────

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<axum::response::Response, AppError> {
    // Input validation (GAP-029)
    if req.username.len() < 3 || req.username.len() > 50 {
        return Err(AppError::Validation("Username must be 3-50 characters".into()));
    }
    if !req.username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::Validation("Username must contain only alphanumeric characters, underscores, or hyphens".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 characters".into()));
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
    Ok((StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "engine_version": "2.0.0",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))).into_response())
}

// ── Compression endpoint ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CompressRequest {
    pub raw_text: String,
    pub task: Option<String>,

    pub model: Option<String>,
    pub use_case: Option<String>,
    pub mode: Option<String>,
    pub max_tokens: Option<usize>,
    // When enabled, the response will include extra SQLx/engine diagnostics.
    pub verbose: Option<bool>,
    pub rag_enabled: Option<bool>,
    pub rag_document_ids: Option<Vec<String>>,
    pub rag_top_k: Option<usize>,
}

async fn compress(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<CompressRequest>,
) -> Result<axum::response::Response, AppError> {
    // Process universally through domain::compress_new, which uses PipelineOrchestrator
    // The mode differentiation is now strictly handled by PipelineOrchestrator::process
    let resp = domain::compress_new(&state, &user_id, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn decompress(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let record = domain::get_history_record(&state.pool, &user_id, &id).await?;
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
    axum::Extension(user_id): axum::Extension<String>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records = domain::list_history(&state.pool, &user_id, limit, offset).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

async fn feedback(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Json(req): Json<crate::models::feedback_signal::ExplicitFeedback>,
) -> Result<axum::response::Response, AppError> {
    domain::record_explicit_feedback(&state, &user_id, req).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn dev_config(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let config = domain::get_dev_config(&state.pool).await?;
    Ok((StatusCode::OK, Json(config)).into_response())
}

async fn stats(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
) -> Result<axum::response::Response, AppError> {
    let s = domain::get_stats(&state.pool, &user_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

// ── Admin endpoints ────────────────────────────────────────────────────────

async fn admin_stats(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let s = domain::admin_get_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

async fn admin_users(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let users = domain::admin_list_users(&state.pool, limit, offset).await?;
    Ok((StatusCode::OK, Json(users)).into_response())
}

#[derive(serde::Deserialize)]
struct AdminUpdateUserRequest {
    business_type: String,
}

async fn admin_update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_update_user(&state.pool, &id, &req.business_type).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_delete_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    domain::admin_delete_user(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn admin_history(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let records = domain::admin_list_history(&state.pool, limit, offset).await?;
    Ok((StatusCode::OK, Json(records)).into_response())
}

// ── Admin Monitoring Endpoints ────────────────────────────────────────────────

async fn admin_get_system_performance(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_system_performance(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_learning_engine_stats(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_learning_engine_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

#[derive(serde::Deserialize)]
struct CompressionAnalyticsParams {
    time_window: Option<String>,
}

async fn admin_get_compression_analytics(
    State(state): State<AppState>,
    Query(params): Query<CompressionAnalyticsParams>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_compression_analytics(&state.pool, params.time_window).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_user_engagement_stats(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_user_engagement_stats(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_feedback_analysis(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_feedback_analysis(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

async fn admin_get_error_metrics(
    State(state): State<AppState>,
) -> Result<axum::response::Response, AppError> {
    let stats = domain::admin_get_error_metrics(&state.pool).await?;
    Ok((StatusCode::OK, Json(stats)).into_response())
}

// ── JWT and Admin middleware ───────────────────────────────────────────────

async fn admin_middleware(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let is_admin = domain::is_admin(&state.pool, &user_id).await?;
    if !is_admin {
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

    let user_id = crate::domain::verify_jwt(token)?;
    
    let user_exists = crate::data::Repository::find_user_by_id(&state.pool, &user_id)
        .await
        .map_err(|_| AppError::Internal("Database error".into()))?
        .is_some();
        
    if !user_exists {
        return Err(AppError::Unauthorized);
    }

    req.extensions_mut().insert(user_id);
    Ok(next.run(req).await)
}

// ── NPAE Handlers ─────────────────────────────────────────────────────────
use crate::npae::schema::types::{AggressiveRequest, HallucinationCheckRequest};

async fn npae_compress(
    State(_state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    let result = crate::npae::compression::pipeline::run_parallel_pipeline(&req.prompt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(result)).into_response())
}

async fn npae_aggressive(
    State(state): State<AppState>,
    Json(req): Json<AggressiveRequest>,
) -> Result<axum::response::Response, AppError> {
    let empty_cfg = crate::npae::schema::types::NpaeConfig { ambiguity_threshold: None, max_questions: None, confidence_threshold: None, skip_stage: None };
    let cfg = req.config.as_ref().unwrap_or(&empty_cfg);
    let repr = crate::npae::compression::pipeline::run_parallel_pipeline(&req.prompt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
        
    let structurer = crate::npae::aggressive::structurer::HttpStructurer {
        route: "/api/v1/aggressive".to_string(),
        remote_addr: "127.0.0.1".to_string(),
        body: req.prompt.clone(),
    };
    
    let resp = crate::npae::aggressive::engine::AggressiveEngine::run(&req.prompt, &repr, cfg, state.npae_config.clone(), state.ory_engine.clone(), &structurer)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

async fn npae_hallucination_check(
    State(_state): State<AppState>,
    Json(req): Json<HallucinationCheckRequest>,
) -> Result<axum::response::Response, AppError> {
    let default_cfg = crate::npae::schema::types::HallucinationGuardConfig {
        self_critique_enabled: true,
        confidence_threshold: 0.75,
        contradiction_check: true,
        claim_verification_rules: vec![],
        uncertainty_markers: vec!["[UNCERTAIN]".into(), "[VERIFY]".into(), "[APPROX]".into()],
    };
    let cfg = req.guard_config.as_ref().unwrap_or(&default_cfg);
    let report = crate::npae::hallucination::guard::run_tri_layer(&req.output, &req.original_prompt, cfg)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)).into_response())
}

async fn npae_schema() -> Result<axum::response::Response, AppError> {
    // Phase 7.4
    let schema_json = serde_json::json!({ "version": "1.0", "message": "Schema endpoint stub" });
    Ok((StatusCode::OK, Json(schema_json)).into_response())
}

async fn npae_health() -> Result<axum::response::Response, AppError> {
    // Phase 7.5
    let health_json = serde_json::json!({
        "status": "ok",
        "version": "npae-1.0.0",
        "uptime_ms": 0,
    });
    Ok((StatusCode::OK, Json(health_json)).into_response())
}

// ── RAG Handlers ─────────────────────────────────────────────────────────────

async fn rag_health(State(state): State<AppState>) -> Result<axum::response::Response, AppError> {
    let db_healthy = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": if db_healthy { "healthy" } else { "unhealthy" },
            "subsystem": "rag",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    ).into_response())
}

async fn rag_upload(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Response, AppError> {
    let mut filename = "unknown.pdf".to_string();
    let mut file_bytes = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::Validation(e.to_string()))? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            filename = field.file_name().unwrap_or("unknown.pdf").to_string();
            file_bytes = field.bytes().await.map_err(|e| AppError::Validation(e.to_string()))?.to_vec();
            break;
        }
    }

    if file_bytes.is_empty() {
        return Err(AppError::Validation("No file uploaded or file is empty".into()));
    }

    let ingester = crate::rag::ingest::DocumentIngester::new(state.pool.clone());
    let config = crate::rag::types::ChunkingConfig::default();

    let user_id_clone = user_id.clone();
    let filename_clone = filename.clone();
    
    tokio::spawn(async move {
        if let Err(e) = ingester.ingest_pdf(&user_id_clone, &filename_clone, &file_bytes, &config).await {
            tracing::error!("Background RAG PDF ingestion failed for user {}: {}", user_id_clone, e);
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "processing",
            "filename": filename,
            "message": "File upload accepted. Processing started in the background."
        })),
    ).into_response())
}

async fn rag_list_documents(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    let docs = state.rag_store.list_documents(&user_id, limit, offset).await
        .map_err(AppError::Database)?;

    Ok((StatusCode::OK, Json(docs)).into_response())
}

async fn rag_get_document(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let doc = state.rag_store.get_document(&user_id, &id).await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

    let chunk_count = state.rag_store.get_chunk_count(&id).await
        .map_err(AppError::Database)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "document": doc,
            "chunk_count": chunk_count,
        })),
    ).into_response())
}

async fn rag_delete_document(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let deleted = state.rag_store.delete_document(&user_id, &id).await
        .map_err(AppError::Database)?;

    if !deleted {
        return Err(AppError::NotFound("Document not found or access denied".into()));
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": "Document and all related chunks deleted"
        })),
    ).into_response())
}

async fn rag_search(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<String>,
    Query(params): Query<crate::rag::types::RagSearchQuery>,
) -> Result<axum::response::Response, AppError> {
    let doc_ids: Option<Vec<String>> = params.document_ids.map(|s| {
        s.split(',').map(|id| id.trim().to_string()).filter(|id| !id.is_empty()).collect()
    });

    let query_vector = crate::rag::embeddings::EmbeddingEngine::new().embed(&params.query);
    
    let results = state.rag_store.search(
        &query_vector,
        &user_id,
        doc_ids.as_deref(),
        params.top_k.unwrap_or(5),
        0.25,
    ).await
    .map_err(AppError::Database)?;

    Ok((StatusCode::OK, Json(results)).into_response())
}

// ── Admin RAG Handlers ─────────────────────────────────────────────────────

/// Query params for admin RAG upload — allows targeting a specific user's doc store.
#[derive(serde::Deserialize)]
struct AdminRagUploadParams {
    /// Optional user_id to scope the document to. Defaults to the admin's own id.
    target_user_id: Option<String>,
}

/// POST /api/admin/rag/upload
///
/// Admin-only multipart upload. Ingests the PDF and associates it with
/// `target_user_id` (query param) or the admin's own id if omitted.
async fn admin_rag_upload(
    State(state): State<AppState>,
    axum::Extension(admin_id): axum::Extension<String>,
    Query(params): Query<AdminRagUploadParams>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Response, AppError> {
    // Determine which user the document belongs to
    let owner_id = params.target_user_id.unwrap_or_else(|| admin_id.clone());

    let mut filename = "unknown.pdf".to_string();
    let mut file_bytes: Vec<u8> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "file" {
            filename = field.file_name().unwrap_or("unknown.pdf").to_string();
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::Validation(e.to_string()))?
                .to_vec();
            break;
        }
    }

    if file_bytes.is_empty() {
        return Err(AppError::Validation("No file field found or file is empty".into()));
    }

    if !filename.to_lowercase().ends_with(".pdf") {
        return Err(AppError::Validation("Only PDF files are supported".into()));
    }

    let ingester = crate::rag::ingest::DocumentIngester::new(state.pool.clone());
    let config = crate::rag::types::ChunkingConfig::default();

    let owner_id_clone = owner_id.clone();
    let filename_clone = filename.clone();
    let admin_id_clone = admin_id.clone();

    // Run ingestion asynchronously — returns 202 immediately
    tokio::spawn(async move {
        match ingester
            .ingest_pdf(&owner_id_clone, &filename_clone, &file_bytes, &config)
            .await
        {
            Ok(result) => {
                tracing::info!(
                    admin_id = %admin_id_clone,
                    owner_id = %owner_id_clone,
                    document_id = %result.document_id,
                    chunks = result.chunks_created,
                    domain = ?result.detected_domain,
                    "Admin RAG upload complete"
                );
            }
            Err(e) => {
                tracing::error!(
                    admin_id = %admin_id_clone,
                    owner_id = %owner_id_clone,
                    error = %e,
                    "Admin RAG upload failed"
                );
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "processing",
            "filename": filename,
            "owner_user_id": owner_id,
            "uploaded_by": admin_id,
            "message": "PDF accepted. Text extraction and embedding running in the background."
        })),
    )
        .into_response())
}

/// GET /api/admin/rag/documents?limit=50&offset=0
///
/// List ALL documents across ALL users for admin oversight.
async fn admin_rag_list_documents(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<axum::response::Response, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    let docs = state
        .rag_store
        .list_all_documents(limit, offset)
        .await
        .map_err(AppError::Database)?;

    Ok((StatusCode::OK, Json(docs)).into_response())
}

/// DELETE /api/admin/rag/documents/:id
///
/// Force-delete any document (ignores owner check).
async fn admin_rag_delete_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let deleted = state
        .rag_store
        .admin_delete_document(&id)
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
