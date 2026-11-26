//! Canonical multi-stage compression pipeline modules (`ARCHITECTURE.md` §1.1, §1.2).
//!
//! Exposes [`orchestrator::PipelineOrchestrator`] and stages 0A through 6B.

pub mod orchestrator;
pub mod stage0_normalize;
pub mod stage0b_topology;
pub mod stage1;
pub mod stage2;
pub mod stage3;
pub mod stage4;
pub mod stage5;
pub mod stage6a;
pub mod stage6b;
