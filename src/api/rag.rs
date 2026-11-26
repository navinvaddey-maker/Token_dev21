//! Cloud RAG document ingestion, search, deletion, and health HTTP handlers.
//!
//! // ARCH: ARCHITECTURE.md §2 Full Component Map, §9 RAG Rules — Tenant-isolated RAG API surface.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

use super::middleware::{
    validate_prompt_byte_and_word_bounds, PaginationParams, MAX_PDF_UPLOAD_BYTES,
    REQUEST_TIMEOUT_SECS,
};
use crate::{data::Repository, domain::AuthContext, errors::AppError, AppState};

/// Streams and validates a bounded `multipart/form-data` PDF upload up to [`MAX_PDF_UPLOAD_BYTES`].
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
                        "File exceeds maximum allowed size of {MAX_PDF_UPLOAD_BYTES} bytes"
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

/// Parameters for a background PDF ingestion task.
struct BackgroundPdfIngestJob {
    permit: tokio::sync::OwnedSemaphorePermit,
    tenant_id: String,
    owner_user_id: String,
    filename: String,
    file_bytes: Vec<u8>,
    admin_user_id: Option<String>,
}

/// Spawns bounded background PDF ingestion holding `job.permit` until completion or 65s timeout.
fn spawn_background_pdf_ingestion(state: &AppState, job: BackgroundPdfIngestJob) {
    let ingester = crate::rag::ingest::DocumentIngester::with_config(
        state.pool.clone(),
        state.npae_config.clone(),
    );
    let config = crate::rag::types::ChunkingConfig::default();

    tokio::spawn(async move {
        let BackgroundPdfIngestJob {
            permit: _permit,
            tenant_id,
            owner_user_id,
            filename,
            file_bytes,
            admin_user_id,
        } = job;
        let ingest_fut =
            ingester.ingest_pdf_scoped(&tenant_id, &owner_user_id, &filename, &file_bytes, &config);
        match tokio::time::timeout(std::time::Duration::from_secs(65), ingest_fut).await {
            Ok(Ok(result)) => {
                if let Some(ref admin_id) = admin_user_id {
                    tracing::info!(
                        admin_id = %admin_id,
                        tenant_id = %tenant_id,
                        owner_id = %owner_user_id,
                        document_id = %result.document_id,
                        chunks = result.chunks_created,
                        domain = ?result.detected_domain,
                        "Admin RAG upload complete"
                    );
                }
            }
            Ok(Err(e)) => {
                if let Some(ref admin_id) = admin_user_id {
                    tracing::error!(
                        admin_id = %admin_id,
                        tenant_id = %tenant_id,
                        owner_id = %owner_user_id,
                        error = %e,
                        "Admin RAG upload failed"
                    );
                } else {
                    tracing::error!(
                        tenant_id = %tenant_id,
                        user_id = %owner_user_id,
                        error = %e,
                        "Background RAG PDF ingestion failed"
                    );
                }
            }
            Err(_) => {
                if let Some(ref admin_id) = admin_user_id {
                    tracing::error!(
                        admin_id = %admin_id,
                        tenant_id = %tenant_id,
                        owner_id = %owner_user_id,
                        "Admin RAG upload timed out"
                    );
                } else {
                    tracing::error!(
                        tenant_id = %tenant_id,
                        user_id = %owner_user_id,
                        "Background RAG PDF ingestion timed out"
                    );
                }
            }
        }
    });
}

/// Checks backing database connectivity for the RAG subsystem via `GET /api/rag/health`.
pub(crate) async fn rag_health(State(state): State<AppState>) -> axum::response::Response {
    let db_healthy = Repository::verify_database_connectivity(&state.pool).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": if db_healthy { "healthy" } else { "unhealthy" },
            "subsystem": "rag",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
        .into_response()
}

/// Accepts a user PDF upload and dispatches background ingestion via `POST /api/rag/upload`.
pub(crate) async fn rag_upload(
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

    spawn_background_pdf_ingestion(
        &state,
        BackgroundPdfIngestJob {
            permit,
            tenant_id: auth.tenant_id,
            owner_user_id: auth.user_id,
            filename: sanitized_filename.clone(),
            file_bytes,
            admin_user_id: None,
        },
    );

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

/// Lists RAG documents owned by the authenticated user within their tenant via `GET /api/rag/documents`.
pub(crate) async fn rag_list_documents(
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

/// Fetches a single RAG document and its chunk count via `GET /api/rag/documents/:id`.
pub(crate) async fn rag_get_document(
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

/// Deletes a user's RAG document and its chunks via `DELETE /api/rag/documents/:id`.
pub(crate) async fn rag_delete_document(
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

/// Executes hybrid RAG search scoped to `(tenant_id, user_id)` via `GET /api/rag/search`.
pub(crate) async fn rag_search(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<crate::rag::types::RagSearchQuery>,
) -> Result<axum::response::Response, AppError> {
    validate_prompt_byte_and_word_bounds(&params.query)?;

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

/// Optional query parameters for `POST /api/admin/rag/upload`.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct AdminRagUploadParams {
    target_user_id: Option<String>,
}

/// Accepts a tenant-admin PDF upload via `POST /api/admin/rag/upload`.
pub(crate) async fn admin_rag_upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<AdminRagUploadParams>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Response, AppError> {
    let (owner_id, owner_tenant_id) = if let Some(target_id) = params.target_user_id {
        let user = Repository::find_user_by_id(&state.pool, &target_id)
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

    spawn_background_pdf_ingestion(
        &state,
        BackgroundPdfIngestJob {
            permit,
            tenant_id: owner_tenant_id,
            owner_user_id: owner_id.clone(),
            filename: sanitized_filename.clone(),
            file_bytes,
            admin_user_id: Some(auth.user_id.clone()),
        },
    );

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

/// Lists all RAG documents in the admin's tenant via `GET /api/admin/rag/documents`.
pub(crate) async fn admin_rag_list_documents(
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

/// Deletes any RAG document inside the admin's tenant via `DELETE /api/admin/rag/documents/:id`.
pub(crate) async fn admin_rag_delete_document(
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
