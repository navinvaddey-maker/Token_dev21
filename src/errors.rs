use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

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
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
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
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "E099"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        tracing::error!(error_code = code, error = %self);
        (
            status,
            Json(serde_json::json!({ "error": { "code": code, "message": self.to_string() } })),
        )
            .into_response()
    }
}
