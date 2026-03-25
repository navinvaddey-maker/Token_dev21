use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct FeedbackSignal {
    pub id:           String,
    pub user_id:      String,
    pub history_id:   String,
    pub signal_type:  String,
    pub signal_layer: i64,
    pub value:        f64,
    pub meta:         String,   // JSON string
    pub detected_at:  String,
}

#[derive(Debug, Deserialize)]
pub struct ExplicitFeedback {
    pub history_id: String,
    pub rating:     String,   // "thumbs_up" | "thumbs_down"
}
