//! Mock LLM provider for testing.
//!
//! `MockLlm` implements `LlmProvider` and returns pre-configured responses
//! from a queue, supporting both successful streams and controlled failures.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_stream::iter as stream_iter;

use crate::traits::llm_provider::{LlmError, LlmEvent, LlmProvider, LlmRequest, LlmStream};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A pre-configured response for the mock LLM.
#[derive(Debug, Clone)]
pub enum MockLlmResponse {
    /// Return a stream of events.
    Stream(Vec<LlmEvent>),
    /// Return an error immediately (before streaming).
    Error(LlmError),
}

/// Mock LLM provider that pops responses from a queue.
///
/// Thread-safe: the internal queue is wrapped in `Arc<Mutex<_>>`.
#[derive(Clone)]
pub struct MockLlm {
    responses: Arc<Mutex<VecDeque<MockLlmResponse>>>,
}

impl MockLlm {
    /// Create a new `MockLlm` with a sequence of responses.
    ///
    /// Each call to `chat_stream` pops the next response from the front.
    /// If the queue is empty, `LlmError::Unavailable` is returned.
    #[must_use] 
    pub fn new(responses: Vec<MockLlmResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }

    /// Convenience constructor: each text becomes a `[TextDelta(text), Done]` stream.
    #[must_use] 
    pub fn with_texts(texts: Vec<&str>) -> Self {
        let responses = texts
            .into_iter()
            .map(|t| {
                MockLlmResponse::Stream(vec![
                    LlmEvent::TextDelta(t.to_string()),
                    LlmEvent::Done,
                ])
            })
            .collect();
        Self::new(responses)
    }

    /// Returns how many responses remain in the queue.
    #[must_use] 
    pub fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

#[async_trait]
impl LlmProvider for MockLlm {
    async fn chat_stream(&self, _req: LlmRequest) -> Result<LlmStream, LlmError> {
        let response = {
            let mut queue = self.responses.lock().unwrap();
            queue.pop_front()
        };

        match response {
            Some(MockLlmResponse::Stream(events)) => {
                Ok(Box::pin(stream_iter(events)))
            }
            Some(MockLlmResponse::Error(err)) => Err(err),
            None => Err(LlmError::Unavailable {
                reason: "MockLlm: response queue is empty".to_string(),
            }),
        }
    }

    fn provider_id(&self) -> &'static str {
        "mock"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    use crate::traits::llm_provider::{LlmMessage, LlmRole};

    fn dummy_request() -> LlmRequest {
        LlmRequest {
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "hello".to_string(),
            }],
            model: "test-model".to_string(),
            max_tokens: None,
            temperature: None,
        }
    }

    #[tokio::test]
    async fn returns_stream_events_in_order() {
        let events = vec![
            LlmEvent::TextDelta("Hello".to_string()),
            LlmEvent::TextDelta(" world".to_string()),
            LlmEvent::Done,
        ];
        let mock = MockLlm::new(vec![MockLlmResponse::Stream(events.clone())]);

        let mut stream = mock.chat_stream(dummy_request()).await.unwrap();

        let e1 = stream.next().await.unwrap();
        assert!(matches!(e1, LlmEvent::TextDelta(ref s) if s == "Hello"));

        let e2 = stream.next().await.unwrap();
        assert!(matches!(e2, LlmEvent::TextDelta(ref s) if s == " world"));

        let e3 = stream.next().await.unwrap();
        assert!(matches!(e3, LlmEvent::Done));

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_error_when_configured() {
        let mock = MockLlm::new(vec![MockLlmResponse::Error(LlmError::RateLimited)]);

        let result = mock.chat_stream(dummy_request()).await;
        assert!(matches!(result, Err(LlmError::RateLimited)));
    }

    #[tokio::test]
    async fn returns_unavailable_when_queue_empty() {
        let mock = MockLlm::new(vec![]);

        let result = mock.chat_stream(dummy_request()).await;
        assert!(matches!(result, Err(LlmError::Unavailable { .. })));
    }

    #[tokio::test]
    async fn pops_responses_sequentially() {
        let mock = MockLlm::new(vec![
            MockLlmResponse::Stream(vec![LlmEvent::TextDelta("first".to_string()), LlmEvent::Done]),
            MockLlmResponse::Error(LlmError::RateLimited),
            MockLlmResponse::Stream(vec![LlmEvent::TextDelta("third".to_string()), LlmEvent::Done]),
        ]);

        // First call: stream
        let mut s1 = mock.chat_stream(dummy_request()).await.unwrap();
        let e = s1.next().await.unwrap();
        assert!(matches!(e, LlmEvent::TextDelta(ref s) if s == "first"));

        // Second call: error
        let r2 = mock.chat_stream(dummy_request()).await;
        assert!(matches!(r2, Err(LlmError::RateLimited)));

        // Third call: stream
        let mut s3 = mock.chat_stream(dummy_request()).await.unwrap();
        let e = s3.next().await.unwrap();
        assert!(matches!(e, LlmEvent::TextDelta(ref s) if s == "third"));

        // Fourth call: empty
        let r4 = mock.chat_stream(dummy_request()).await;
        assert!(matches!(r4, Err(LlmError::Unavailable { .. })));
    }

    #[tokio::test]
    async fn with_texts_convenience() {
        let mock = MockLlm::with_texts(vec!["hello", "world"]);

        let mut s1 = mock.chat_stream(dummy_request()).await.unwrap();
        assert!(matches!(s1.next().await.unwrap(), LlmEvent::TextDelta(ref s) if s == "hello"));
        assert!(matches!(s1.next().await.unwrap(), LlmEvent::Done));

        let mut s2 = mock.chat_stream(dummy_request()).await.unwrap();
        assert!(matches!(s2.next().await.unwrap(), LlmEvent::TextDelta(ref s) if s == "world"));
        assert!(matches!(s2.next().await.unwrap(), LlmEvent::Done));
    }

    #[tokio::test]
    async fn provider_id_is_mock() {
        let mock = MockLlm::new(vec![]);
        assert_eq!(mock.provider_id(), "mock");
    }

    #[tokio::test]
    async fn remaining_tracks_queue_size() {
        let mock = MockLlm::new(vec![
            MockLlmResponse::Stream(vec![LlmEvent::Done]),
            MockLlmResponse::Stream(vec![LlmEvent::Done]),
        ]);
        assert_eq!(mock.remaining(), 2);

        let _ = mock.chat_stream(dummy_request()).await;
        assert_eq!(mock.remaining(), 1);

        let _ = mock.chat_stream(dummy_request()).await;
        assert_eq!(mock.remaining(), 0);
    }
}
