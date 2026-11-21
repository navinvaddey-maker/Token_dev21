use axum::http::{header, HeaderValue, Method};
use axum::middleware;
use dashmap::DashMap;
use dotenvy::dotenv;
use learning_engine::TenantLearningPool;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use token_compress_engine::pipeline::orchestrator::PipelineOrchestrator;
use token_compress_engine::AppState;
use tokio::sync::{Mutex, Semaphore};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn build_cors_layer() -> CorsLayer {
    let allowed: Vec<HeaderValue> = env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8081,http://127.0.0.1:8081".to_string())
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "*")
        .filter_map(|s| HeaderValue::from_str(s).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

use token_compress_engine::api::security_headers_middleware;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    // ARCH: §0 Security — Fail fast on missing or weak JWT_SECRET before binding any socket.
    token_compress_engine::domain::validate_security_env().map_err(anyhow::Error::msg)?;

    let db_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable must be set"))?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .min_connections(5)
        .max_connections(20)
        .idle_timeout(std::time::Duration::from_secs(300))
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&db_url)
        .await?;

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
        "#,
    )
    .execute(&pool)
    .await?;

    let ory_engine = Arc::new(Mutex::new(
        token_compress_engine::npae::ory::OryEngine::load_from_db(&pool).await?,
    ));

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let embedding_engine = Arc::new(token_compress_engine::rag::embeddings::EmbeddingEngine::new());

    // Seed ScenarioClassifier archetypes
    let classifier =
        token_compress_engine::classifier::ScenarioClassifier::new(&pool, &embedding_engine);
    if let Err(e) = classifier.seed_archetype_vectors_on_startup().await {
        tracing::error!("Failed to seed scenario archetypes: {:?}", e);
    }

    let learning_pool = Arc::new(TenantLearningPool::new(pool.clone(), 4096));

    // Load NPAE unified config once at startup for high-performance memory access
    let npae_config_data =
        token_compress_engine::npae::aggressive::config::ConfigLoader::load("config/unified.json")
            .map_err(|e| anyhow::anyhow!("Failed to load unified config: {e}"))?;
    let npae_config = Arc::new(
        token_compress_engine::npae::aggressive::config::ConfigHandle::new(npae_config_data),
    );

    let schema_priors: Arc<DashMap<String, u32>> = Arc::new(DashMap::new());
    let pipeline = Arc::new(PipelineOrchestrator::build_with_pool(
        schema_priors,
        npae_config.clone(),
        ory_engine.clone(),
        Some(pool.clone()),
    ));

    // Spawn FileWatcher background task
    let watcher_config_handle = npae_config.clone();
    tokio::task::spawn_blocking(move || {
        if let Ok(mut watcher) = token_compress_engine::npae::aggressive::watcher::FileWatcher::new(
            "config/unified.json",
        ) {
            loop {
                if watcher.wait_for_change(2000).is_ok() {
                    if let Ok(new_config) =
                        token_compress_engine::npae::aggressive::config::ConfigLoader::load(
                            "config/unified.json",
                        )
                    {
                        watcher_config_handle.swap(new_config);
                        tracing::info!("Hot-reloaded unified.json into memory");
                    }
                }
            }
        }
    });

    // Spawn ConfigEnricher background task (HumanGate by default)
    let enricher_config_handle = npae_config.clone();
    tokio::task::spawn_blocking(move || {
        let auto_merge = std::env::var("NPAE_AUTO_MERGE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let review_mode = if auto_merge {
            token_compress_engine::npae::aggressive::enricher::ReviewMode::AutoMerge
        } else {
            token_compress_engine::npae::aggressive::enricher::ReviewMode::HumanGate
        };
        let enricher =
            token_compress_engine::npae::aggressive::enricher::ConfigEnricher::new(review_mode);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if let Ok(mut store) =
                token_compress_engine::npae::aggressive::feedback::FeedbackStore::load(
                    "config/feedback.json",
                )
            {
                let mut current_config = enricher_config_handle.read();
                if let Ok(true) = enricher.process(&mut current_config, &mut store, 5) {
                    enricher_config_handle.swap(current_config.clone());
                    let _ = store.save();
                    let _ = token_compress_engine::npae::aggressive::config::ConfigLoader::save(
                        "config/unified.json",
                        &current_config,
                    );
                    tracing::info!("ConfigEnricher promoted matched feedback to unified.json");
                }
            }
        }
    });

    // Spawn Ory Pattern Memory background persistence task
    let ory_persistence_engine = ory_engine.clone();
    let ory_persistence_pool = pool.clone();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
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

    let rag_store = std::sync::Arc::new(token_compress_engine::rag::store::RagStore::new(
        pool.clone(),
    ));
    let sessions = std::sync::Arc::new(dashmap::DashMap::new());

    let domain_registry = Arc::new(
        token_compress_engine::scenario::ScenarioDomainRegistry::load_from_file(
            "config/scenario/domains.json",
        )
        .map_err(|e| anyhow::anyhow!("Failed to load Scenario Domain Registry: {e}"))?,
    );

    let style_registry = Arc::new(
        token_compress_engine::scenario::ScenarioStyleRegistry::load_from_file(
            "config/scenario/styles.json",
            "prompts/styles",
        )
        .map_err(|e| anyhow::anyhow!("Failed to load Scenario Style Registry: {e}"))?,
    );

    let egress_guard = Arc::new(
        token_compress_engine::scenario::ScenarioEgressGuard::load_from_file(
            "config/scenario/egress.json",
        )
        .map_err(|e| anyhow::anyhow!("Failed to load Egress Guard Config: {e}"))?,
    );

    let scenario_router = Arc::new(token_compress_engine::scenario::ScenarioModeRouter::new(
        domain_registry,
        style_registry,
        egress_guard,
        "prompts/styles".to_string(),
    ));

    // Optional explicit bootstrap admin provisioning (only when zero admins exist and strong password is provided).
    if let (Ok(admin_user), Ok(admin_pass)) = (
        env::var("BOOTSTRAP_ADMIN_USERNAME"),
        env::var("BOOTSTRAP_ADMIN_PASSWORD"),
    ) {
        let admin_count = token_compress_engine::data::Repository::count_admins(&pool).await?;
        if admin_count == 0 && admin_pass.len() >= 16 && admin_user.trim().len() >= 3 {
            let pw_hash = bcrypt::hash(&admin_pass, bcrypt::DEFAULT_COST)
                .map_err(|e| anyhow::anyhow!("Admin bootstrap bcrypt failed: {e}"))?;
            let id = uuid::Uuid::new_v4().to_string();
            let email = format!("{}@admin.local", admin_user.trim());
            token_compress_engine::data::Repository::create_user_scoped(
                &pool,
                &id,
                admin_user.trim(),
                &pw_hash,
                &email,
                "Business",
                "admin",
                "default",
            )
            .await?;
            tracing::info!("Bootstrapped initial admin user via explicit env vars");
        }
    }

    let state = AppState {
        pool: pool.clone(),
        learning_pool,
        pipeline,
        npae_config,
        ory_engine: ory_engine.clone(),
        rag_store,
        sessions,
        embedding_engine,
        scenario_router,
        pdf_ingest_sem: Arc::new(Semaphore::new(8)),
        aggressive_sem: Arc::new(Semaphore::new(32)),
        compress_sem: Arc::new(Semaphore::new(64)),
    };

    let api_router = token_compress_engine::api::router(state);

    let app = api_router
        .nest_service("/", ServeDir::new("static"))
        .layer(build_cors_layer())
        .layer(middleware::from_fn(security_headers_middleware));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await?;
    tracing::info!("TokenCompress Engine listening on 0.0.0.0:8081");

    let final_ory_engine = ory_engine.clone();
    let final_ory_pool = pool.clone();

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install CTRL+C handler: {:?}", e);
            return;
        }
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
