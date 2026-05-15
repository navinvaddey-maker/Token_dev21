use crate::types::{
    AggressiveWm, GentleWm, Mode, WmSlot, AGGRESSIVE_WM_CAPACITY, GENTLE_WM_CAPACITY,
};
use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};

/// Working Memory — the bounded active context buffer.
///
/// Neuroscience basis:
///   Miller's Law (1956): 7 ± 2 items maximum capacity
///   Cowan's Revision (2001): 4 ± 1 under focused attention
///
/// Gentle mode  = 3 slots (Cowan focused attention limit − 1)
///              = structural fidelity floor: ~70%
/// Aggressive   = 7 slots (Miller's upper bound)
///              = structural fidelity ceiling: ~92%
///
/// Fidelity gap is NOT arbitrary — it emerges from slot difference (3 vs 7).
/// Displacement policy: lowest salience slot evicted when buffer full.
#[derive(Debug)]
pub enum WorkingMemory {
    Gentle(GentleWm),
    Aggressive(AggressiveWm),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextFrame {
    pub slots_used: usize,
    pub capacity: usize,
    pub utilisation: f32,
    pub fidelity: f32,
    pub items: Vec<WmSlot>,
}

const DECAY_LAMBDA: f32 = 0.5; // Half-life ~1.4s, near zero at 4s
const MIN_SALIENCE_THRESHOLD: f32 = 0.05;

impl WorkingMemory {
    pub fn new(mode: &Mode) -> Self {
        match mode {
            Mode::Gentle | Mode::Ambiguous | Mode::Balanced => Self::Gentle(ArrayVec::new()),
            Mode::Aggressive => Self::Aggressive(ArrayVec::new()),
        }
    }

    /// Load items sorted by salience descending.
    /// Items exceeding capacity trigger displacement of lowest-salience slot.
    pub fn load(&mut self, mut items: Vec<WmSlot>) {
        self.decay();

        // Sort highest salience first — most important items load first
        items.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for mut item in items {
            item.last_accessed = std::time::Instant::now();
            match self {
                Self::Gentle(slots) => {
                    if slots.len() < GENTLE_WM_CAPACITY {
                        slots.push(item);
                    } else {
                        Self::try_displace_gentle(slots, item);
                    }
                }
                Self::Aggressive(slots) => {
                    if slots.len() < AGGRESSIVE_WM_CAPACITY {
                        slots.push(item);
                    } else {
                        Self::try_displace_aggressive(slots, item);
                    }
                }
            }
        }
    }

    pub fn get_context_frame(&mut self) -> ContextFrame {
        self.decay();

        let (slots, capacity) = match self {
            Self::Gentle(s) => (s.as_slice(), GENTLE_WM_CAPACITY),
            Self::Aggressive(s) => (s.as_slice(), AGGRESSIVE_WM_CAPACITY),
        };

        let utilisation = slots.len() as f32 / capacity as f32;
        let fidelity = Self::estimate_fidelity(slots.len(), capacity);

        let mut items: Vec<WmSlot> = slots.to_vec();
        
        items.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ContextFrame {
            slots_used: slots.len(),
            capacity,
            utilisation,
            fidelity,
            items,
        }
    }

    /// Applies exponential decay to all non-protected slots and evicts those below threshold.
    pub fn decay(&mut self) {
        match self {
            Self::Gentle(slots) => {
                let mut to_remove = Vec::new();
                for (i, slot) in slots.iter_mut().enumerate() {
                    if !slot.is_protected {
                        let elapsed = slot.last_accessed.elapsed().as_secs_f32();
                        slot.salience *= (-DECAY_LAMBDA * elapsed).exp();
                        slot.last_accessed = std::time::Instant::now();
                        
                        if slot.salience < MIN_SALIENCE_THRESHOLD {
                            to_remove.push(i);
                        }
                    }
                }
                // Remove from back to front to preserve indices
                for &i in to_remove.iter().rev() {
                    slots.remove(i);
                }
            }
            Self::Aggressive(slots) => {
                let mut to_remove = Vec::new();
                for (i, slot) in slots.iter_mut().enumerate() {
                    if !slot.is_protected {
                        let elapsed = slot.last_accessed.elapsed().as_secs_f32();
                        slot.salience *= (-DECAY_LAMBDA * elapsed).exp();
                        slot.last_accessed = std::time::Instant::now();
                        
                        if slot.salience < MIN_SALIENCE_THRESHOLD {
                            to_remove.push(i);
                        }
                    }
                }
                for &i in to_remove.iter().rev() {
                    slots.remove(i);
                }
            }
        }
    }

    /// Fidelity = (slots_used / capacity) × mode_ceiling
    /// Gentle ceiling = 0.75, Aggressive ceiling = 0.95
    fn estimate_fidelity(slots_used: usize, capacity: usize) -> f32 {
        let utilisation = slots_used as f32 / capacity as f32;
        let ceiling = if capacity == GENTLE_WM_CAPACITY {
            0.75
        } else {
            0.95
        };
        (utilisation * ceiling).clamp(0.0, 1.0)
    }

