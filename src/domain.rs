//! Domain business logic: authentication, compression execution, and history retrieval.
//!
//! ARCH: ARCHITECTURE.md §1 Reality Check, §2 Component Map, §7 sqlx Rules — Auth and multi-tenancy.

use bcrypt::{hash, verify, DEFAULT_COST};
use db::DbPool;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    classifier::ScenarioClassifier,
    data::Repository,
    engine::evaluation,
    errors::AppError,
    models::{
        feedback_signal::ExplicitFeedback,
        token_history::TokenHistory,
        user::{LoginRequest, LoginResponse, RegisterRequest, User},
    },
    session::SessionHistory,
    utils::tokens::estimate_tokens,
    AppState,
};

const ENGINE_VERSION: &str = "2.0.0";

/// Allowed non-privileged business types for public user registration.
const ALLOWED_BUSINESS_TYPES: &[&str] = &[
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
pub fn validate_security_env() -> Result<(), String> {
    match std::env::var("JWT_SECRET") {
        Ok(secret) if secret.len() >= 32 => Ok(()),
        Ok(_) => Err("JWT_SECRET environment variable must be at least 32 bytes".to_string()),
        Err(_) => Err("JWT_SECRET environment variable is not set".to_string()),
    }
}

#[cfg(not(test))]
static JWT_CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Reads and validates the `JWT_SECRET` from environment variables without hardcoded fallbacks.
/// Caches the secret in non-test production builds for performance.
///
/// # Errors
/// Returns `AppError::Internal` if missing or fewer than 32 bytes.
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

// ── JWT & AuthContext ──────────────────────────────────────────────────────

/// Context extracted from verified JWT authentication tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub user_id: String,
    pub tenant_id: String,
    pub role: String,
}

/// Claims carried inside issued JWT tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub tenant_id: String,
    pub role: String,
    pub exp: usize,
    pub iss: String,
    pub aud: String,
}

