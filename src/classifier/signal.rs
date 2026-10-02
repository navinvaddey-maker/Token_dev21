use serde::{Deserialize, Serialize};

/// The output of ScenarioClassifier.
///
/// Passed as `Option<ScenarioSignal>` through the pipeline.
/// Absence (`None`) is a valid state — all stages fall back to existing behavior.
///
/// Fields are kept as owned `String` to avoid lifetime
/// complications when crossing async stage boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioSignal {
    /// Semantic domain of the query.
    /// Values: "real-estate" | "finance" | "legal" | "tech" | "marketing" | "general"
    pub domain: String,

    /// Structural task category.
    /// Values: "creation" | "analysis" | "troubleshooting"
    pub task_type: String,

    /// User's apparent knowledge level, detected from lexical cues in the raw query.
    /// Values: "beginner" | "intermediate" | "expert"
    pub expertise: String,

    /// Specific intent within the domain.
    /// Values: "wealth-building" | "compliance" | "debugging" | "brand-building" | "general"
    pub intent_class: String,

    /// Normalized confidence score [0.0, 1.0].
    /// Computed as weighted combination of top cosine match score and score gap.
    /// Below 0.60 → None returned. 0.60–0.84 → nudge. ≥0.85 → override.
    pub confidence: f32,

    /// Industry vertical, independent of primary role (e.g. "pharma").
    #[serde(default)]
    pub secondary_domain: Option<String>,

    /// Generic composed role from the rule layer, if it fired.
    #[serde(default)]
    pub primary_role: Option<String>,

    /// When true, hash/Ory classifiers must not override this signal.
    #[serde(default)]
    pub rule_locked: bool,

    /// Legal/regulatory is a constraint lens, not the owning role.
    #[serde(default)]
    pub legal_as_constraint: bool,
}

impl ScenarioSignal {
    /// Returns true when confidence is sufficient to skip PredictiveCoding entirely.
    /// Threshold: 0.85 — chosen to require strong top-match AND meaningful gap from second.
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.85
    }

    /// Returns true when confidence is sufficient to nudge (but not override) the pipeline.
    /// Threshold: 0.60 — minimum to be useful without introducing noise.
    pub fn is_usable(&self) -> bool {
        self.confidence >= 0.60
    }
}

/// A raw row fetched from the `scenario_index` SQLite table.
/// Used internally by ScenarioClassifier during top-k lookup.
#[derive(Debug)]
pub struct ScenarioRow {
    /// Composite key: "{domain}::{intent_class}::{expertise}"
    pub label: String,
    pub domain: String,
    pub task_type: String,
    pub expertise: String,
    pub intent_class: String,
    /// L2-normalized f32 vector stored as raw bytes (via bytemuck).
    /// Empty blob = row not yet seeded; skipped during classification.
    pub vector_blob: Vec<u8>,
}
