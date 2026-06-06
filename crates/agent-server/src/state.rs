//! Shared application state for axum handlers.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_core::models::SessionId;
use agent_core::orchestrator::{AgentEvent, UserMessageInput};
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::session_store::SessionStore;
use agent_core::traits::stt_provider::SttProvider;
use api::sidecar::{
    CoverageMatrixDto, ReferenceSoftware, SidecarRenderRequest, SidecarSnippetDto,
    SnapshotExportResponse,
};
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

// ---------------------------------------------------------------------------
// Sidecar / coverage-matrix / snapshot provider traits (tasks 10.2 / 10.3 / 10.4)
// ---------------------------------------------------------------------------
//
// These traits decouple the agent-server crate from the `stats-code` crate
// so that the HTTP handlers can ship before (and independently of) the
// concrete implementations. The launcher in `stats-code` injects concrete
// providers when it constructs `AppState`; tests inject mocks.
//
// `agent-server` cannot depend on `stats-code` (the dependency arrow runs
// the other way), and the handlers must surface 503 / 4xx behaviour
// before the launcher wires the providers — hence each provider is
// `Option<Arc<dyn ...>>`.

/// Provides the embedded Algorithm Coverage Matrix snapshot for
/// `GET /api/coverage-matrix` (Requirement 6.2).
pub trait CoverageMatrixProvider: Send + Sync {
    /// Return the matrix DTO. Implementations are expected to clone an
    /// immutable, process-global value, so this is cheap.
    fn get(&self) -> CoverageMatrixDto;
}

/// Failure modes surfaced by [`SidecarProvider::generate`]. Mapped to
/// HTTP status codes by `handlers::sidecar::post_sidecar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarProviderError {
    /// The `algorithm_id` path parameter does not appear in the
    /// Algorithm Coverage Matrix.
    UnknownAlgorithm(String),
    /// A required template file is missing for an `(algorithm, software)`
    /// pair whose coverage state is `live`, `recorded`, or `sidecar_only`.
    MissingTemplate {
        algorithm_id: String,
        software: ReferenceSoftware,
    },
    /// The request carried a column dtype outside the closed set
    /// `{numeric, categorical, date, string}`, or a malformed dataset
    /// SHA256 / template placeholder. Carries a human-readable reason.
    InvalidRequest(String),
    /// The redaction policy detected forbidden content in the rendered
    /// snippet (e.g. an unredacted API key).
    RedactionViolation(String),
    /// The runtime sentinel (`SpawnPolicy::forbid_external_runtimes`)
    /// detected a forbidden child-process spawn or shared-library load.
    ForbiddenSpawn(String),
    /// Internal generator failure not covered above.
    Internal(String),
}

/// Generates an Equivalent Code Sidecar snippet for one
/// `(algorithm_id, software)` cell (Requirement 1.3, 2.2).
///
/// The sidecar is a pure function of its inputs, so the provider takes the
/// column metadata, dataset SHA256, and parameters directly from the
/// request rather than resolving them from server-side run state. The SPA
/// already holds all of these, which lets the endpoint be fully functional
/// without a run-state store.
pub trait SidecarProvider: Send + Sync {
    /// Render the snippet for `(algorithm_id, request.software)` using the
    /// caller-supplied column metadata, dataset SHA256, and parameters.
    fn generate(
        &self,
        algorithm_id: &str,
        request: &SidecarRenderRequest,
    ) -> Result<SidecarSnippetDto, SidecarProviderError>;
}

/// Failure modes surfaced by [`SnapshotProvider::export`]. Mapped to
/// HTTP status codes by `handlers::snapshot::post_snapshot_export`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotProviderError {
    /// The `run_id` does not reference an existing analysis run.
    UnknownRun(String),
    /// The run's `status` is not `completed` (Requirement 7.8).
    RunNotCompleted { actual_status: String },
    /// The run has no completed workflow step that can be exported
    /// (Requirement 8.2).
    NoExportableStep { run_id: String },
    /// The dataset referenced by the run cannot be resolved/read
    /// (Requirement 8.3).
    DatasetUnresolved { reason: String },
    /// The total artifact payload exceeds the 50 MB ceiling
    /// (Requirement 7.7).
    PayloadTooLarge {
        measured_bytes: u64,
        ceiling_bytes: u64,
    },
    /// The runtime sentinel detected a forbidden spawn or library load.
    ForbiddenSpawn(String),
    /// Internal exporter failure not covered above.
    Internal(String),
}

/// Produces an Audit Snapshot `.zip` for the given run
/// (Requirement 7.1, 7.2).
pub trait SnapshotProvider: Send + Sync {
    /// Export the snapshot for `run_id` to `destination`.
    ///
    /// The exporter writes to `<destination>.tmp` and atomically renames
    /// on success; any error variant guarantees the destination is
    /// untouched.
    fn export(
        &self,
        run_id: &str,
        destination: &str,
    ) -> Result<SnapshotExportResponse, SnapshotProviderError>;
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
    /// The LLM config store for reading/writing LLM settings (optional; launcher injects `TomlFileStore`).
    pub llm_config_store: Option<Arc<dyn LlmConfigStore>>,
    /// The LLM probe for connectivity testing (optional; launcher injects real implementation).
    pub llm_probe: Option<Arc<dyn LlmProbe>>,
    /// Algorithm Coverage Matrix provider (`GET /api/coverage-matrix`).
    pub coverage_matrix_provider: Option<Arc<dyn CoverageMatrixProvider>>,
    /// Equivalent Code Sidecar provider (`GET /api/sidecar/{algorithm_id}`).
    pub sidecar_provider: Option<Arc<dyn SidecarProvider>>,
    /// Audit Snapshot provider (`POST /api/snapshot/export`).
    pub snapshot_provider: Option<Arc<dyn SnapshotProvider>>,
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
            coverage_matrix_provider: None,
            sidecar_provider: None,
            snapshot_provider: None,
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
            coverage_matrix_provider: None,
            sidecar_provider: None,
            snapshot_provider: None,
        }
    }
}
