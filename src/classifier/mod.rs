pub mod expertise;
pub mod intent_rule;
pub mod signal;
pub mod similarity;

use crate::rag::embeddings::EmbeddingEngine;
use expertise::detect_expertise_from_query;
use intent_rule::{hash_margin_allows, resolve_prompt_rules};
use signal::{ScenarioRow, ScenarioSignal};
use similarity::{
    compute_confidence_from_rankings, deserialize_vector_blob, select_top_k_archetypes,
};
use sqlx::SqlitePool;

/// Number of archetypes to rank during top-k selection.
const ARCHETYPE_TOP_K: usize = 3;

/// Minimum confidence below which the classifier returns None,
/// deferring entirely to PredictiveCoding in Stage 2.
const MIN_CONFIDENCE_THRESHOLD: f32 = 0.60;

/// The ScenarioClassifier runs inside the Domain Layer, before the PipelineOrchestrator.
pub struct ScenarioClassifier<'a> {
    pool: &'a SqlitePool,
    engine: &'a EmbeddingEngine,
}

impl<'a> ScenarioClassifier<'a> {
    pub fn new(pool: &'a SqlitePool, engine: &'a EmbeddingEngine) -> Self {
        Self { pool, engine }
    }

    /// Classifies a raw user query into a typed ScenarioSignal.
    pub async fn classify_query_into_scenario(&self, raw_query: &str) -> Option<ScenarioSignal> {
        if let Some(verdict) = resolve_prompt_rules(raw_query) {
            let detected_expertise = detect_expertise_from_query(raw_query).to_string();
            return Some(ScenarioSignal {
                domain: verdict.pipeline_domain().to_string(),
                task_type: verdict.task_type().to_string(),
                expertise: detected_expertise,
                intent_class: verdict.intent_class().to_string(),
                confidence: 0.95,
                secondary_domain: verdict.secondary_domain().map(|s| s.to_string()),
                primary_role: Some(verdict.primary_role.to_string()),
                rule_locked: true,
                legal_as_constraint: verdict.legal_as_constraint,
            });
        }

        // Step 1: Vectorize the raw query.
        let query_vec = self.engine.embed(raw_query);

        // Step 2: Load all seeded archetypes from the scenario_index table.
        let rows = self
            .load_seeded_archetype_rows()
            .await
            .map_err(|e| {
                tracing::warn!("ScenarioClassifier: failed to load archetypes: {}", e);
            })
            .ok()?;

        if rows.is_empty() {
            tracing::warn!("ScenarioClassifier: scenario_index is empty — returning None");
            return None;
        }

        // Step 3: Build candidate list for top-k comparison.
        let candidates: Vec<(Vec<f32>, String, String, String, String)> = rows
            .iter()
            .filter(|row| !row.vector_blob.is_empty())
            .map(|row| {
                (
                    deserialize_vector_blob(&row.vector_blob),
                    row.domain.clone(),
                    row.task_type.clone(),
                    row.expertise.clone(),
                    row.intent_class.clone(),
                )
            })
            .collect();

        // Step 4: Rank archetypes by cosine similarity to the query vector.
        let ranked = select_top_k_archetypes(&query_vec, &candidates, ARCHETYPE_TOP_K);

        if ranked.is_empty() {
            return None;
        }

        // Step 5: Compute weighted confidence from top-k scores.
        let confidence = compute_confidence_from_rankings(&ranked);
        let second = ranked.get(1).map(|r| r.score);

        if !hash_margin_allows(ranked[0].score, second, MIN_CONFIDENCE_THRESHOLD)
            || confidence < MIN_CONFIDENCE_THRESHOLD
        {
            tracing::debug!(
                "ScenarioClassifier: abstaining (confidence {:.2}, top {:.2}, second {:?})",
                confidence,
                ranked[0].score,
                second
            );
            return None;
        }

        // Step 6: Take the top-ranked archetype as the base signal.
        let best = &ranked[0];

        // Step 7: Override expertise using the rule-based detector.
        let detected_expertise = detect_expertise_from_query(raw_query).to_string();

        Some(ScenarioSignal {
            domain: best.domain.clone(),
            task_type: best.task_type.clone(),
            expertise: detected_expertise,
            intent_class: best.intent_class.clone(),
            confidence,
            secondary_domain: None,
            primary_role: None,
            rule_locked: false,
            legal_as_constraint: false,
        })
    }

    /// Seeds the scenario_index table by embedding all unseeded archetype example queries.
    pub async fn seed_archetype_vectors_on_startup(
        &self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unseeded_rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, example_query FROM scenario_index WHERE length(vector_blob) = 0",
        )
        .fetch_all(self.pool)
        .await?;

        let count = unseeded_rows.len();
        for (id, example_query) in unseeded_rows {
            let vector = self.engine.embed(&example_query);
            let blob = crate::rag::types::embedding_to_bytes(&vector);
            sqlx::query("UPDATE scenario_index SET vector_blob = ?1 WHERE id = ?2")
                .bind(blob)
                .bind(id)
                .execute(self.pool)
                .await?;
        }

        tracing::info!("ScenarioClassifier: seeded {} archetype vectors", count);
        Ok(())
    }

    async fn load_seeded_archetype_rows(&self) -> Result<Vec<ScenarioRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, Vec<u8>)>(
            "SELECT label, domain, task_type, expertise, intent_class, vector_blob
             FROM scenario_index
             WHERE length(vector_blob) > 0",
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(label, domain, task_type, expertise, intent_class, vector_blob)| ScenarioRow {
                    label,
                    domain,
                    task_type,
                    expertise,
                    intent_class,
                    vector_blob,
                },
            )
            .collect())
    }
}
