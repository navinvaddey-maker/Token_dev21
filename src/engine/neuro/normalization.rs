#[derive(Debug, PartialEq, Eq)]
pub enum InputModality {
    Text,
    Numerical,
    Graph,
    Embedding,
}

pub trait Normalizer: Send + Sync {
    fn transform(&self, raw: &str) -> String;
}

/// GAP-04: Typed Normalization Stage specifying exactly how different modalities are standardized.
pub struct NormalizationStage {
    // In a real application, we'd route dynamically based on modality enum.
    pub default_modality: InputModality,
}

impl NormalizationStage {
    pub fn new() -> Self {
        Self {
            default_modality: InputModality::Text,
        }
    }

    pub fn execute(&self, raw: &str, modality: &InputModality) -> String {
        match modality {
            InputModality::Text => TextNormalizer::new().transform(raw),
            InputModality::Numerical => ZScoreNormalizer::new().transform(raw),
            InputModality::Graph => AdjacencyNormalizer::new().transform(raw),
            InputModality::Embedding => L2Normalizer::new().transform(raw),
        }
    }
}

pub struct TextNormalizer;
impl TextNormalizer {
    pub fn new() -> Self { Self }
}
impl Normalizer for TextNormalizer {
    fn transform(&self, raw: &str) -> String {
        // e.g. lowercase, unicode NFC, etc.
        raw.to_lowercase()
    }
}

pub struct ZScoreNormalizer;
impl ZScoreNormalizer {
    pub fn new() -> Self { Self }
}
impl Normalizer for ZScoreNormalizer {
    fn transform(&self, raw: &str) -> String {
        // e.g. z-score calculation and stringification
        raw.to_string()
    }
}

pub struct AdjacencyNormalizer;
impl AdjacencyNormalizer {
    pub fn new() -> Self { Self }
}
impl Normalizer for AdjacencyNormalizer {
    fn transform(&self, raw: &str) -> String {
        raw.to_string()
    }
}

pub struct L2Normalizer;
impl L2Normalizer {
    pub fn new() -> Self { Self }
}
impl Normalizer for L2Normalizer {
    fn transform(&self, raw: &str) -> String {
        raw.to_string()
    }
}
