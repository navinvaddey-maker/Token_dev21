//! Neuro-symbolic online learning engine.
//!
//! Integrates Hebbian co-activation learning and competitive neural clustering
//! with multi-tenant isolation via `TenantLearningPool`.
//! ARCH: ARCHITECTURE.md §1.2, §2, §4 — Multi-tenant neural isolation.

use std::collections::HashMap;
use std::sync::Arc;

use competitive_core::CompetitiveNet;
use db::{queries::*, DbPool};
use hebbian_core::HebbianNet;
use schema_engine::{
    constraints::ConstraintSet,
    extractor::{RuleBasedExtractor, SchemaExtractor},
};
use uuid::Uuid;

/// Online neuro-symbolic learning engine instance for an individual user/tenant context.
pub struct LearningEngine {
    extractor: Box<dyn SchemaExtractor + Send + Sync>,
    constraints: ConstraintSet,
    hebbian: HebbianNet,
    competitive: CompetitiveNet,
    pool: DbPool,
}

impl std::fmt::Debug for LearningEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LearningEngine")
            .field("constraints", &self.constraints)
            .field("hebbian", &self.hebbian)
            .field("competitive", &self.competitive)
            .finish()
    }
}

/// Returned to the token optimizer binary.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub cluster_id: usize,
    pub is_new_domain: bool,
    /// Pre-loaded domain vocabulary — prepend to next LLM call.
    pub cluster_vocab: Vec<(String, f32)>,
    pub blocked: bool,
}

impl LearningEngine {
    /// Constructs a new `LearningEngine` instance.
    pub fn new(pool: DbPool) -> Self {
        Self {
            extractor: Box::new(RuleBasedExtractor),
            constraints: ConstraintSet::default_rust(),
            hebbian: HebbianNet::new(0.01, 0.001),
            competitive: CompetitiveNet::new(
                /*n_clusters*/ 8, /*learning_rate*/ 0.05, /*novelty_threshold*/ 0.25,
            ),
            pool,
        }
    }

    /// Called on every user prompt, defaulting to 'default' tenant.
    pub async fn process(
        &mut self,
        user_id: Uuid,
        prompt: &str,
    ) -> Result<ProcessResult, sqlx::Error> {
        self.process_scoped("default", user_id, prompt).await
    }

    /// Called on every user prompt with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on database failure during state persistence.
    pub async fn process_scoped(
        &mut self,
        tenant_id: &str,
        user_id: Uuid,
        prompt: &str,
    ) -> Result<ProcessResult, sqlx::Error> {
        // ── 1. Schema + constraint check → single TokenActivation ──────
        let activation = self.extractor.activate(prompt, &self.constraints);
        let schema = self.extractor.extract(prompt);

        // ── 2. Hebbian: token co-activation weights ────────────────────
        self.hebbian.update(&activation);

        // ── 3. Competitive: domain cluster drift ───────────────────────
        let cluster = self.competitive.update(&activation);

        // ── 4. Pull winner cluster vocab for prompt enrichment ─────────
        let cluster_vocab = self.competitive.cluster_vocabulary(cluster.winner_id, 20);

        // ── 5. Persist via SQLx with tenant scope ──────────────────────
        save_prompt_scoped(&self.pool, tenant_id, user_id, prompt, &schema).await?;

        self.persist_state_scoped(tenant_id, user_id).await?;

        Ok(ProcessResult {
            cluster_id: cluster.winner_id,
            is_new_domain: cluster.is_new_cluster,
            cluster_vocab,
            blocked: activation.blocked,
        })
    }

    /// Process explicit feedback (thumbs up/down) defaulting to 'default' tenant.
    pub async fn apply_feedback(
        &mut self,
        user_id: Uuid,
        prompt: &str,
        value: f32,
    ) -> Result<(), sqlx::Error> {
        self.apply_feedback_scoped("default", user_id, prompt, value)
            .await
    }

    /// Process explicit feedback with explicit tenant scoping.
    ///
    /// # Errors
    /// Returns `sqlx::Error` on database failure during state persistence.
    pub async fn apply_feedback_scoped(
        &mut self,
        tenant_id: &str,
        user_id: Uuid,
        prompt: &str,
        value: f32,
    ) -> Result<(), sqlx::Error> {
        // 1. Re-generate activation for the prompt
        let activation = self.extractor.activate(prompt, &self.constraints);

        // 2. Apply feedback to both networks
        self.hebbian.apply_feedback(&activation, value);
        self.competitive.apply_feedback(&activation, value);

        // 3. Persist updated state with tenant scope
        self.persist_state_scoped(tenant_id, user_id).await
    }