/// Issues a JWT token for a specific user ID, tenant ID, and role with 1-hour expiration.
///
/// # Errors
/// Returns `AppError::Internal` if signing fails or secret is misconfigured.
pub fn create_jwt_for_user(user_id: &str, tenant_id: &str, role: &str) -> Result<String, AppError> {
    let secret = jwt_secret()?;
    let claims = Claims {
        sub: user_id.to_string(),
        tenant_id: tenant_id.to_string(),
        role: role.to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
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
/// Returns `AppError::Internal` if signing fails.
pub fn create_jwt(user_id: &str) -> Result<String, AppError> {
    create_jwt_for_user(user_id, "default", "user")
}

/// Verifies and decodes a JWT token into its typed `AuthContext`.
///
/// # Errors
/// Returns `AppError::Unauthorized` on invalid, expired, or non-matching tokens.
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
/// Returns `AppError::Unauthorized` on invalid token.
pub fn verify_jwt(token: &str) -> Result<String, AppError> {
    verify_jwt_claims(token).map(|ctx| ctx.user_id)
}

// ── User auth ──────────────────────────────────────────────────────────────

/// Registers a new user account with strict business_type validation and default role assignment.
///
/// # Errors
/// Returns `AppError::Validation` if input is invalid or business_type is unapproved/admin.
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

/// Authenticates user credentials and issues a JWT token populated with user role and tenant.
/// Executes bcrypt verification in `spawn_blocking` and enforces constant-time checks
/// against a dummy bcrypt hash on non-existent users to eliminate timing attacks.
///
/// # Errors
/// Returns `AppError::InvalidCredentials` on bad username or password.
pub async fn login_user(pool: &DbPool, req: LoginRequest) -> Result<LoginResponse, AppError> {
    let maybe_user = Repository::find_user_by_username(pool, &req.username)
        .await
        .map_err(AppError::Database)?;

    let password = req.password;
    const DUMMY_HASH: &str = "$2b$12$CwTycUXWue0Thq9StjUM0uJ8hT8mXKfR3c8sM8r7u3T4gJ2wN5aOa";

    let (user_opt, hash_to_verify) = match maybe_user {
        Some(u) => {
            let h = u.password_hash.clone();
            (Some(u), h)
        }
        None => (None, DUMMY_HASH.to_string()),
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

// ── Compression (7-stage pipeline) ─────────────────────────────────────────

/// Executes prompt compression defaulting to 'default' tenant.
///
/// # Errors
/// Returns `AppError` on execution or persistence failure.
pub async fn compress_new(
    state: &AppState,
    user_id_str: &str,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    compress_new_scoped(state, user_id_str, "default", req).await
}

/// Executes prompt compression with explicit tenant scoping.
///
/// # Errors
/// Returns `AppError` on execution or persistence failure.
pub async fn compress_new_scoped(
    state: &AppState,
    user_id_str: &str,
    tenant_id: &str,
    req: crate::api::CompressRequest,
) -> Result<serde_json::Value, AppError> {
    let user_id =
        Uuid::parse_str(user_id_str).map_err(|_| AppError::Internal("Invalid user_id".into()))?;

    // 1. Process via isolated per-(tenant_id, user_id) Learning Engine
    let engine_result = state
        .learning_pool
        .process(tenant_id, user_id, &req.raw_text)
        .await
        .map_err(AppError::Database)?;

    if engine_result.blocked {
        return Err(AppError::Engine("Prompt blocked by constraints".into()));
    }

    // 2. Prepare enrichment context (Cluster Vocab + RAG)
    let cluster_vocab = engine_result
        .cluster_vocab
        .iter()
        .map(|(token, _): &(String, f32)| token.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let history_id = Uuid::new_v4().to_string();

    let mut enrichment = crate::types::EnrichmentContext {
        cluster_vocab: if !cluster_vocab.is_empty() {
            Some(cluster_vocab)
        } else {
            None
        },
        rag_chunks: None,
        ..Default::default()
    };

    if req.rag_enabled.unwrap_or(false) {
        let query = crate::rag::types::RetrievalQuery {
            query_text: req.raw_text.clone(),
            user_id: user_id_str.to_string(),
            document_ids: req.rag_document_ids.clone(),
            top_k: req.rag_top_k.unwrap_or(5).clamp(1, 20),
            min_similarity: 0.25,
        };
        let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
        if let Ok(retrieved_chunks) = retriever
            .retrieve_scoped(tenant_id, &query, Some(&history_id))
            .await
        {
            if !retrieved_chunks.is_empty() {
                let chunks = retrieved_chunks
                    .into_iter()
                    .map(|res| crate::types::RagChunk {
                        domain_tag: res.document_domain.unwrap_or_else(|| "general".to_string()),
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

    // 2. Scenario Classification
    let scenario = ScenarioClassifier::new(&state.pool, &state.embedding_engine)
        .classify_query_into_scenario(&req.raw_text)
        .await;

    tracing::debug!(
        "ScenarioClassifier result: {:?}",
        scenario
            .as_ref()
            .map(|s| (&s.domain, &s.intent_class, s.confidence))
    );

    // 3. Run the 7-stage pipeline (0A → 6B) with tenant-scoped, bounded session cache
    let session_key = format!("{tenant_id}:{user_id_str}");
    if !state.sessions.contains_key(&session_key) && state.sessions.len() >= 10_000 {
        if let Some(evict_key) = state.sessions.iter().next().map(|e| e.key().clone()) {
            state.sessions.remove(&evict_key);
        }
    }
    let mut session = state
        .sessions
        .entry(session_key.clone())
        .or_insert_with(|| SessionHistory::new(10))
        .clone();

    let npae_cfg = crate::npae::schema::types::NpaeConfig {
        ambiguity_threshold: req.npae_ambiguity_threshold,
        max_questions: req.npae_max_questions,
        confidence_threshold: req.npae_confidence_threshold,
        skip_stage: None,
        render_style: req.npae_render_style,
        target_model_family: None,
    };

    let orchestrator_response = state
        .pipeline
        .process(
            &req.raw_text,
            &mut session,
            scenario,
            req.mode.as_deref(),
            Some(&npae_cfg),
            Some(enrichment),
        )
        .await
        .map_err(|e: Box<dyn std::error::Error>| AppError::Engine(e.to_string()))?;

    state.sessions.insert(session_key, session);

    let compression_response = match orchestrator_response {
        crate::types::OrchestratorResponse::Legacy(resp) => resp,
        crate::types::OrchestratorResponse::Aggressive(resp) => {
            let mut verbose = crate::models::verbose::SqlxVerbose::default();

            let token_original = resp.token_original as usize;
            let token_final = resp.token_final as usize;
            let token_saved = resp.token_saved as usize;

            let start = std::time::Instant::now();
            let result = sqlx::query(
                "INSERT INTO token_history (id, user_id, tenant_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
            )
            .bind(&history_id)
            .bind(user_id_str)
            .bind(tenant_id)
            .bind(&req.raw_text)
            .bind(&resp.optimized_prompt)
            .bind(token_saved as i64)
            .bind(token_original as i64)
            .bind(token_final as i64)
            .bind("auto")
            .bind("aggressive")
            .bind(ENGINE_VERSION)
            .bind(serde_json::json!({ "mode": "aggressive", "request_id": resp.request_id }))
            .bind(serde_json::json!([]))
            .execute(&state.pool)
            .await
            .map_err(AppError::Database)?;

            let duration_ms = start.elapsed().as_millis() as u64;
            verbose.push(crate::models::verbose::SqlxEvent {
                operation: "insert_history_aggressive".into(),
                duration_ms,
                row_count: Some(result.rows_affected() as i64),
                sql: Some("token_history.insert_aggressive".into()),
            });

            tracing::debug!("Aggressive history logged: {}ms", duration_ms);

            let tes = resp.scoring_result.tes;
            let sfs = resp.scoring_result.sfs;
            let scs = resp.scoring_result.scs;
            let overall = (tes + sfs + scs) / 3.0;
            let mut json_resp = serde_json::to_value(&resp).map_err(|e| {
                AppError::Internal(format!("Failed to serialize aggressive response: {e}"))
            })?;
            if let Some(obj) = json_resp.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(history_id));
                obj.insert("mode".to_string(), serde_json::json!("aggressive"));
                obj.insert("tes_score".to_string(), serde_json::json!(tes));
                obj.insert(
                    "dual_score".to_string(),
                    serde_json::json!({
                        "tes": tes,
                        "sfs": sfs,
                        "scs": scs,
                        "overall": overall,
                    }),
                );
                obj.entry("warnings".to_string())
                    .or_insert_with(|| serde_json::json!([]));
            }

            // 8. Implicit Signal Detection (Aggressive Mode branch)
            let prev_history =
                Repository::list_history_scoped(&state.pool, user_id_str, tenant_id, 1, 1)
                    .await
                    .ok()
                    .and_then(|list| list.into_iter().next());

            let response_time_ms = if let Some(ref prev) = prev_history {
                chrono::Utc::now()
                    .signed_duration_since(prev.created_at)
                    .num_milliseconds()
                    .max(0) as u64
            } else {
                0
            };

            let ctx = crate::behavior::feedback_detector::DetectionContext {
                user_id: user_id_str.to_string(),
                history_id: history_id.clone(),
                prev_prompt: prev_history.as_ref().map(|h| h.original_prompt.clone()),
                curr_prompt: req.raw_text.clone(),
                response_time_ms,
                engagement_ms: 0,
            };

            let signals = crate::behavior::feedback_detector::detect(&ctx);
            for sig in signals {
                let weight = sig.map_to_weight();
                if weight != 0.0 {
                    let target_prompt = if let Some(ref prev) = prev_history {
                        &prev.original_prompt
                    } else {
                        &req.raw_text
                    };
                    let _ = state
                        .learning_pool
                        .apply_feedback(tenant_id, user_id, target_prompt, weight)
                        .await;

                    let mut sig_model = crate::models::feedback_signal::FeedbackSignal {
                        id: Uuid::new_v4().to_string(),
                        user_id: user_id_str.to_string(),
                        history_id: history_id.clone(),
                        signal_type: sig.signal_type,
                        signal_layer: sig.signal_layer,
                        value: sig.value,
                        meta: sig.meta,
                        detected_at: chrono::Utc::now(),
                    };
                    if let Some(ref prev) = prev_history {
                        sig_model.history_id = prev.id.clone();
                    }
                    let _ = Repository::insert_feedback_scoped(&state.pool, &sig_model, tenant_id)
                        .await;
                }
            }

            return Ok(json_resp);
        }
    };

    // 4. Compute token counts
    let token_original = estimate_tokens(&req.raw_text) as usize;
    let token_final = estimate_tokens(&compression_response.response) as usize;
    let token_saved = token_original.saturating_sub(token_final);

    // 5. Run evaluation metrics
    let eval = evaluation::evaluate(&req.raw_text, &compression_response.response);

    // 6. Persist to DB with tenant scoping
    let mut verbose = crate::models::verbose::SqlxVerbose::default();
    Repository::insert_history_from_compression_scoped(
        &state.pool,
        &history_id,
        user_id_str,
        tenant_id,
        &req.raw_text,
        &compression_response,
        token_original,
        token_final,
        token_saved,
        ENGINE_VERSION,
        &mut verbose,
    )
    .await
    .map_err(AppError::Database)?;

    // 7. Build response
    let json_resp = serde_json::json!({
        "id":               history_id,
        "optimized_prompt":  compression_response.response,
        "token_original":   token_original,
        "token_final":      token_final,
        "token_saved":      token_saved,
        "warnings":         compression_response.field_issues.iter()
                                .map(|i| format!("{}: {}", i.field_name, i.description))
                                .collect::<Vec<String>>(),
        "evaluation":       eval,
        "engine_version":   ENGINE_VERSION,
        "cluster_id":       engine_result.cluster_id,
        "is_new_domain":    engine_result.is_new_domain,

        "mode":             compression_response.mode,
        "error_score":      compression_response.error_score,
        "fidelity":         compression_response.fidelity,
        "dual_score":       {
            "tes": compression_response.scoring_result.tes,
            "sfs": compression_response.scoring_result.sfs,
            "scs": compression_response.scoring_result.scs,
            "overall": (compression_response.scoring_result.tes + compression_response.scoring_result.sfs + compression_response.scoring_result.scs) / 3.0
        },
        "topology":         format!("{:?}", compression_response.topology),
        "normalization":    compression_response.normalization,
        "field_issues":     compression_response.field_issues,
        "scope_injections": compression_response.scope_injections,
        "correction_cycle": compression_response.correction_cycle,
        "task":             compression_response.schema.task,
        "context":          compression_response.schema.context,
        "null_fields":      compression_response.null_fields,

        "wm_slots_used":    compression_response.wm_slots_used,
        "clusters":         compression_response.clusters,
        "delta_tokens":     compression_response.delta_tokens,
    });

    // 8. Implicit Signal Detection
    let prev_history = Repository::list_history_scoped(&state.pool, user_id_str, tenant_id, 1, 1)
        .await
        .ok()
        .and_then(|list| list.into_iter().next());

    let response_time_ms = if let Some(ref prev) = prev_history {
        chrono::Utc::now()
            .signed_duration_since(prev.created_at)
            .num_milliseconds()
            .max(0) as u64
    } else {
        0
    };

    let ctx = crate::behavior::feedback_detector::DetectionContext {
        user_id: user_id_str.to_string(),
        history_id: history_id.clone(),
        prev_prompt: prev_history.as_ref().map(|h| h.original_prompt.clone()),
        curr_prompt: req.raw_text.clone(),
        response_time_ms,
        engagement_ms: 0,
    };

    let signals = crate::behavior::feedback_detector::detect(&ctx);
    for sig in signals {
        let weight = sig.map_to_weight();
        if weight != 0.0 {
            let target_prompt = if let Some(ref prev) = prev_history {
                &prev.original_prompt
            } else {
                &req.raw_text
            };

            let _ = state
                .learning_pool
                .apply_feedback(tenant_id, user_id, target_prompt, weight)
                .await;

            let mut sig_model = crate::models::feedback_signal::FeedbackSignal {
                id: Uuid::new_v4().to_string(),
                user_id: user_id_str.to_string(),
                history_id: history_id.clone(),
                signal_type: sig.signal_type,
                signal_layer: sig.signal_layer,
                value: sig.value,
                meta: sig.meta,
                detected_at: chrono::Utc::now(),
            };
            if let Some(ref prev) = prev_history {
                sig_model.history_id = prev.id.clone();
            }
            let _ = Repository::insert_feedback_scoped(&state.pool, &sig_model, tenant_id).await;
        }
    }

    Ok(json_resp)
}

/// Fetches a specific history record defaulting to 'default' tenant.
///
/// # Errors
/// Returns `AppError::NotFound` or `AppError::Database`.
pub async fn get_history_record(
    pool: &DbPool,
    user_id: &str,
    id: &str,
) -> Result<TokenHistory, AppError> {
    get_history_record_scoped(pool, user_id, "default", id).await
}

/// Fetches a specific history record with explicit tenant scoping.
///
/// # Errors
/// Returns `AppError::NotFound` or `AppError::Database`.
pub async fn get_history_record_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
    id: &str,
) -> Result<TokenHistory, AppError> {
    Repository::get_history_scoped(pool, user_id, tenant_id, id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(id.into()))
}

/// Lists history records defaulting to 'default' tenant.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn list_history(
    pool: &DbPool,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    list_history_scoped(pool, user_id, "default", limit, offset).await
}

/// Lists history records with explicit tenant scoping.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn list_history_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history_scoped(pool, user_id, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

// ── Feedback ───────────────────────────────────────────────────────────────

/// Records explicit feedback defaulting to 'default' tenant.
///
/// # Errors
/// Returns `AppError` on database failure or unauthorized request.
pub async fn record_explicit_feedback(
    state: &AppState,
    user_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    record_explicit_feedback_scoped(state, user_id, "default", req).await
}

/// Records explicit feedback with explicit tenant scoping.
///
/// # Errors
/// Returns `AppError` on database failure or unauthorized request.
pub async fn record_explicit_feedback_scoped(
    state: &AppState,
    user_id: &str,
    tenant_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    let pool = &state.pool;
    let feedback_val = if req.rating == "thumbs_up" {
        1.0f32
    } else {
        -1.0f32
    };
    let u_id = Uuid::parse_str(user_id).map_err(|_| AppError::Unauthorized)?;

    if req.history_id.starts_with("mock-") {
        tracing::info!(
            "Received feedback for mock history ID: {}. Skipping database lookup.",
            req.history_id
        );
        return Ok(());
    }

    let history = Repository::get_history_scoped(pool, user_id, tenant_id, &req.history_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "History record {} not found for user",
                req.history_id
            ))
        })?;

    let sig_id = Uuid::new_v4().to_string();
    let sig = crate::models::feedback_signal::FeedbackSignal {
        id: sig_id,
        user_id: user_id.to_string(),
        history_id: req.history_id.clone(),
        signal_type: req.rating.clone(),
        signal_layer: 5,
        value: feedback_val as f64,
        meta: serde_json::json!({}),
        detected_at: chrono::Utc::now(),
    };

    Repository::insert_feedback_scoped(pool, &sig, tenant_id)
        .await
        .map_err(AppError::Database)?;

    {
        let _ = state
            .learning_pool
            .apply_feedback(tenant_id, u_id, &history.original_prompt, feedback_val)
            .await;

        let engagement_ms = chrono::Utc::now()
            .signed_duration_since(history.created_at)
            .num_milliseconds()
            .max(0) as u64;

        let ctx = crate::behavior::feedback_detector::DetectionContext {
            user_id: user_id.to_string(),
            history_id: req.history_id.clone(),
            prev_prompt: None,
            curr_prompt: history.original_prompt.clone(),
            response_time_ms: 0,
            engagement_ms,
        };

        let signals = crate::behavior::feedback_detector::detect(&ctx);
        for sig in signals {
            if sig.signal_type == "long_engagement" {
                let weight = sig.map_to_weight();
                if weight != 0.0 {
                    let _ = state
                        .learning_pool
                        .apply_feedback(tenant_id, u_id, &history.original_prompt, weight)
                        .await;

                    let sig_model = crate::models::feedback_signal::FeedbackSignal {
                        id: Uuid::new_v4().to_string(),
                        user_id: user_id.to_string(),
                        history_id: req.history_id.clone(),
                        signal_type: sig.signal_type,
                        signal_layer: sig.signal_layer,
                        value: sig.value,
                        meta: sig.meta,
                        detected_at: chrono::Utc::now(),
                    };
                    let _ = Repository::insert_feedback_scoped(pool, &sig_model, tenant_id).await;
                }
            }
        }
    }

    Ok(())
}

// ── Dev config ─────────────────────────────────────────────────────────────

/// Returns configuration for development environments.
///
/// # Errors
/// Returns `AppError::Database` on failure.
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

/// Returns compression statistics for a user defaulting to 'default' tenant.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn get_stats(pool: &DbPool, user_id: &str) -> Result<serde_json::Value, AppError> {
    get_stats_scoped(pool, user_id, "default").await
}

/// Returns compression statistics for a user scoped by tenant.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn get_stats_scoped(
    pool: &DbPool,
    user_id: &str,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    let stats = Repository::get_user_stats_scoped(pool, user_id, tenant_id)
        .await
        .map_err(AppError::Database)?;
    Ok(serde_json::json!(stats))
}

// ── Admin Domain ───────────────────────────────────────────────────────────

/// Checks whether the user has administrative privileges based on `user.role == "admin"`.
///
/// # Errors
/// Returns `AppError::Unauthorized` if user is not found or `AppError::Database` on query error.
pub async fn is_admin(pool: &DbPool, user_id: &str) -> Result<bool, AppError> {
    let user = Repository::find_user_by_id(pool, user_id)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;
    // ARCH: §0, §7 — Strict RBAC check based on `role`, not `business_type`
    Ok(user.role.eq_ignore_ascii_case("admin"))
}

/// Lists users scoped to a specific tenant for administrative review.
///
/// # Errors
/// Returns `AppError::Database` on query error.
pub async fn admin_list_users(
    pool: &DbPool,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<User>, AppError> {
    Repository::list_users_by_tenant(pool, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

/// Deletes a user account inside a tenant, preventing deletion of the last admin.
///
/// # Errors
/// Returns `AppError::NotFound` if user does not exist in tenant, `AppError::Validation` if attempting to delete the last admin, or `AppError::Database`.
pub async fn admin_delete_user(pool: &DbPool, tenant_id: &str, id: &str) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::Database)?;

    let target_user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, email, business_type, role, tenant_id, license, created_at, updated_at
         FROM users WHERE id = $1 AND tenant_id = $2"
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("User not found in tenant".into()))?;

    if target_user.role.eq_ignore_ascii_case("admin") {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE LOWER(role) = 'admin' AND tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::Database)?;

        if count.0 <= 1 {
            return Err(AppError::Validation("Cannot delete the last admin".into()));
        }
    }

    let rows = sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found in tenant".into()));
    }

    tx.commit().await.map_err(AppError::Database)?;
    Ok(())
}

