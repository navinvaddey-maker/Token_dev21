#[derive(Debug, Clone)]
pub enum TopologyClass {
    Instructional,
    Creative,
    DataHeavy,
    Conversational,
}

#[derive(Debug, Clone)]
pub struct ScoringThresholds {
    pub tes_floor: f32,
    pub sfs_floor: f32,
}

impl ScoringThresholds {
    /// GAP-18: Topology-keyed scoring thresholds to avoid uniform correction triggers
    pub fn for_topology(topology: &TopologyClass) -> Self {
        match topology {
            TopologyClass::Instructional => Self {
                tes_floor: 7.0,
                sfs_floor: 7.5,
            },
            TopologyClass::Creative => Self {
                tes_floor: 6.0,
                sfs_floor: 5.0,
            },
            TopologyClass::DataHeavy => Self {
                tes_floor: 7.5,
                sfs_floor: 8.0,
            },
            TopologyClass::Conversational => Self {
                tes_floor: 5.5,
                sfs_floor: 6.0,
            },
        }
    }
}
