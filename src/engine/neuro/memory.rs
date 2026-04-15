use uuid::Uuid;
use crate::types::{Mode, WmSlot, SlotSource};

pub const MAX_WM_CAPACITY: usize = 7;

#[derive(Debug, Default)]
pub struct WorkingMemory {
    pub slots:    Vec<WmSlot>,
    pub capacity: usize,
    pub warm_start_applied: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WmError {
    #[error("DB fetch failed during warm start: {0}")]
    DbFetch(String),
}

impl WorkingMemory {
    /// Warm start: seeds WM with user priors from DB.
    pub async fn warm_start(
        _user_id: Uuid,
        _db:      &() // Mock db connection
    ) -> Result<Self, WmError> {
        let priors: Vec<WmSlot> = vec![]; 

        let slots: Vec<WmSlot> = priors
            .into_iter()
            .take(MAX_WM_CAPACITY)
            .map(|mut p| {
                p.salience = 0.30;
                p.source = SlotSource::Delta; 
                p
            })
            .collect();

        Ok(Self {
            slots,
            capacity: MAX_WM_CAPACITY,
            warm_start_applied: true,
        })
    }

    /// Called in Stage 3 after mode is known. Narrows capacity and evicts.
    pub fn set_capacity(&mut self, mode: &Mode) {
        self.capacity = match mode {
            Mode::Gentle => 3,
            Mode::Ambiguous | Mode::Balanced => 5, // Balanced substitute
            Mode::Aggressive => 7,
        };

        while self.slots.len() > self.capacity {
            let min_idx = self.slots
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let sa = if a.1.salience.is_nan() { 0.0 } else { a.1.salience };
                    let sb = if b.1.salience.is_nan() { 0.0 } else { b.1.salience };
                    sa.partial_cmp(&sb).unwrap()
                })
                .map(|(i, _)| i)
                .unwrap();
            self.slots.remove(min_idx);
        }
    }

    /// Load prompt-derived slots. Higher salience items displace warm-start priors.
    pub fn load(&mut self, mut candidates: Vec<WmSlot>) {
        candidates.sort_by(|a, b|
            b.salience.partial_cmp(&a.salience).unwrap_or(std::cmp::Ordering::Equal)
        );
        for slot in candidates {
            if self.slots.len() < self.capacity {
                self.slots.push(slot);
            } else {
                let min_idx = self.slots.iter().enumerate()
                    .min_by(|a,b| {
                        let sa = if a.1.salience.is_nan() { 0.0 } else { a.1.salience };
                        let sb = if b.1.salience.is_nan() { 0.0 } else { b.1.salience };
                        sa.partial_cmp(&sb).unwrap()
                    })
                    .map(|(i,_)| i)
                    .unwrap();
                if self.slots[min_idx].salience < slot.salience {
                    self.slots[min_idx] = slot;
                }
            }
        }
    }
}
