//! Core trait definitions (async interfaces for stores and providers).

pub mod dataset_store;
pub mod llm_provider;
pub mod session_store;
pub mod stt_provider;

// Re-exports for convenience.
pub use dataset_store::*;
pub use llm_provider::*;
pub use session_store::*;
pub use stt_provider::*;
