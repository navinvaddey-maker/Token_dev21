//! Topology-driven downstream stage routing (`ARCHITECTURE.md` §1.1, §1.2).
//!
//! Maps a prompt's classified structural topology (`Sequential`, `Hierarchical`, `Graph`,
//! or `Tabular`) to its sparse-coding mode and boundary prior configuration.

/// Structural topology classification for a prompt.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TopologyType {
    /// Linear or chronological instruction flow.
    Sequential,
    /// Nested sections, trees, or multi-level headings.
    Hierarchical,
    /// Interconnected entity or dependency relationships.
    Graph,
    /// Row/column structured data or tables.
    Tabular,
}

/// Sparse-coding and boundary-detection parameters selected for a [`TopologyType`].
#[derive(Debug, Clone)]
pub struct StageProfile {
    /// Sparse coding strategy identifier (`"temporal"`, `"tree"`, `"spectral"`, `"columnar"`).
    pub sparse_mode: String,
    /// Boundary detection prior (`"linear"`, `"recursive"`, `"community"`, `"row_delimited"`).
    pub boundary_prior: String,
}

/// Result wrapper carrying the detected [`TopologyType`].
#[derive(Debug, Clone)]
pub struct TopologyResult {
    /// Classified topology variant.
    pub topology_type: TopologyType,
}

/// Maps classified prompt topology to downstream [`StageProfile`] configuration (`GAP-05`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TopologyRouter;

impl TopologyRouter {
    /// Returns the [`StageProfile`] matching `topology.topology_type`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
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