    /// Persists neural state for a user and tenant.
    async fn persist_state_scoped(
        &self,
        tenant_id: &str,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let top_pairs = self.hebbian.top_associations(50);
        upsert_learned_context_scoped(&self.pool, tenant_id, user_id, &top_pairs).await?;

        let snapshots = self.competitive.snapshot();
        upsert_domain_profile_scoped(&self.pool, tenant_id, user_id, &snapshots).await?;

        Ok(())
    }
}

#[derive(Debug)]
struct CachedEngine {
    engine: Arc<tokio::sync::Mutex<LearningEngine>>,
    last_used_ms: std::sync::atomic::AtomicU64,
}

/// Bounded pool of per-(tenant_id, user_id) `LearningEngine` instances.
///
/// Ensures neural weights (Hebbian co-activations & competitive cluster centroids)
/// are strictly isolated across tenants and users, preventing cross-tenant leakage
/// and avoiding contention on a single global lock.
#[derive(Debug)]
pub struct TenantLearningPool {
    pool: DbPool,
    max_entries: usize,
    created_at: std::time::Instant,
    engines: tokio::sync::RwLock<HashMap<(String, Uuid), CachedEngine>>,
}

impl TenantLearningPool {
    /// Creates a new `TenantLearningPool` with a maximum capacity.
    pub fn new(pool: DbPool, max_entries: usize) -> Self {
        Self {
            pool,
            max_entries: if max_entries == 0 { 4096 } else { max_entries },
            created_at: std::time::Instant::now(),
            engines: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    fn now_ms(&self) -> u64 {
        self.created_at.elapsed().as_millis() as u64
    }

    /// Retrieves or instantiates an isolated `LearningEngine` for `(tenant_id, user_id)`.
    ///
    /// Uses a read-lock fast path with atomic LRU timestamp tracking, and performs database
    /// hydration outside the write lock so slow DB queries for one tenant do not stall others.
    pub async fn get_engine(
        &self,
        tenant_id: &str,
        user_id: Uuid,
    ) -> Arc<tokio::sync::Mutex<LearningEngine>> {
        let key = (tenant_id.to_string(), user_id);
        let ts = self.now_ms();

        // Fast path: shared read lock check without blocking other readers or holding lock over DB I/O
        {
            let read_guard = self.engines.read().await;
            if let Some(entry) = read_guard.get(&key) {
                entry
                    .last_used_ms
                    .store(ts, std::sync::atomic::Ordering::Relaxed);
                return Arc::clone(&entry.engine);
            }
        }

        // Hydrate outside the RwLock so DB `.await` never blocks unrelated tenants
        let mut engine = LearningEngine::new(self.pool.clone());
        if let Ok(deltas) = load_learned_context_scoped(&self.pool, tenant_id, user_id).await {
            for delta in deltas {
                engine.hebbian.weights.insert(delta.pair, delta.new_weight);
            }
        }
        if let Ok(snapshots) = load_domain_profiles_scoped(&self.pool, tenant_id, user_id).await {
            if !snapshots.is_empty() {
                engine.competitive.centroids = snapshots
                    .into_iter()
                    .map(|snap| {
                        let vector: std::collections::HashMap<String, f32> =
                            snap.top_tokens.into_iter().collect();
                        competitive_core::centroid::Centroid {
                            id: snap.id,
                            label: snap.label,
                            vector,
                            hits: snap.hits,
                        }
                    })
                    .collect();
            }
        }

        // Slow path: write lock insertion with double-check and LRU eviction
        let mut write_guard = self.engines.write().await;
        if let Some(entry) = write_guard.get(&key) {
            entry
                .last_used_ms
                .store(self.now_ms(), std::sync::atomic::Ordering::Relaxed);
            return Arc::clone(&entry.engine);
        }

        if write_guard.len() >= self.max_entries {
            let oldest_key = write_guard
                .iter()
                .min_by_key(|(_, entry)| {
                    entry
                        .last_used_ms
                        .load(std::sync::atomic::Ordering::Relaxed)
                })
                .map(|(k, _)| k.clone());
            if let Some(evict_key) = oldest_key {
                write_guard.remove(&evict_key);
            }
        }

        let new_engine = Arc::new(tokio::sync::Mutex::new(engine));
        write_guard.insert(
            key,
            CachedEngine {
                engine: Arc::clone(&new_engine),
                last_used_ms: std::sync::atomic::AtomicU64::new(self.now_ms()),
            },
        );
        new_engine
    }

    /// Processes a prompt through the tenant/user's isolated engine instance.
    ///
    /// # Errors
    /// Returns `sqlx::Error` if persistence fails.
    pub async fn process(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        prompt: &str,
    ) -> Result<ProcessResult, sqlx::Error> {
        let engine_arc = self.get_engine(tenant_id, user_id).await;
        let mut engine = engine_arc.lock().await;
        engine.process_scoped(tenant_id, user_id, prompt).await
    }

    /// Applies explicit feedback to the tenant/user's isolated engine instance.
    ///
    /// # Errors
    /// Returns `sqlx::Error` if persistence fails.
    pub async fn apply_feedback(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        prompt: &str,
        value: f32,
    ) -> Result<(), sqlx::Error> {
        let engine_arc = self.get_engine(tenant_id, user_id).await;
        let mut engine = engine_arc.lock().await;
        engine
            .apply_feedback_scoped(tenant_id, user_id, prompt, value)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tenant_learning_pool_isolation() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        let learning_pool = TenantLearningPool::new(pool, 10);
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();

        let engine_t1_u1 = learning_pool.get_engine("t1", u1).await;
        let engine_t1_u1_again = learning_pool.get_engine("t1", u1).await;
        let engine_t2_u1 = learning_pool.get_engine("t2", u1).await;
        let engine_t1_u2 = learning_pool.get_engine("t1", u2).await;

        assert!(Arc::ptr_eq(&engine_t1_u1, &engine_t1_u1_again));
        assert!(!Arc::ptr_eq(&engine_t1_u1, &engine_t2_u1));
        assert!(!Arc::ptr_eq(&engine_t1_u1, &engine_t1_u2));
    }

    #[tokio::test]
    async fn test_tenant_learning_pool_hydration_after_eviction() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        sqlx::query(
            "CREATE TABLE user_prompts (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL, tenant_id TEXT NOT NULL DEFAULT 'default',
                raw_text TEXT NOT NULL, schema_json TEXT NOT NULL, token_count INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .execute(&pool)
        .await
        .expect("create user_prompts");

        sqlx::query(
            "CREATE TABLE learned_context (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL UNIQUE, tenant_id TEXT NOT NULL DEFAULT 'default',
                top_pairs TEXT NOT NULL, updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .execute(&pool)
        .await
        .expect("create learned_context");

        sqlx::query(
            "CREATE TABLE user_domain_profile (
                id TEXT PRIMARY KEY, user_id TEXT NOT NULL, tenant_id TEXT NOT NULL DEFAULT 'default',
                cluster_id INTEGER NOT NULL, cluster_label TEXT, top_tokens TEXT NOT NULL,
                hits INTEGER NOT NULL DEFAULT 1, updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(user_id, cluster_id)
            );",
        )
        .execute(&pool)
        .await
        .expect("create user_domain_profile");

        let learning_pool = TenantLearningPool::new(pool, 1);
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();

        learning_pool
            .process("t1", u1, "draft legal indemnity clause review")
            .await
            .expect("process u1");
        learning_pool
            .apply_feedback("t1", u1, "draft legal indemnity clause review", 1.0)
            .await
            .expect("feedback u1");

        // Trigger LRU eviction of u1 by loading u2 with max_entries=1
        learning_pool
            .process("t1", u2, "unrelated software deployment script")
            .await
            .expect("process u2");

        // Re-load u1 from DB and verify weights were hydrated rather than blank
        let u1_rehydrated = learning_pool.get_engine("t1", u1).await;
        let guard = u1_rehydrated.lock().await;
        assert!(
            !guard.hebbian.weights.is_empty(),
            "Hebbian weights should hydrate from DB after LRU eviction"
        );
    }
}
