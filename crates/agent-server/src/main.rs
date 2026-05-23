//! Agent Server binary entry point.
//!
//! Startup flow:
//! 1. Initialize tracing (JSON structured logs)
//! 2. Load configuration from `--config <path>` (default: `config/agent.yaml`)
//!    or `AGENT_CONFIG` env var. Falls back to mem-only no-LLM mode for
//!    development if `AGENT_DEV_NO_LLM=1` and no config file is present.
//! 3. Validate LLM configuration eagerly (R13.6) — abort with non-zero exit
//!    code on missing/invalid api_key, base_url, or model.
//! 4. Wire up SessionStore, DatasetStore, SkillRunner, LlmProvider, and
//!    AgentOrchestrator into AppState.
//! 5. Build router and serve.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tokio::net::TcpListener;

use agent_core::llm::build_llm_provider;
use agent_core::orchestrator::AgentOrchestrator;
use agent_core::skill::{SkillRegistry, SkillRunner};
use agent_core::store::{FsDatasetStore, MemSessionStore, SledSessionStore};
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::session_store::SessionStore;

use agent_server::config::Config;
use agent_server::middleware::load_shedding::LoadCounter;
use agent_server::orchestrator_adapter::OrchestratorAdapter;
use agent_server::state::{AppState, MessageHandler};

#[tokio::main]
async fn main() -> ExitCode {
    // 1. Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_server=debug,agent_core=debug,tower_http=info".into()),
        )
        .json()
        .init();

    // 2. Resolve config path
    let config_path = resolve_config_path();
    let dev_no_llm = std::env::var("AGENT_DEV_NO_LLM")
        .map(|v| v == "1")
        .unwrap_or(false);

    let config = match (config_path.as_deref(), dev_no_llm) {
        (Some(path), _) => {
            tracing::info!(path = %path.display(), "loading config");
            match Config::from_file(path) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::error!(error = %e, "failed to load config");
                    return ExitCode::from(1);
                }
            }
        }
        (None, true) => {
            tracing::warn!(
                "AGENT_DEV_NO_LLM=1 set and no config file — starting in dev mode without LLM"
            );
            None
        }
        (None, false) => {
            tracing::error!(
                "no config file found at default locations (./config/agent.yaml) and AGENT_DEV_NO_LLM is not set; \
                 set --config or AGENT_CONFIG env var, or AGENT_DEV_NO_LLM=1 to start without LLM"
            );
            return ExitCode::from(1);
        }
    };

    // 3. Bind addr / concurrency from config or defaults
    let bind_addr = config
        .as_ref()
        .map(|c| c.server.bind.clone())
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());
    let concurrency_threshold = config
        .as_ref()
        .map(|c| c.server.max_concurrent_sessions)
        .unwrap_or(50);

    // 4. Build SessionStore
    let session_store: Arc<dyn SessionStore> = match &config {
        Some(c) => match build_session_store(&c.storage.session_store).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to build session store");
                return ExitCode::from(1);
            }
        },
        None => Arc::new(MemSessionStore::new()),
    };

    // 5. Build DatasetStore
    let dataset_store: Option<Arc<dyn DatasetStore>> = match &config {
        Some(c) => match FsDatasetStore::new(c.storage.dataset_root.clone()).await {
            Ok(ds) => Some(Arc::new(ds)),
            Err(e) => {
                tracing::error!(error = %e, "failed to build dataset store");
                return ExitCode::from(1);
            }
        },
        None => None,
    };

    // 6. Build LLM provider (R13.6: validate eagerly, abort on failure)
    let llm_provider = match &config {
        Some(c) => match c.build_llm_config() {
            Ok(llm_cfg) => match build_llm_provider(&llm_cfg) {
                Ok(p) => {
                    tracing::info!(
                        provider = p.provider_id(),
                        model = llm_cfg.model(),
                        "LLM provider initialized"
                    );
                    Some(p)
                }
                Err(e) => {
                    tracing::error!(error = %e, "LLM configuration validation failed (R13.6)");
                    return ExitCode::from(1);
                }
            },
            Err(e) => {
                tracing::error!(error = %e, "failed to construct LLM config");
                return ExitCode::from(1);
            }
        },
        None => None,
    };

    // 7. Wire up the orchestrator if both dataset_store and llm_provider are present.
    let message_handler: Option<Arc<dyn MessageHandler>> = match (&config, &dataset_store, &llm_provider) {
        (Some(c), Some(ds), Some(llm)) => {
            let runner = SkillRunner::new(
                c.skill_runner.stats_code_bin.clone(),
                std::env::temp_dir(),
                c.skill_runner.max_wall_secs,
                c.skill_runner.max_rss_mib,
            );
            let registry = SkillRegistry::with_defaults();

            let orch = AgentOrchestrator::new(
                session_store.clone(),
                ds.clone(),
                registry,
                runner,
                llm.clone(),
            );
            Some(Arc::new(OrchestratorAdapter::new(orch)) as Arc<dyn MessageHandler>)
        }
        _ => None,
    };

    if message_handler.is_none() {
        tracing::warn!(
            "no orchestrator wired up — POST /api/sessions/:sid/messages will return errors"
        );
    }

    // 8. Build AppState
    let mut app_state = AppState::new(session_store);
    app_state.dataset_store = dataset_store;
    app_state.message_handler = message_handler;
    // STT provider: not yet configured via YAML; future task.

    let load_counter = LoadCounter::new(concurrency_threshold);
    let app = agent_server::build_router(load_counter, app_state);

    // 9. Bind and serve
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, addr = %bind_addr, "failed to bind");
            return ExitCode::from(1);
        }
    };
    tracing::info!(addr = %listener.local_addr().map(|a| a.to_string()).unwrap_or_default(), "agent-server listening");

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!(error = %e, "server error");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Find the config file: CLI arg `--config <path>`, env `AGENT_CONFIG`, or
/// the default `./config/agent.yaml`. Returns `None` if none exist.
fn resolve_config_path() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            if let Some(p) = iter.next() {
                return Some(PathBuf::from(p));
            }
        }
    }
    if let Ok(p) = std::env::var("AGENT_CONFIG") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    let default = PathBuf::from("./config/agent.yaml");
    if default.exists() {
        return Some(default);
    }
    None
}

/// Build a SessionStore from the config string.
/// - "mem"            → in-memory store (development)
/// - "sled:<path>"    → sled-backed persistent store
async fn build_session_store(spec: &str) -> Result<Arc<dyn SessionStore>, String> {
    if spec == "mem" {
        return Ok(Arc::new(MemSessionStore::new()));
    }
    if let Some(path) = spec.strip_prefix("sled:") {
        let store = SledSessionStore::open(path).map_err(|e| e.to_string())?;
        return Ok(Arc::new(store));
    }
    Err(format!(
        "unsupported session_store '{spec}' (expected 'mem' or 'sled:<path>')"
    ))
}
