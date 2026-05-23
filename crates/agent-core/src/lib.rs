//! Agent Core: domain logic for the Stats Web Platform agent.
//!
//! This crate contains all pure domain logic (Session state machine, validation,
//! `ChoicePrompt` parsing, Skill parameter validation, quota calculation, error code
//! mapping) independent of any HTTP framework.

pub mod encoding;
pub mod llm;
pub mod models;
pub mod orchestrator;
pub mod sanitize;
pub mod session_lifecycle;
pub mod skill;
pub mod stt;
pub mod store;
pub mod traits;
pub mod util;
pub mod validation;
