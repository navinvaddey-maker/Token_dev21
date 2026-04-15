use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct UseCase {
    pub id: String,
    pub key: String,
    pub version: String,
    pub role_frame: String,
    pub output_format: String,
    pub chunk_strategy: String,
    pub description: Option<String>,
    pub active: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// GAP-20: Versioning and Rollback Protocol for Prompt Frames
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptFrameVersion {
    pub version_id: uuid::Uuid,
    pub parent_version_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tes_score_at_creation: f32,
    pub sfs_score_at_creation: f32,
    pub is_active: bool,
}

pub struct DualScore {
    pub tes: f32,
    pub sfs: f32,
}
impl DualScore {
    pub fn combined(&self) -> f32 { self.tes + self.sfs }
}

const DEGRADATION_THRESHOLD: f32 = 1.0;

#[async_trait::async_trait]
pub trait PromptFrameRepository {
    async fn rollback_to(&self, version_id: uuid::Uuid) -> Result<(), sqlx::Error>;
    
    async fn find_last_stable_version(&self) -> Result<PromptFrameVersion, sqlx::Error>;
    
    async fn auto_rollback_if_degraded(
        &self, 
        current_scores: DualScore, 
        baseline: DualScore
    ) -> Result<(), sqlx::Error> {
        if current_scores.combined() < baseline.combined() - DEGRADATION_THRESHOLD {
            let stable = self.find_last_stable_version().await?;
            self.rollback_to(stable.version_id).await?;
        }
        Ok(())
    }
}

