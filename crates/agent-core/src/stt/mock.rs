//! Mock STT provider for testing.
//!
//! `MockStt` implements `SttProvider` and returns pre-configured transcription
//! results from a queue, supporting both successful results and controlled failures.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;

use crate::traits::stt_provider::{SttError, SttProvider, SttResult};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A pre-configured response for the mock STT.
#[derive(Debug, Clone)]
pub enum MockSttResponse {
    /// Successful transcription with text and confidence.
    Ok { text: String, confidence: f32 },
    /// Return an error.
    Error(SttError),
}

/// Mock STT provider that pops responses from a queue.
///
/// Thread-safe: the internal queue is wrapped in `Arc<Mutex<_>>`.
#[derive(Clone)]
pub struct MockStt {
    responses: Arc<Mutex<VecDeque<MockSttResponse>>>,
}

impl MockStt {
    /// Create a new `MockStt` with a sequence of responses.
    ///
    /// Each call to `transcribe` pops the next response from the front.
    /// If the queue is empty, `SttError::Unavailable` is returned.
    #[must_use] 
    pub fn new(responses: Vec<MockSttResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }

    /// Convenience constructor: single successful response.
    #[must_use] 
    pub fn with_text(text: &str, confidence: f32) -> Self {
        Self::new(vec![MockSttResponse::Ok {
            text: text.to_string(),
            confidence,
        }])
    }

    /// Returns how many responses remain in the queue.
    #[must_use] 
    pub fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

#[async_trait]
impl SttProvider for MockStt {
    async fn transcribe(&self, _audio: Bytes) -> Result<SttResult, SttError> {
        let response = {
            let mut queue = self.responses.lock().unwrap();
            queue.pop_front()
        };

        match response {
            Some(MockSttResponse::Ok { text, confidence }) => {
                Ok(SttResult { text, confidence })
            }
            Some(MockSttResponse::Error(err)) => Err(err),
            None => Err(SttError::Unavailable {
                reason: "MockStt: response queue is empty".to_string(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_audio() -> Bytes {
        Bytes::from_static(b"fake audio data")
    }

    #[tokio::test]
    async fn returns_ok_with_text_and_confidence() {
        let mock = MockStt::with_text("你好世界", 0.95);

        let result = mock.transcribe(dummy_audio()).await.unwrap();
        assert_eq!(result.text, "你好世界");
        assert!((result.confidence - 0.95).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn returns_error_when_configured() {
        let mock = MockStt::new(vec![MockSttResponse::Error(SttError::UnsupportedFormat {
            reason: "bad format".to_string(),
        })]);

        let result = mock.transcribe(dummy_audio()).await;
        assert!(matches!(result, Err(SttError::UnsupportedFormat { .. })));
    }

    #[tokio::test]
    async fn returns_unavailable_when_queue_empty() {
        let mock = MockStt::new(vec![]);

        let result = mock.transcribe(dummy_audio()).await;
        assert!(matches!(result, Err(SttError::Unavailable { .. })));
    }

    #[tokio::test]
    async fn pops_responses_sequentially() {
        let mock = MockStt::new(vec![
            MockSttResponse::Ok {
                text: "first".to_string(),
                confidence: 0.9,
            },
            MockSttResponse::Error(SttError::TranscriptionFailed {
                reason: "timeout".to_string(),
            }),
            MockSttResponse::Ok {
                text: "third".to_string(),
                confidence: 0.8,
            },
        ]);

        // First call: success
        let r1 = mock.transcribe(dummy_audio()).await.unwrap();
        assert_eq!(r1.text, "first");
        assert!((r1.confidence - 0.9).abs() < f32::EPSILON);

        // Second call: error
        let r2 = mock.transcribe(dummy_audio()).await;
        assert!(matches!(r2, Err(SttError::TranscriptionFailed { .. })));

        // Third call: success
        let r3 = mock.transcribe(dummy_audio()).await.unwrap();
        assert_eq!(r3.text, "third");
        assert!((r3.confidence - 0.8).abs() < f32::EPSILON);

        // Fourth call: empty queue
        let r4 = mock.transcribe(dummy_audio()).await;
        assert!(matches!(r4, Err(SttError::Unavailable { .. })));
    }

    #[tokio::test]
    async fn remaining_tracks_queue_size() {
        let mock = MockStt::new(vec![
            MockSttResponse::Ok {
                text: "a".to_string(),
                confidence: 0.5,
            },
            MockSttResponse::Ok {
                text: "b".to_string(),
                confidence: 0.7,
            },
        ]);
        assert_eq!(mock.remaining(), 2);

        let _ = mock.transcribe(dummy_audio()).await;
        assert_eq!(mock.remaining(), 1);

        let _ = mock.transcribe(dummy_audio()).await;
        assert_eq!(mock.remaining(), 0);
    }

    #[tokio::test]
    async fn low_confidence_result() {
        let mock = MockStt::with_text("模糊的文字", 0.4);

        let result = mock.transcribe(dummy_audio()).await.unwrap();
        assert_eq!(result.text, "模糊的文字");
        assert!(result.confidence < 0.6); // Below threshold for confirmation
    }
}
