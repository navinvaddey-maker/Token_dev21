use super::config::{UnifiedConfig, ConstraintRule, WeightedWord};
use super::feedback::{FeedbackStore, FeedbackStatus, FeedbackEntry};
use anyhow::Result;
use std::path::Path;

pub enum ReviewMode {
    AutoMerge,   // frequency trusted — merge directly into config.json
    HumanGate,   // write to proposals/ — human approves
    DryRun,      // log only — never write
}

pub struct ConfigEnricher {
    mode: ReviewMode,
}

impl ConfigEnricher {
    pub fn new(mode: ReviewMode) -> Self {
        Self { mode }
    }

    pub fn process(&self, config: &mut UnifiedConfig, store: &mut FeedbackStore, threshold: u32) -> Result<bool> {
        let mut changed = false;
        let mut promoted_ids = Vec::new();

        for (fingerprint, entry) in &mut store.entries {
            if entry.status == FeedbackStatus::Unmatched && entry.occurrence_count >= threshold {
                match self.mode {
                    ReviewMode::AutoMerge => {
                        let new_rule = self.create_rule(entry);
                        config.constraints.push(new_rule);
                        entry.status = FeedbackStatus::Promoted;
                        entry.promoted = true;
                        promoted_ids.push(fingerprint.clone());
                        changed = true;
                    }
                    ReviewMode::HumanGate => {
                        self.write_proposal(entry)?;
                        entry.status = FeedbackStatus::Confirmed; // Mark as "pending" basically
                    }
                    ReviewMode::DryRun => {
                        tracing::info!("DryRun: Would promote rule for fingerprint {}", fingerprint);
                    }
                }
            }
        }

        if changed {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn create_rule(&self, entry: &FeedbackEntry) -> ConstraintRule {
        ConstraintRule {
            id: format!("auto_{}", entry.id),
            domain: entry.candidate.inferred_domain.clone(),
            priority: 2, // Default low priority for auto-generated rules
            is_forbidden: entry.candidate.inferred_intent == "forbidden",
            description: format!("Auto-generated rule for terms: {:?}", entry.candidate.trigger_words),
            trigger: vec![
                entry.candidate.trigger_words.iter().map(|w| WeightedWord {
                    word: w.clone(),
                    weight: 7, // Default weight
                    synonyms: None,
                }).collect()
            ],
        }
    }

    fn write_proposal(&self, entry: &FeedbackEntry) -> Result<()> {
        let proposal_dir = Path::new("proposals");
        if !proposal_dir.exists() {
            std::fs::create_dir_all(proposal_dir)?;
        }
        let proposal_path = proposal_dir.join(format!("auto_{}.json", entry.id));
        let rule = self.create_rule(entry);
        let content = serde_json::to_string_pretty(&rule)?;
        std::fs::write(proposal_path, content)?;
        Ok(())
    }
}
