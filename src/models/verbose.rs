use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct SqlxEvent {
    pub operation: String,
    pub duration_ms: u64,
    pub row_count: Option<i64>,
    pub sql: Option<String>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct SqlxVerbose {
    pub events: Vec<SqlxEvent>,
}

impl SqlxVerbose {
    pub fn push(&mut self, event: SqlxEvent) {
        self.events.push(event);
    }
}
