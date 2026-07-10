use bcrypt::{hash, verify, DEFAULT_COST};
use db::DbPool;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    data::Repository,
    engine::evaluation,
    errors::AppError,
    models::{
        feedback_signal::ExplicitFeedback,
        token_history::TokenHistory,
        user::{LoginRequest, LoginResponse, RegisterRequest, User},
    },
    session::SessionHistory,
    AppState,
};

const ENGINE_VERSION: &str = "2.0.0";

/// JWT secret loaded from environment variable at startup.
/// Panics if JWT_SECRET is not set — this is intentional to prevent
/// running with a forgeable default secret.
static JWT_SECRET: Lazy<String> = Lazy::new(|| {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("JWT_SECRET not set — using fallback for development only");
        "dev_fallback_secret_do_not_use_in_production".to_string()
    })
});

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

// ── Compression (7-stage pipeline) ─────────────────────────────────────────

/// Simple word-based token estimation (matches engine/pipeline.rs convention)
fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f64 * 1.3).ceil() as usize
}

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

    let history_id = Uuid::new_v4().to_string();

    let mut enriched_prompt = if !context_prefix.is_empty() {
        format!("Context: [{}]\n\n{}", context_prefix, req.raw_text)
    } else {
        req.raw_text.clone()
    };

    if req.rag_enabled.unwrap_or(false) {
        let query = crate::rag::types::RetrievalQuery {
            query_text: req.raw_text.clone(),
            user_id: user_id_str.to_string(),
            document_ids: req.rag_document_ids.clone(),
            top_k: req.rag_top_k.unwrap_or(5),
            min_similarity: 0.25,
        };
        let retriever = crate::rag::retriever::DocumentRetriever::new((*state.rag_store).clone());
        if let Ok(retrieved_chunks) = retriever.retrieve(&query, Some(&history_id)).await {
            if !retrieved_chunks.is_empty() {
                let mut rag_section = String::new();
                rag_section.push_str("\n--- Retrieved Knowledge ---\n");
                for res in retrieved_chunks {
                    let page_str = res.chunk.metadata.as_ref()
                        .and_then(|m| m.page_number)
                        .map(|p| format!(", Page {}", p))
                        .unwrap_or_default();
                    rag_section.push_str(&format!(
                        "[Source: {}{}]\n{}\n\n",
                        res.document_filename,
                        page_str,
                        res.chunk.content
                    ));
                }
                rag_section.push_str("--- End Retrieved Knowledge ---\n");
                
                if !context_prefix.is_empty() {
                    enriched_prompt = format!("Context: [{}]\n\n{}{}", context_prefix, rag_section, req.raw_text);
                } else {
                    enriched_prompt = format!("{}{}", rag_section, req.raw_text);
                }
            }
        }
    }

    // 3. Run the 7-stage pipeline (0A → 6B)
    //    SessionHistory is per-request for now (future: persist across user sessions)
    let mut session = SessionHistory::new(10);

    let orchestrator_response = state
        .pipeline
        .process(&enriched_prompt, &mut session, req.mode.as_deref())
        .await
        .map_err(|e: Box<dyn std::error::Error>| AppError::Engine(e.to_string()))?;

    let compression_response = match orchestrator_response {
        crate::types::OrchestratorResponse::Legacy(resp) => resp,
        crate::types::OrchestratorResponse::Aggressive(resp) => {
            // Log aggressive mode history so feedback loop can function (GAP-N02 fix)
            let mut verbose = crate::models::verbose::SqlxVerbose::default();
            
            // Map StructuredPromptResponse to something Repository can handle
            // Since StructuredPromptResponse is slightly different, we build a partial history entry
            let token_original = resp.token_original as usize;
            let token_final = resp.token_final as usize;
            let token_saved = resp.token_saved as usize;

            let start = std::time::Instant::now();
            let result = sqlx::query(
                "INSERT INTO token_history (id, user_id, original_prompt, optimized_prompt, tokens_saved, token_original, token_final, use_case, mode, engine_version, principle_logs, warnings)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
            )
            .bind(&history_id)
            .bind(user_id_str)
            .bind(&enriched_prompt)
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

            // In Aggressive mode, we return immediately.
            // In real app, we might want to log 'verbose' to a diagnostic sink here.
            tracing::debug!("Aggressive history logged: {}ms", duration_ms);

            let mut json_resp = serde_json::to_value(resp).unwrap();
            if let Some(obj) = json_resp.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(history_id));
            }
            
            // 8. Implicit Signal Detection (Aggressive Mode branch)
            let prev_history = Repository::list_history(&state.pool, user_id_str, 1, 1).await
                .ok()
                .and_then(|list| list.into_iter().next());

            let response_time_ms = if let Some(ref prev) = prev_history {
                chrono::Utc::now().signed_duration_since(prev.created_at).num_milliseconds().max(0) as u64
            } else {
                0
            };

            let ctx = crate::behavior::feedback_detector::DetectionContext {
                user_id: user_id_str.to_string(),
                history_id: history_id.clone(),
                prev_prompt: prev_history.as_ref().map(|h| h.original_prompt.clone()),
                curr_prompt: enriched_prompt.clone(),
                response_time_ms,
                engagement_ms: 0,
            };

            let signals = crate::behavior::feedback_detector::detect(&ctx);
            for sig in signals {
                let weight = sig.map_to_weight();
                if weight != 0.0 {
                    let target_prompt = if let Some(ref prev) = prev_history { &prev.original_prompt } else { &enriched_prompt };
                    let mut engine = state.engine.lock().await;
                    let _ = engine.apply_feedback(user_id, target_prompt, weight).await;
                    
                    // ALSO apply to schema priors (GAP-N03 fix)
                    state.pipeline.apply_feedback(target_prompt, weight);
                    
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
                    if let Some(ref prev) = prev_history { sig_model.history_id = prev.id.clone(); }
                    let _ = Repository::insert_feedback(&state.pool, &sig_model).await;
                }
            }
            
            return Ok(json_resp);
        }
    };

    // 4. Compute token counts
    let token_original = estimate_tokens(&enriched_prompt);
    let token_final = estimate_tokens(&compression_response.response);
    let token_saved = token_original.saturating_sub(token_final);

    // 5. Run evaluation metrics (lexical overlap, semantic similarity, fact recall)
    let eval = evaluation::evaluate(&enriched_prompt, &compression_response.response);

    // 6. Persist to DB
    let mut verbose = crate::models::verbose::SqlxVerbose::default();
    Repository::insert_history_from_compression(
        &state.pool,
        &history_id,
        user_id_str,
        &enriched_prompt,
        &compression_response,
        token_original,
        token_final,
        token_saved,
        ENGINE_VERSION,
        &mut verbose,
    )
    .await
    .map_err(AppError::Database)?;

    // 7. Build response — backward compatible fields + new pipeline fields
    let json_resp = serde_json::json!({
        // Backward-compatible fields (consumed by app.js)
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

        // New 7-stage pipeline fields
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

    // 8. Implicit Signal Detection (GAP-N03)
    // Runs after current history is persisted to compare with previous prompt
    let prev_history = Repository::list_history(&state.pool, user_id_str, 1, 1).await
        .ok()
        .and_then(|list| list.into_iter().next());

    let response_time_ms = if let Some(ref prev) = prev_history {
        chrono::Utc::now().signed_duration_since(prev.created_at).num_milliseconds().max(0) as u64
    } else {
        0
    };

    let ctx = crate::behavior::feedback_detector::DetectionContext {
        user_id: user_id_str.to_string(),
        history_id: history_id.clone(),
        prev_prompt: prev_history.as_ref().map(|h| h.original_prompt.clone()),
        curr_prompt: req.raw_text.clone(),
        response_time_ms,
        engagement_ms: 0, // Not available yet in this flow
    };

    let signals = crate::behavior::feedback_detector::detect(&ctx);
    for sig in signals {
        let weight = sig.map_to_weight();
        if weight != 0.0 {
            // Apply feedback to the PREVIOUS prompt if it's a repetition/fast-reprompt signal
            let target_prompt = if let Some(ref prev) = prev_history {
                &prev.original_prompt
            } else {
                &req.raw_text
            };

            let mut engine = state.engine.lock().await;
            let _ = engine.apply_feedback(user_id, target_prompt, weight).await;

            // ALSO apply to schema priors (GAP-N03 fix)
            state.pipeline.apply_feedback(target_prompt, weight);
            
            // Persist the detected implicit signal for transparency
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
            // If it's a signal about the previous prompt, link it to the previous history ID
            if let Some(ref prev) = prev_history {
                sig_model.history_id = prev.id.clone();
            }
            let _ = Repository::insert_feedback(&state.pool, &sig_model).await;
        }
    }

    Ok(json_resp)
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

pub async fn list_history(pool: &DbPool, user_id: &str, limit: i64, offset: i64) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_history(pool, user_id, limit, offset)
        .await
        .map_err(AppError::Database)
}

// ── Feedback ───────────────────────────────────────────────────────────────

pub async fn record_explicit_feedback(
    state: &AppState,
    user_id: &str,
    req: ExplicitFeedback,
) -> Result<(), AppError> {
    let pool = &state.pool;
    let feedback_val = if req.rating == "thumbs_up" { 1.0f32 } else { -1.0f32 };
    let u_id = Uuid::parse_str(user_id).map_err(|_| AppError::Unauthorized)?;

    // 1. Fetch history FIRST to verify it exists and belongs to this user (GAP-N03 fix)
    if req.history_id.starts_with("mock-") {
        tracing::info!("Received feedback for mock history ID: {}. Skipping database lookup.", req.history_id);
        return Ok(());
    }

    let history = Repository::get_history(pool, user_id, &req.history_id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound(format!("History record {} not found for user", req.history_id)))?;

    // 2. Persist the raw explicit feedback signal
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
    
    Repository::insert_feedback(pool, &sig)
        .await
        .map_err(AppError::Database)?;

    // 3. Trigger the learning loop
    {
        let mut engine = state.engine.lock().await;
        let _ = engine.apply_feedback(u_id, &history.original_prompt, feedback_val).await;
        
        // ALSO apply to schema priors (GAP-N03 fix)
        state.pipeline.apply_feedback(&history.original_prompt, feedback_val);

        // 4. Implicit Signal Detection at Feedback Time
        // Check for long engagement as an implicit positive signal
        let engagement_ms = chrono::Utc::now().signed_duration_since(history.created_at).num_milliseconds().max(0) as u64;
        
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
                    let _ = engine.apply_feedback(u_id, &history.original_prompt, weight).await;
                    
                    // ALSO apply to schema priors (GAP-N03 fix)
                    state.pipeline.apply_feedback(&history.original_prompt, weight);
                    
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
                    let _ = Repository::insert_feedback(pool, &sig_model).await;
                }
            }
        }
    }

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

pub async fn admin_list_users(pool: &DbPool, limit: i64, offset: i64) -> Result<Vec<User>, AppError> {
    Repository::list_all_users(pool, limit, offset)
        .await
        .map_err(AppError::Database)
}

pub async fn admin_delete_user(pool: &DbPool, id: &str) -> Result<(), AppError> {
    // Prevent deleting the last admin
    if let Ok(Some(user)) = Repository::find_user_by_id(pool, id).await {
        if user.business_type.to_lowercase() == "admin" {
            let count = Repository::count_admins(pool).await.unwrap_or(0);
            if count <= 1 {
                return Err(AppError::Validation("Cannot delete the last admin".into()));
            }
        }
    }

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

pub async fn admin_list_history(pool: &DbPool, limit: i64, offset: i64) -> Result<Vec<TokenHistory>, AppError> {
    Repository::list_all_history(pool, limit, offset)
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
