use competitive_core::CompetitiveNet;
use db::{queries::*, DbPool};
use hebbian_core::HebbianNet;
use schema_engine::{
    constraints::ConstraintSet,
    extractor::{RuleBasedExtractor, SchemaExtractor},
};
use uuid::Uuid;

pub struct LearningEngine {
    extractor: Box<dyn SchemaExtractor + Send + Sync>,
    constraints: ConstraintSet,
    hebbian: HebbianNet,
    competitive: CompetitiveNet,
    pool: DbPool,
}

/// Returned to the token optimizer binary.
#[derive(Debug)]
pub struct ProcessResult {
    pub cluster_id: usize,
    pub is_new_domain: bool,
    /// Pre-loaded domain vocabulary — prepend to next LLM call.
    pub cluster_vocab: Vec<(String, f32)>,
    pub blocked: bool,
}

impl LearningEngine {
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

    /// Called on every user prompt.
    /// Both neuro engines run in sequence, sharing one TokenActivation.
    pub async fn process(
        &mut self,
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

        // ── 5. Persist via SQLx ────────────────────────────────────────
        save_prompt(&self.pool, user_id, prompt, &schema).await?;

        let top_pairs = self.hebbian.top_associations(50);
        upsert_learned_context(&self.pool, user_id, &top_pairs).await?;

        let snapshots = self.competitive.snapshot();
        upsert_domain_profile(&self.pool, user_id, &snapshots).await?;

        Ok(ProcessResult {
            cluster_id: cluster.winner_id,
            is_new_domain: cluster.is_new_cluster,
            cluster_vocab,
            blocked: activation.blocked,
        })
    }
}
