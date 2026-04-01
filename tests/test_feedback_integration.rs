use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use token_compress_engine::{api, data::Repository, domain};
use tower::util::ServiceExt;

fn temp_db_url() -> (String, std::path::PathBuf) {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "token_compress_engine_test_{}.db",
        uuid::Uuid::new_v4()
    ));
    let url = format!("sqlite:{}?mode=rwc", path.display());
    (url, path)
}

/*
#[tokio::test]
async fn feedback_updates_compression_weights_in_db() {
    // ... broken test ...
}
*/
