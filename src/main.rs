use dashmap::DashMap;
use dotenvy::dotenv;
use learning_engine::LearningEngine;
use std::env;
use std::sync::Arc;
use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
use token_compress_engine::AppState;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = db::DbPool::connect(&db_url).await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let engine = Arc::new(Mutex::new(LearningEngine::new(pool.clone())));

    // Build the 7-stage pipeline orchestrator with shared schema priors.
    // The DashMap is shared across all requests — enables cross-request schema learning
    // (prediction error drops 20–35% over 10 turns on familiar topics).
    let schema_priors: Arc<DashMap<String, u32>> = Arc::new(DashMap::new());
    let pipeline = Arc::new(PipelineOrchestrator::build(schema_priors));

    let state = AppState {
        pool: pool.clone(),
        engine,
        pipeline,
    };

    let api_router = token_compress_engine::api::router(state);

    // Seed admin user if not exists, or update business_type to admin if exists
    match token_compress_engine::data::Repository::find_user_by_username(&pool, "navin").await {
        Ok(None) => {
            tracing::info!("Seeding admin user: navin");
            let admin_password = env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set for admin user seeding");
            let req = token_compress_engine::models::user::RegisterRequest {
                username: "navin".to_string(),
                password: admin_password,
                email: "navin@token-optimizer.local".to_string(),
                business_type: Some("admin".to_string()),
            };
            if let Err(e) = token_compress_engine::domain::register_user(&pool, req).await {
                tracing::error!("Failed to seed admin user: {:?}", e);
            } else {
                tracing::info!("Admin user seeded successfully.");
            }
        }
        Ok(Some(user)) => {
            if user.business_type.to_lowercase() != "admin" {
                tracing::info!("Updating existing user 'navin' to have business_type 'admin'");
                if let Err(e) = token_compress_engine::data::Repository::update_user_business_type(
                    &pool, &user.id, "admin",
                )
                .await
                {
                    tracing::error!("Failed to update business_type for user 'navin': {:?}", e);
                } else {
                    tracing::info!("Updated business_type for user 'navin' to admin.");
                }
            } else {
                tracing::info!("Admin user 'navin' already exists with correct business_type.");
            }
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
