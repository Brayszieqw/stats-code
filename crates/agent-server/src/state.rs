//! Shared application state for axum handlers.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_core::models::SessionId;
use agent_core::orchestrator::{AgentEvent, UserMessageInput};
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::session_store::SessionStore;
use agent_core::traits::stt_provider::SttProvider;
use tokio_stream::Stream;

use crate::handlers::llm_config::{LlmConfigStore, LlmProbe};

/// Trait abstracting the orchestrator's message handling capability.
///
/// This allows `AppState` to hold a type-erased orchestrator, making it
/// easy to swap in mocks during testing.
///
/// Uses explicit `Pin<Box<dyn Future>>` return to ensure dyn-compatibility
/// (async fn in traits is not dyn-compatible).
pub trait MessageHandler: Send + Sync {
    /// Process a user message and return a stream of agent events.
    fn handle_message(
        &self,
        sid: SessionId,
        msg: UserMessageInput,
    ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>;
}

/// Shared application state passed to all handlers via axum's `State` extractor.
///
/// Uses `Arc` internally so it can be cheaply cloned across request tasks.
#[derive(Clone)]
pub struct AppState {
    /// The session store implementation (e.g., `MemSessionStore` or `SledSessionStore`).
    pub session_store: Arc<dyn SessionStore>,
    /// The orchestrator for handling user messages (optional for routes that don't need it).
    pub message_handler: Option<Arc<dyn MessageHandler>>,
    /// The STT provider for audio transcription (optional; not all deployments have STT).
    pub stt_provider: Option<Arc<dyn SttProvider>>,
    /// The dataset store for file persistence and parsing (optional; not all deployments need it).
    pub dataset_store: Option<Arc<dyn DatasetStore>>,
    /// The LLM config store for reading/writing LLM settings (optional; launcher injects TomlFileStore).
    pub llm_config_store: Option<Arc<dyn LlmConfigStore>>,
    /// The LLM probe for connectivity testing (optional; launcher injects real implementation).
    pub llm_probe: Option<Arc<dyn LlmProbe>>,
}

impl AppState {
    /// Create a new `AppState` with the given session store.
    pub fn new(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            message_handler: None,
            stt_provider: None,
            dataset_store: None,
            llm_config_store: None,
            llm_probe: None,
        }
    }

    /// Create a new `AppState` with both session store and message handler.
    pub fn with_message_handler(
        session_store: Arc<dyn SessionStore>,
        message_handler: Arc<dyn MessageHandler>,
    ) -> Self {
        Self {
            session_store,
            message_handler: Some(message_handler),
            stt_provider: None,
            dataset_store: None,
            llm_config_store: None,
            llm_probe: None,
        }
    }
}