/// Updates a user's business type scoped to a tenant, rejecting attempts to elevate to admin via `business_type`.
///
/// # Errors
/// Returns `AppError::Validation` if `business_type == "admin"` or invalid, `AppError::NotFound` if user not found, or `AppError::Database`.
pub async fn admin_update_user(
    pool: &DbPool,
    tenant_id: &str,
    id: &str,
    business_type: &str,
) -> Result<(), AppError> {
    let b_type_lower = business_type.trim().to_lowercase();
    if b_type_lower == "admin" || !ALLOWED_BUSINESS_TYPES.contains(&b_type_lower.as_str()) {
        return Err(AppError::Validation(
            "Cannot set business_type to admin or unapproved type. Roles must be managed explicitly."
                .into(),
        ));
    }
    let affected = Repository::update_user_business_type_scoped(pool, tenant_id, id, business_type)
        .await
        .map_err(AppError::Database)?;
    if affected == 0 {
        return Err(AppError::NotFound("User not found in tenant".into()));
    }
    Ok(())
}

/// Returns platform statistics scoped to a specific tenant.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn admin_get_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_tenant_stats(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

/// Lists compression history records scoped to a specific tenant.
///
/// # Errors
/// Returns `AppError::Database` on query failure.
pub async fn admin_list_history(
    pool: &DbPool,
    tenant_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history_by_tenant(pool, tenant_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

// ── Admin Monitoring ───────────────────────────────────────────────────────────

pub async fn admin_get_system_performance(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_system_performance_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_learning_engine_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_learning_engine_stats_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_compression_analytics(
    pool: &DbPool,
    tenant_id: &str,
    time_window: Option<String>,
) -> Result<serde_json::Value, AppError> {
    Repository::get_compression_analytics_scoped(pool, tenant_id, time_window)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_user_engagement_stats(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_user_engagement_stats_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_feedback_analysis(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_feedback_analysis_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_get_error_metrics(
    pool: &DbPool,
    tenant_id: &str,
) -> Result<serde_json::Value, AppError> {
    Repository::get_error_metrics_scoped(pool, tenant_id)
        .await
        .map_err(AppError::Database)
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_validate_security_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("JWT_SECRET");
        assert!(validate_security_env().is_err());

        std::env::set_var("JWT_SECRET", "short_secret");
        assert!(validate_security_env().is_err());

        std::env::set_var(
            "JWT_SECRET",
            "this_is_a_sufficiently_long_secret_key_32_bytes",
        );
        assert!(validate_security_env().is_ok());
    }

    #[test]
    fn test_jwt_claims_and_auth_context() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var(
            "JWT_SECRET",
            "this_is_a_sufficiently_long_secret_key_32_bytes",
        );

        let token =
            create_jwt_for_user("user_123", "tenant_xyz", "admin").expect("Token creation failed");

        let auth_ctx = verify_jwt_claims(&token).expect("Token verification failed");
        assert_eq!(auth_ctx.user_id, "user_123");
        assert_eq!(auth_ctx.tenant_id, "tenant_xyz");
        assert_eq!(auth_ctx.role, "admin");

        let simple_user_id = verify_jwt(&token).expect("Simple verify failed");
        assert_eq!(simple_user_id, "user_123");
    }

    #[tokio::test]
    async fn test_registration_business_type_allowlist() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        // Reject "admin" as business_type
        let bad_admin_req = RegisterRequest {
            username: "hacker".into(),
            password: "password123".into(),
            email: "hacker@example.com".into(),
            business_type: Some("admin".into()),
        };
        assert!(register_user(&pool, bad_admin_req).await.is_err());

        // Reject unknown business_type
        let bad_type_req = RegisterRequest {
            username: "hacker2".into(),
            password: "password123".into(),
            email: "hacker2@example.com".into(),
            business_type: Some("super_user".into()),
        };
        assert!(register_user(&pool, bad_type_req).await.is_err());

        // Accept valid business_type
        let valid_req = RegisterRequest {
            username: "developer1".into(),
            password: "password123".into(),
            email: "dev@example.com".into(),
            business_type: Some("developer".into()),
        };
        let user = register_user(&pool, valid_req)
            .await
            .expect("registration failed");
        assert_eq!(user.role, "user");
        assert_eq!(user.tenant_id, "default");
        assert_eq!(user.business_type, "developer");
    }
}
