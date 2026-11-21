//! Application Error Types and HTTP Response Hygiene.
//!
//! ARCH: §2 Full Component Map (User → API (Axum) → Auth+RateLimit → Domain)
//! Sanitizes internal database and unexpected server failures to prevent credential,
//! schema, or stack trace leakage to API clients.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

/// Root error type for API and domain workflows.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("raw_text must not be empty")]
    EmptyInput,

    #[error("task must not be empty")]
    EmptyTask,

    #[error("Input exceeds 8000 words")]
    InputTooLarge,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    #[error("Gateway timeout: {0}")]
    Timeout(String),

    #[error("Forbidden")]
    Forbidden,
}

impl AppError {
    /// Maps each error variant to an HTTP status code and deterministic error code.
    ///
    /// # Panics
    /// Never panics.
    #[must_use]
    pub fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::EmptyInput | AppError::EmptyTask | AppError::InputTooLarge => {
                (StatusCode::UNPROCESSABLE_ENTITY, "E001")
            }
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "E002"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "E003"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "E004"),
            AppError::Engine(_) => (StatusCode::BAD_REQUEST, "E005"),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "E006"),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, "E007"),
            AppError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "E008"),
            AppError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, "E009"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "E010"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "E099"),
        }
    }
}

impl IntoResponse for AppError {
    /// Converts domain errors into sanitized JSON HTTP responses with correlation IDs.
    ///
    /// Database (`E006`), internal (`E099`), and engine (`E005`) error messages are replaced with opaque
    /// descriptions to prevent leaking internal database schemas or server details,
    /// while the full error context is logged along with the correlation `error_id`.
    ///
    /// # Panics
    /// Never panics.
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let error_id = uuid::Uuid::new_v4();

        // ARCH: §2 API Boundary Error Hygiene — full error is traced internally with correlation UUID.
        tracing::error!(
            error_code = code,
            error_id = %error_id,
            error = %self
        );

        let message = match self {
            AppError::Database(_) => "Internal database error".to_string(),
            AppError::Internal(_) => "Internal server error".to_string(),
            AppError::Engine(_) => "Prompt processing error".to_string(),
            _ => self.to_string(),
        };

        (
            status,
            Json(serde_json::json!({
                "error": {
                    "code": code,
                    "message": message,
                    "error_id": error_id.to_string()
                }
            })),
        )
            .into_response()
    }
}