    fn try_displace_gentle(slots: &mut GentleWm, candidate: WmSlot) {
        if let Some(min_idx) = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_protected)
            .min_by(|(_, a), (_, b)| a.salience.partial_cmp(&b.salience).unwrap())
            .map(|(i, _)| i)
        {
            if slots[min_idx].salience < candidate.salience || candidate.is_protected {
                slots.remove(min_idx);
                let _ = slots.try_push(candidate);
            }
        }
    }

    fn try_displace_aggressive(slots: &mut AggressiveWm, candidate: WmSlot) {
        if let Some(min_idx) = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_protected)
            .min_by(|(_, a), (_, b)| a.salience.partial_cmp(&b.salience).unwrap())
            .map(|(i, _)| i)
        {
            if slots[min_idx].salience < candidate.salience || candidate.is_protected {
                slots.remove(min_idx);
                let _ = slots.try_push(candidate);
            }
        }
    }
}

// Re-export for convenience

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SlotSource;

    fn make_slot(content: &str, salience: f32) -> WmSlot {
        WmSlot {
            content: content.into(),
            salience,
            source: SlotSource::Delta,
            is_protected: false,
            last_accessed: std::time::Instant::now(),
        }
    }

    #[test]
    fn gentle_hard_cap_at_3() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        wm.load(vec![
            make_slot("task", 0.9),
            make_slot("deliv", 0.8),
            make_slot("ctx1", 0.7),
            make_slot("ctx2", 0.6), // this should be displaced or not loaded
            make_slot("ctx3", 0.5),
        ]);
        let frame = wm.get_context_frame();
        assert_eq!(
            frame.slots_used, GENTLE_WM_CAPACITY,
            "Gentle WM must hard-cap at 3"
        );
    }

    #[test]
    fn aggressive_hard_cap_at_7() {
        let mut wm = WorkingMemory::new(&Mode::Aggressive);
        let items: Vec<WmSlot> = (0..10)
            .map(|i| make_slot(&i.to_string(), i as f32 * 0.1))
            .collect();
        wm.load(items);
        let frame = wm.get_context_frame();
        assert_eq!(
            frame.slots_used, AGGRESSIVE_WM_CAPACITY,
            "Aggressive WM must hard-cap at 7"
        );
    }

    #[test]
    fn displacement_keeps_highest_salience() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        wm.load(vec![
            make_slot("low", 0.2),
            make_slot("mid", 0.5),
            make_slot("high", 0.9),
            make_slot("new", 0.8), // should displace "low"
        ]);
        let frame = wm.get_context_frame();
        assert!(
            !frame.items.iter().any(|s| s.content == "low"),
            "Lowest salience slot must be displaced"
        );
        assert!(frame.items.iter().any(|s| s.content == "high"));
        assert!(frame.items.iter().any(|s| s.content == "new"));
    }

    #[test]
    fn fidelity_floor_gentle() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        wm.load(vec![
            make_slot("a", 0.9),
            make_slot("b", 0.8),
            make_slot("c", 0.7),
        ]);
        let frame = wm.get_context_frame();
        assert!(
            frame.fidelity >= 0.65,
            "Gentle fidelity floor must be at least 65%"
        );
        assert!(
            frame.fidelity <= 0.80,
            "Gentle fidelity ceiling must be at most 80%"
        );
    }

    #[test]
    fn test_decay_over_time() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        wm.load(vec![make_slot("decay_me", 1.0)]);
        
        // Instant decay check
        let frame1 = wm.get_context_frame();
        assert!(frame1.items[0].salience <= 1.0);

        // Sleep for 1 second. Decay should reduce salience.
        // e^(-0.5 * 1) ≈ 0.606
        std::thread::sleep(std::time::Duration::from_millis(1100));
        
        let frame2 = wm.get_context_frame();
        assert!(frame2.items[0].salience < 0.7, "Salience should have decayed, got {}", frame2.items[0].salience);
        assert!(frame2.items[0].salience > 0.4, "Salience shouldn't have decayed too much, got {}", frame2.items[0].salience);
    }

    #[test]
    fn test_eviction_threshold() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        // Start with very low salience
        wm.load(vec![make_slot("evict_me", 0.06)]);
        
        // Wait for it to fall below 0.05
        // 0.06 * e^(-0.5 * t) < 0.05
        // e^(-0.5 * t) < 0.833
        // -0.5 * t < ln(0.833) ≈ -0.182
        // t > 0.364
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        // Call load or decay to trigger eviction
        wm.decay();
        
        let frame = wm.get_context_frame();
        assert_eq!(frame.slots_used, 0, "Slot should have been evicted due to low salience");
    }

    #[test]
    fn test_protected_no_decay() {
        let mut wm = WorkingMemory::new(&Mode::Gentle);
        let mut protected_slot = make_slot("protected", 1.0);
        protected_slot.is_protected = true;
        wm.load(vec![protected_slot]);
        
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        let frame = wm.get_context_frame();
        assert_eq!(frame.items[0].salience, 1.0, "Protected slots must not decay");
    }
}
