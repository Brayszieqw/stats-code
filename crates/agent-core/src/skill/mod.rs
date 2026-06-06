//! Skill registry and execution infrastructure.

pub mod algorithm_map;
pub mod registry;
pub mod risk;
pub mod runner;

pub use algorithm_map::skill_to_algorithm;
pub use registry::{SkillDescriptor, SkillHandlerFn, SkillInvoker, SkillRegistry};
pub use risk::detect_risk_signals;
pub use runner::SkillRunner;
