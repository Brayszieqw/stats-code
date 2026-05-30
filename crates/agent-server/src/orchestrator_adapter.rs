//! Adapter that bridges `agent_core::AgentOrchestrator` to `state::MessageHandler`.
//!
//! `AgentOrchestrator` is generic over `<S: SessionStore, D: DatasetStore>` for
//! testability with concrete in-memory stores. The HTTP handlers want a
//! type-erased `Arc<dyn MessageHandler>`, so we wrap the orchestrator here.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_stream::Stream;

use agent_core::models::SessionId;
use agent_core::orchestrator::{AgentEvent, AgentOrchestrator, UserMessageInput};
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::session_store::SessionStore;

use crate::state::MessageHandler;

/// Type-erased adapter wrapping a concrete `AgentOrchestrator`.
pub struct OrchestratorAdapter<S, D>
where
    S: SessionStore + 'static,
    D: DatasetStore + 'static,
{
    inner: Arc<AgentOrchestrator<S, D>>,
}

impl<S, D> OrchestratorAdapter<S, D>
where
    S: SessionStore + 'static,
    D: DatasetStore + 'static,
{
    /// Wrap an existing `AgentOrchestrator` for use as a `MessageHandler`.
    #[must_use]
    pub fn new(orchestrator: AgentOrchestrator<S, D>) -> Self {
        Self {
            inner: Arc::new(orchestrator),
        }
    }

    /// Wrap an `Arc<AgentOrchestrator>` (when the orchestrator is shared elsewhere).
    #[must_use]
    pub fn from_arc(orchestrator: Arc<AgentOrchestrator<S, D>>) -> Self {
        Self {
            inner: orchestrator,
        }
    }
}

impl<S, D> MessageHandler for OrchestratorAdapter<S, D>
where
    S: SessionStore + 'static,
    D: DatasetStore + 'static,
{
    fn handle_message(
        &self,
        sid: SessionId,
        msg: UserMessageInput,
    ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>
    {
        let orchestrator = self.inner.clone();
        Box::pin(async move {
            let stream = futures::StreamExt::flatten(futures::stream::once(async move {
                orchestrator.handle_user_message(sid, msg).await
            }));
            Box::pin(stream) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio_stream::StreamExt;

    use agent_core::llm::MockLlm;
    use agent_core::models::SessionSettings;
    use agent_core::skill::{SkillRegistry, SkillRunner};
    use agent_core::store::{FsDatasetStore, MemSessionStore};
    use agent_core::traits::llm_provider::{
        LlmError, LlmEvent, LlmProvider, LlmRequest, LlmStream,
    };
    use tempfile::TempDir;

    struct SlowLlm;

    #[async_trait]
    impl LlmProvider for SlowLlm {
        async fn chat_stream(&self, _req: LlmRequest) -> Result<LlmStream, LlmError> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let response = r#"{"skill_ids":[],"resolved_args":{},"has_query_intent":false,"text_response":"ok"}"#;
            Ok(Box::pin(tokio_stream::iter(vec![
                LlmEvent::TextDelta(response.to_string()),
                LlmEvent::Done,
            ])))
        }

        fn provider_id(&self) -> &'static str {
            "slow"
        }
    }

    async fn make_adapter(
        llm: Arc<dyn LlmProvider>,
    ) -> (
        OrchestratorAdapter<MemSessionStore, FsDatasetStore>,
        SessionId,
        TempDir,
    ) {
        let tmp = TempDir::new().unwrap();
        let session_store = MemSessionStore::new();
        let session = session_store.create().await.unwrap();
        let dataset_store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let registry = SkillRegistry::with_defaults();
        let runner = SkillRunner::new(
            PathBuf::from("stats-code"),
            tmp.path().to_path_buf(),
            60,
            1024,
        );

        let orch = AgentOrchestrator::new(session_store, dataset_store, registry, runner, llm);
        (OrchestratorAdapter::new(orch), session.id, tmp)
    }

    #[tokio::test]
    async fn adapter_dispatches_to_orchestrator() {
        let tmp = TempDir::new().unwrap();
        let session_store = MemSessionStore::new();
        let dataset_store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let registry = SkillRegistry::with_defaults();
        let runner = SkillRunner::new(
            PathBuf::from("stats-code"),
            tmp.path().to_path_buf(),
            60,
            1024,
        );
        let llm = Arc::new(MockLlm::with_texts(vec!["你好"]));
        let session = session_store.create().await.unwrap();

        let orch = AgentOrchestrator::new(session_store, dataset_store, registry, runner, llm);
        let adapter = OrchestratorAdapter::new(orch);

        let sid = session.id;
        let msg = UserMessageInput {
            text: "hi".to_string(),
            settings: SessionSettings::default(),
        };

        let stream = adapter.handle_message(sid, msg).await;
        let events: Vec<AgentEvent> = stream.collect().await;

        // At minimum we expect a Done event terminating the stream.
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Done)));
    }

    #[tokio::test]
    async fn adapter_returns_stream_before_slow_llm_completes() {
        let (adapter, sid, _tmp) = make_adapter(Arc::new(SlowLlm)).await;

        let msg = UserMessageInput {
            text: "hi".to_string(),
            settings: SessionSettings::default(),
        };

        let stream =
            tokio::time::timeout(Duration::from_millis(50), adapter.handle_message(sid, msg))
                .await
                .expect("adapter should return an SSE stream before slow LLM work finishes");

        let events: Vec<AgentEvent> =
            tokio::time::timeout(Duration::from_secs(1), stream.collect())
                .await
                .expect("lazy stream should still produce events");

        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta(text) if text == "ok")));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Done)));
    }
}
