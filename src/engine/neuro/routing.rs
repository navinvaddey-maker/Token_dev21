use super::context::PipelineContext;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TopologyType {
    Sequential,
    Hierarchical,
    Graph,
    Tabular,
}

#[derive(Debug, Clone)]
pub struct StageProfile {
    pub sparse_mode: String,
    pub boundary_prior: String,
}

#[derive(Debug, Clone)]
pub struct TopologyResult {
    pub topology_type: TopologyType,
}

/// GAP-05: Topology Router uses classification to route downstream processing
pub struct TopologyRouter;

impl TopologyRouter {
    pub fn configure(&self, topology: &TopologyResult) -> StageProfile {
        match topology.topology_type {
            TopologyType::Sequential => StageProfile {
                sparse_mode: "temporal".into(),
                boundary_prior: "linear".into(),
            },
            TopologyType::Hierarchical => StageProfile {
                sparse_mode: "tree".into(),
                boundary_prior: "recursive".into(),
            },
            TopologyType::Graph => StageProfile {
                sparse_mode: "spectral".into(),
                boundary_prior: "community".into(),
            },
            TopologyType::Tabular => StageProfile {
                sparse_mode: "columnar".into(),
                boundary_prior: "row_delimited".into(),
            },
        }
    }
}

/// GAP-07: Processing mode taxonomy and decision stage
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ProcessingMode {
    Generative,
    Extractive,
    Classifying,
    Translating,
    Summarizing,
}

pub struct ModeDecisionStage;

impl ModeDecisionStage {
    pub fn decide(&self, _boundary_map: &serde_json::Value, _ctx: &PipelineContext) -> ProcessingMode {
        // Feature extraction and model prediction go here
        
        // Mock prediction based on working memory
        let _span = (); // ctx.working_memory.snapshot();
        ProcessingMode::Generative
    }
}

/// GAP-08: Branching gate between direct fill and validation modes
pub struct SchemaResult {
    pub confidence: f32,
}

pub enum StageVariant {
    Stage4,
    Stage4B,
}

pub struct SchemaGate {
    pub confidence_threshold: f32,
}

impl SchemaGate {
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.75,
        }
    }

    pub fn route(&self, schema_result: &SchemaResult, _ctx: &PipelineContext) -> StageVariant {
        if schema_result.confidence >= self.confidence_threshold {
            StageVariant::Stage4 // Direct fill
        } else {
            StageVariant::Stage4B // Full validation mode
        }
    }
}
