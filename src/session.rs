use crate::{algorithms::predictive_coding::SessionTurn, types::AlgorithmOutput};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistory {
    pub turns: Vec<SessionTurn>,
    max_turns: usize,
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self::new(10)
    }
}

impl SessionHistory {
    pub fn new(max_turns: usize) -> Self {
        Self {
            turns: Vec::with_capacity(max_turns),
            max_turns,
        }
    }

    pub fn push(&mut self, _prompt: &str, output: &AlgorithmOutput) {
        let turn = SessionTurn {
            tokens: output
                .sparse_tokens
                .iter()
                .map(|t| t.text.clone())
                .collect(),
            mode: format!("{:?}", output.mode),
            error_score: output.error_score,
        };
        if self.turns.len() >= self.max_turns {
            self.turns.remove(0); // FIFO eviction, cap at max_turns
        }
        self.turns.push(turn);
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}
