use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use super::builder::CandidateRule;
use std::collections::HashMap;
use anyhow::Result;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeedbackEntry {
    pub id: String,
    pub raw_prompt: String,
    pub candidate: CandidateRule,
    pub status: FeedbackStatus,
    pub occurrence_count: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub promoted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum FeedbackStatus {
    Unmatched,
    Confirmed,
    Promoted,
    Rejected,
}

pub struct FeedbackStore {
    pub entries: HashMap<String, FeedbackEntry>,
    path: String,
}

impl FeedbackStore {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        if !path.as_ref().exists() {
            return Ok(Self { entries: HashMap::new(), path: path_str });
        }
        let content = std::fs::read_to_string(&path)?;
        let entries: HashMap<String, FeedbackEntry> = serde_json::from_str(&content)?;
        Ok(Self { entries, path: path_str })
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    pub fn record_unmatched(&mut self, raw: &str, candidate: &CandidateRule) {
        let fingerprint = format!("{}-{:?}", candidate.inferred_domain, candidate.trigger_words);
        let entry = self.entries.entry(fingerprint.clone()).or_insert_with(|| FeedbackEntry {
            id: uuid::Uuid::new_v4().to_string(),
            raw_prompt: raw.to_string(),
            candidate: candidate.clone(),
            status: FeedbackStatus::Unmatched,
            occurrence_count: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            promoted: false,
        });

        entry.occurrence_count += 1;
        entry.last_seen = Utc::now();
    }
}
