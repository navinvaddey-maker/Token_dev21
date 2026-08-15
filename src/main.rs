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
    
    // Ensure Ory table exists (standardizing on SQLite for Ory memory)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS learned_patterns (
            pattern_id         TEXT PRIMARY KEY,
            domain_fingerprint TEXT NOT NULL,
            intent_fingerprint TEXT NOT NULL,
            blueprint_json     TEXT NOT NULL,
            usage_count        INTEGER NOT NULL,
            success_rate       REAL NOT NULL,
            last_used          TIMESTAMP NOT NULL,
            created_at         TIMESTAMP NOT NULL
        )
        "#
    )
    .execute(&pool)
    .await?;

    let ory_engine = Arc::new(Mutex::new(token_compress_engine::npae::ory::OryEngine::load_from_db(&pool).await?));

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let engine = Arc::new(Mutex::new(LearningEngine::new(pool.clone())));

    // Load NPAE unified config once at startup for high-performance memory access
    let npae_config_data = token_compress_engine::npae::aggressive::config::ConfigLoader::load("config/unified.json")
        .unwrap_or_else(|e| panic!("Failed to load unified config: {}", e));
    let npae_config = Arc::new(token_compress_engine::npae::aggressive::config::ConfigHandle::new(npae_config_data));

    // Build the 7-stage pipeline orchestrator with shared schema priors.
    // The DashMap is shared across all requests — enables cross-request schema learning
    // (prediction error drops 20–35% over 10 turns on familiar topics).
    let schema_priors: Arc<DashMap<String, u32>> = Arc::new(DashMap::new());
    let pipeline = Arc::new(PipelineOrchestrator::build(schema_priors, npae_config.clone(), ory_engine.clone()));

    // Spawn FileWatcher background task
    let watcher_config_handle = npae_config.clone();
    tokio::task::spawn_blocking(move || {
        if let Ok(mut watcher) = token_compress_engine::npae::aggressive::watcher::FileWatcher::new("config/unified.json") {
            loop {
                if watcher.wait_for_change(2000).is_ok() {
                    if let Ok(new_config) = token_compress_engine::npae::aggressive::config::ConfigLoader::load("config/unified.json") {
                        watcher_config_handle.swap(new_config);
                        tracing::info!("Hot-reloaded unified.json into memory");
                    }
                }
            }
        }
    });

    // Spawn ConfigEnricher background task
    let enricher_config_handle = npae_config.clone();
    tokio::task::spawn_blocking(move || {
        let enricher = token_compress_engine::npae::aggressive::enricher::ConfigEnricher::new(
            token_compress_engine::npae::aggressive::enricher::ReviewMode::AutoMerge
        );
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if let Ok(mut store) = token_compress_engine::npae::aggressive::feedback::FeedbackStore::load("config/feedback.json") {
                let mut current_config = enricher_config_handle.read();
                if let Ok(true) = enricher.process(&mut current_config, &mut store, 5) {
                    enricher_config_handle.swap(current_config.clone());
                    let _ = store.save();
                    let _ = token_compress_engine::npae::aggressive::config::ConfigLoader::save("config/unified.json", &current_config);
                    tracing::info!("ConfigEnricher promoted matched feedback to unified.json");
                }
            }
        }
    });

    // Spawn Ory Pattern Memory background persistence task (GAP-N02)
    let ory_persistence_engine = ory_engine.clone();
    let ory_persistence_pool = pool.clone();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // every 5 mins
        loop {
            interval.tick().await;
            let mut engine = ory_persistence_engine.lock().await;
            if engine.is_dirty() {
                if let Err(e) = engine.save_to_db(&ory_persistence_pool).await {
                    tracing::error!("Failed to persist Ory memory: {:?}", e);
                } else {
                    tracing::info!("Ory Pattern Memory persisted to SQLite");
                }
            }
        }
    });

    let rag_store = std::sync::Arc::new(token_compress_engine::rag::store::RagStore::new(pool.clone()));
    let sessions = std::sync::Arc::new(dashmap::DashMap::new());

    let state = AppState {
        pool: pool.clone(),
        engine,
        pipeline,
        npae_config,
        ory_engine: ory_engine.clone(),
        rag_store,
        sessions,
    };

    let api_router = token_compress_engine::api::router(state);

    // Seed admin user if not exists, or update business_type to admin if exists
    match token_compress_engine::data::Repository::find_user_by_username(&pool, "navin").await {
        Ok(None) => {
            tracing::info!("Seeding admin user: navin");
            let admin_password = env::var("ADMIN_PASSWORD").unwrap_or_else(|_| {
                tracing::warn!("⚠️  ADMIN_PASSWORD not set — using generated fallback. Set ADMIN_PASSWORD env var for production!");
                format!("admin_fallback_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000"))
            });
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
            if let Ok(admin_password) = env::var("ADMIN_PASSWORD") {
                tracing::info!("ADMIN_PASSWORD is set, updating admin user 'navin' password");
                let pw_hash = bcrypt::hash(&admin_password, bcrypt::DEFAULT_COST).unwrap();
                if let Err(e) = token_compress_engine::data::Repository::update_user_password(
                    &pool, &user.id, &pw_hash,
                )
                .await
                {
                    tracing::error!("Failed to update password for user 'navin': {:?}", e);
                } else {
                    tracing::info!("Updated password for user 'navin'.");
                }
            }

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

    // Graceful shutdown with Ory memory persistence
    let final_ory_engine = ory_engine.clone();
    let final_ory_pool = pool.clone();
    
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install CTRL+C handler");
            tracing::info!("Shutdown signal received. Persisting Ory memory...");
            let mut engine = final_ory_engine.lock().await;
            if let Err(e) = engine.save_to_db(&final_ory_pool).await {
                tracing::error!("Final Ory persistence failed: {:?}", e);
            } else {
                tracing::info!("Ory memory safely persisted. Goodbye!");
            }
        })
        .await?;
    Ok(())
}
