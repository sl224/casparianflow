//! Mock LLM Provider for deterministic TUI testing
//!
//! Provides canned responses without network calls or subprocess spawning.
//! Use this for autonomous testing where we need deterministic results.

use async_trait::async_trait;
use futures::stream::BoxStream;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::{LlmConfig, LlmError, LlmProvider, Message, StreamChunk, ToolDefinition};

/// Configuration for a single canned response
#[derive(Debug, Clone)]
pub struct CannedResponse {
    /// Text content to return
    pub text: String,
    /// Optional delay before sending (simulates thinking time)
    pub delay_ms: u64,
}

impl CannedResponse {
    /// Create a simple text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            text: content.into(),
            delay_ms: 0,
        }
    }

    /// Create a response with simulated delay
    pub fn with_delay(content: impl Into<String>, delay_ms: u64) -> Self {
        Self {
            text: content.into(),
            delay_ms,
        }
    }
}

/// Mock LLM provider with deterministic responses
///
/// Responses are queued and consumed in order. If no responses are queued,
/// returns an error (to catch test configuration issues).
pub struct MockClaudeProvider {
    /// Queue of responses to return
    responses: Arc<Mutex<VecDeque<CannedResponse>>>,
    /// Record of messages received (for assertions)
    received_messages: Arc<Mutex<Vec<Vec<Message>>>>,
}

impl MockClaudeProvider {
    /// Create a new mock provider
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            received_messages: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Queue a response for the next chat_stream call
    pub fn queue_response(&self, response: CannedResponse) {
        self.responses.lock().unwrap().push_back(response);
    }

    /// Queue multiple responses
    pub fn queue_responses(&self, responses: Vec<CannedResponse>) {
        let mut queue = self.responses.lock().unwrap();
        for r in responses {
            queue.push_back(r);
        }
    }

    /// Get all messages received by this provider
    #[allow(dead_code)]
    pub fn received_messages(&self) -> Vec<Vec<Message>> {
        self.received_messages.lock().unwrap().clone()
    }

    /// Check how many responses are still queued
    #[allow(dead_code)]
    pub fn responses_remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

impl Default for MockClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockClaudeProvider {
    fn name(&self) -> &str {
        "Mock Claude"
    }

    fn model(&self) -> &str {
        "mock-test-model"
    }

    fn is_ready(&self) -> bool {
        true // Always ready for testing
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _tools: &[ToolDefinition],
        _config: Option<&LlmConfig>,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        // Record received messages
        self.received_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());

        // Get next queued response
        let response = self.responses.lock().unwrap().pop_front().ok_or_else(|| {
            LlmError::Internal(
                "MockClaudeProvider: No responses queued! Queue responses before calling chat_stream"
                    .to_string(),
            )
        })?;

        let (tx, rx) = mpsc::channel::<StreamChunk>(10);

        // Spawn task to send response chunks
        let delay_ms = response.delay_ms;
        let text = response.text;

        tokio::spawn(async move {
            // Optional delay to simulate thinking
            if delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            // Send text response
            if !text.is_empty() {
                let _ = tx.send(StreamChunk::Text(text)).await;
            }

            // Send done
            let _ = tx
                .send(StreamChunk::Done {
                    stop_reason: Some("end_turn".to_string()),
                })
                .await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_returns_queued_response() {
        let provider = MockClaudeProvider::new();
        provider.queue_response(CannedResponse::text("Hello from mock!"));

        let messages = vec![Message::user("hi")];
        let mut stream = provider.chat_stream(&messages, &[], None).await.unwrap();

        let mut response_text = String::new();
        while let Some(chunk) = stream.next().await {
            if let StreamChunk::Text(t) = chunk {
                response_text.push_str(&t);
            }
        }

        assert_eq!(response_text, "Hello from mock!");
    }

    #[tokio::test]
    async fn test_mock_provider_records_messages() {
        let provider = MockClaudeProvider::new();
        provider.queue_response(CannedResponse::text("response"));

        let messages = vec![
            Message::user("first message"),
            Message::assistant("previous response"),
            Message::user("second message"),
        ];

        let _ = provider.chat_stream(&messages, &[], None).await.unwrap();

        let received = provider.received_messages();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].len(), 3);
    }

    #[tokio::test]
    async fn test_mock_provider_error_when_no_response() {
        let provider = MockClaudeProvider::new();
        // Don't queue any response

        let result = provider.chat_stream(&[], &[], None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_multiple_responses() {
        let provider = MockClaudeProvider::new();
        provider.queue_responses(vec![
            CannedResponse::text("First"),
            CannedResponse::text("Second"),
        ]);

        // First call
        let messages = vec![Message::user("1")];
        let mut stream1 = provider.chat_stream(&messages, &[], None).await.unwrap();
        let mut text1 = String::new();
        while let Some(chunk) = stream1.next().await {
            if let StreamChunk::Text(t) = chunk {
                text1.push_str(&t);
            }
        }
        assert_eq!(text1, "First");

        // Second call
        let messages = vec![Message::user("2")];
        let mut stream2 = provider.chat_stream(&messages, &[], None).await.unwrap();
        let mut text2 = String::new();
        while let Some(chunk) = stream2.next().await {
            if let StreamChunk::Text(t) = chunk {
                text2.push_str(&t);
            }
        }
        assert_eq!(text2, "Second");
    }

    #[tokio::test]
    async fn test_mock_provider_with_delay() {
        let provider = MockClaudeProvider::new();
        provider.queue_response(CannedResponse::with_delay("Delayed", 50));

        let start = std::time::Instant::now();
        let messages = vec![Message::user("hi")];
        let mut stream = provider.chat_stream(&messages, &[], None).await.unwrap();

        let mut response_text = String::new();
        while let Some(chunk) = stream.next().await {
            if let StreamChunk::Text(t) = chunk {
                response_text.push_str(&t);
            }
        }

        let elapsed = start.elapsed();
        assert_eq!(response_text, "Delayed");
        assert!(elapsed.as_millis() >= 50, "Should have delayed at least 50ms");
    }
}
