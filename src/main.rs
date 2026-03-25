use dotenvy::dotenv;
use sqlx::sqlite::SqlitePoolOptions;
use std::env;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:token_compress.db?mode=rwc".into());
    let pool   = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Enable WAL mode for better concurrent access
    sqlx::query("PRAGMA journal_mode=WAL;").execute(&pool).await?;
    // Enable foreign keys
    sqlx::query("PRAGMA foreign_keys=ON;").execute(&pool).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let api_router = token_compress_engine::api::router(pool.clone());

    // Seeding admin user if it does not exist
    match token_compress_engine::data::Repository::find_user_by_username(&pool, "navin").await {
        Ok(None) => {
            tracing::info!("Seeding admin user: navin");
            let req = token_compress_engine::models::user::RegisterRequest {
                username: "navin".to_string(),
                password: "Ganapathi@21".to_string(),
                email: "navin@admin.local".to_string(),
                business_type: Some("Admin".to_string()),
            };
            if let Err(e) = token_compress_engine::domain::register_user(&pool, req).await {
                tracing::error!("Failed to seed admin user: {:?}", e);
            } else {
                tracing::info!("Admin user seeded successfully.");
            }
        }
        Ok(Some(_)) => {
            tracing::info!("Admin user 'navin' already exists.");
        }
        Err(e) => {
            tracing::error!("Failed to check for admin user: {:?}", e);
        }
    }

    // Serve static files from the `static/` directory
    let app = api_router.nest_service("/", ServeDir::new("static"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await?;
    tracing::info!("TokenCompress Engine listening on 0.0.0.0:8081");
    axum::serve(listener, app).await?;
    Ok(())
}
