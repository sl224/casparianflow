//! Test harness for TUI integration tests
//!
//! Provides a high-level API for:
//! - Setting up deterministic test scenarios
//! - Sending keystrokes to the TUI
//! - Verifying screen output via TestBackend buffer
//! - Multi-turn conversation testing

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::Arc;
use std::time::Duration;

use super::app::{App, AppFocus, Message, MessageRole, TuiMode};
use super::llm::mock::{CannedResponse, MockClaudeProvider};
use super::ui;
use super::TuiArgs;

/// Screen buffer snapshot for assertions
pub struct ScreenSnapshot {
    /// Raw buffer content as single string (row-major, trimmed)
    pub raw: String,
    /// Content split by rows
    pub rows: Vec<String>,
}

impl ScreenSnapshot {
    /// Create snapshot from TestBackend buffer
    pub fn from_backend(backend: &TestBackend) -> Self {
        let buffer = backend.buffer();
        let width = buffer.area.width;
        let height = buffer.area.height;

        // Extract all cell content
        let mut raw = String::new();
        for y in 0..height {
            for x in 0..width {
                let cell = &buffer[(x, y)];
                raw.push_str(cell.symbol());
            }
        }

        // Split into rows and trim trailing whitespace
        let rows: Vec<String> = raw
            .chars()
            .collect::<Vec<_>>()
            .chunks(width as usize)
            .map(|chunk| chunk.iter().collect::<String>().trim_end().to_string())
            .collect();

        Self { raw, rows }
    }

    /// Check if screen contains text anywhere
    pub fn contains(&self, text: &str) -> bool {
        self.raw.contains(text)
    }

    /// Assert screen contains text (with helpful error message)
    pub fn assert_contains(&self, text: &str) {
        assert!(
            self.contains(text),
            "Screen does not contain '{}'\n\nScreen content:\n{}",
            text,
            self.format_screen()
        );
    }

    /// Assert screen does NOT contain text
    pub fn assert_not_contains(&self, text: &str) {
        assert!(
            !self.contains(text),
            "Screen unexpectedly contains '{}'\n\nScreen content:\n{}",
            text,
            self.format_screen()
        );
    }

    /// Format screen for display (with row numbers)
    pub fn format_screen(&self) -> String {
        self.rows
            .iter()
            .enumerate()
            .map(|(i, row)| format!("{:02}|{}", i, row))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Test harness for TUI integration tests
pub struct TuiTestHarness {
    /// The terminal with TestBackend
    terminal: Terminal<TestBackend>,
    /// The application state
    pub app: App,
}

impl TuiTestHarness {
    /// Create a new test harness with default 80x40 terminal
    pub fn new() -> Self {
        Self::with_size(80, 40)
    }

    /// Create a new test harness with specified terminal size
    pub fn with_size(width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("Failed to create test terminal");

        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "mock-test".into(),
        };

        let mut app = App::new(args);
        // Start in Discover mode 
        app.mode = TuiMode::Discover;
        // Open sidebar for chat testing
        app.show_chat_sidebar = true;
        app.focus = super::app::AppFocus::Chat;

        Self { terminal, app }
    }

    /// Render the current app state and return screen snapshot
    pub fn render(&mut self) -> ScreenSnapshot {
        self.terminal
            .draw(|frame| ui::draw(frame, &self.app))
            .expect("Failed to draw");

        ScreenSnapshot::from_backend(self.terminal.backend())
    }

    /// Send a single key event
    pub async fn send_key(&mut self, key: KeyEvent) {
        self.app.handle_key(key).await;
    }

    /// Type a string (sends each character as a key event)
    pub async fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .await;
        }
    }

    /// Press Enter to send message
    pub async fn press_enter(&mut self) {
        self.send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
    }

    /// Press Shift+Enter for newline
    pub async fn press_shift_enter(&mut self) {
        self.send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))
            .await;
    }

    /// Press Escape
    pub async fn press_escape(&mut self) {
        self.send_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;
    }

    /// Press Ctrl+C
    pub async fn press_ctrl_c(&mut self) {
        self.send_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .await;
    }

    /// Toggle Chat Sidebar (Alt+A)
    pub async fn toggle_chat(&mut self) {
        self.send_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT))
            .await;
    }

    /// Run tick (polls for pending responses)
    pub async fn tick(&mut self) {
        self.app.tick().await;
    }

    /// Wait for response to arrive (polls tick until not awaiting)
    pub async fn wait_for_response(&mut self, timeout: Duration) {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(10);

        while start.elapsed() < timeout && self.app.chat.awaiting_response {
            self.tick().await;
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Send a message and wait for response
    ///
    /// High-level helper that:
    /// 1. Types the message
    /// 2. Presses Enter
    /// 3. Polls tick() until response arrives (or timeout)
    /// 4. Returns screen snapshot
    pub async fn send_message_and_wait(
        &mut self,
        message: &str,
        timeout: Duration,
    ) -> ScreenSnapshot {
        // Type and send
        self.type_text(message).await;
        self.press_enter().await;

        // Wait for response
        self.wait_for_response(timeout).await;

        // Return final screen
        self.render()
    }

    /// Get chat messages
    pub fn messages(&self) -> &[Message] {
        &self.app.chat.messages
    }

    /// Check if awaiting response
    pub fn is_awaiting_response(&self) -> bool {
        self.app.chat.awaiting_response
    }

    /// Check if app is running
    pub fn is_running(&self) -> bool {
        self.app.running
    }

    /// Get the current input text
    pub fn input(&self) -> &str {
        &self.app.chat.input
    }
}

impl Default for TuiTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Builder Pattern for Complex Scenarios
// =============================================================================

/// Builder for setting up test scenarios
pub struct TuiTestBuilder {
    width: u16,
    height: u16,
    responses: Vec<CannedResponse>,
    initial_messages: Vec<Message>,
}

impl TuiTestBuilder {
    pub fn new() -> Self {
        Self {
            width: 80,
            height: 40, // Taller to show more messages
            responses: vec![],
            initial_messages: vec![],
        }
    }

    pub fn with_response(mut self, response: CannedResponse) -> Self {
        self.responses.push(response);
        self
    }

    pub fn build(self) -> TuiTestHarness {
        let backend = TestBackend::new(self.width, self.height);
        let terminal = Terminal::new(backend).expect("Failed to create test terminal");

        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "mock-test".into(),
        };

        // Create mock provider and queue responses
        let mock_provider = Arc::new(MockClaudeProvider::new());
        for response in self.responses {
            mock_provider.queue_response(response);
        }

        // Create app with mock provider
        let mut app = App::new_with_provider(args, mock_provider);

        // Start in Discover mode
        app.mode = TuiMode::Discover;
        // Open sidebar
        app.show_chat_sidebar = true;
        app.focus = super::app::AppFocus::Chat;

        // Add initial messages
        for msg in self.initial_messages {
            app.chat.messages.push(msg);
        }

        TuiTestHarness { terminal, app }
    }
}

impl Default for TuiTestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests for the test harness itself
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_snapshot_contains() {
        let backend = TestBackend::new(10, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        // Draw some text
        terminal
            .draw(|frame| {
                let text = ratatui::widgets::Paragraph::new("Hello");
                frame.render_widget(text, frame.area());
            })
            .unwrap();

        let snapshot = ScreenSnapshot::from_backend(terminal.backend());
        assert!(snapshot.contains("Hello"));
        assert!(!snapshot.contains("World"));
    }

    #[tokio::test]
    async fn test_harness_typing() {
        let mut harness = TuiTestHarness::new();

        harness.type_text("hello").await;

        assert_eq!(harness.input(), "hello");
    }

    #[tokio::test]
    async fn test_harness_chat_toggling() {
        let mut harness = TuiTestHarness::new();

        assert!(harness.app.show_chat_sidebar); // Default in test harness

        harness.toggle_chat().await;
        assert!(!harness.app.show_chat_sidebar);

        harness.toggle_chat().await;
        assert!(harness.app.show_chat_sidebar);
    }

    #[tokio::test]
    async fn test_builder_with_responses() {
        let harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Test response"))
            .build();

        // Verify mock provider is set up
        assert!(harness.app.llm_provider.is_some());
    }

    #[tokio::test]
    async fn test_ctrl_c_quits() {
        let mut harness = TuiTestHarness::new();

        assert!(harness.is_running());

        harness.press_ctrl_c().await;

        assert!(!harness.is_running());
    }

    #[tokio::test]
    async fn test_escape_returns_to_home() {
        let mut harness = TuiTestHarness::new();
        // Harness starts in Discover mode with Chat focus

        harness.type_text("some text").await;
        assert_eq!(harness.input(), "some text");

        harness.press_escape().await;
        // First Esc clears input/unfocuses chat
        assert!(matches!(harness.app.focus, AppFocus::Main));
        assert!(matches!(harness.app.mode, TuiMode::Discover));

        harness.press_escape().await;
        // Second Esc returns to Home mode
        assert!(matches!(harness.app.mode, TuiMode::Home));
    }

    // =========================================================================
    // AUTONOMOUS WORKFLOW TESTS
    // These test actual multi-turn conversations with mock responses
    // =========================================================================

    /// Test: verify the mock provider path is taken
    #[tokio::test]
    async fn test_mock_provider_path_is_taken() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Mock response"))
            .build();

        // Verify provider is set
        assert!(
            harness.app.llm_provider.is_some(),
            "llm_provider should be Some"
        );

        // Type and send
        harness.type_text("hello").await;
        assert_eq!(harness.input(), "hello");

        // Press enter - this should trigger send_message which should use the mock provider
        harness.press_enter().await;

        // Verify awaiting_response is true (meaning send_message was called)
        assert!(
            harness.app.chat.awaiting_response,
            "Should be awaiting response after pressing enter"
        );

        // Verify "Thinking..." message was added
        let last_msg = harness.app.chat.messages.last();
        assert!(
            last_msg.is_some(),
            "Should have at least one message after send"
        );
        let last_msg = last_msg.unwrap();

        // The mock provider path should add "Thinking..." as the last message
        assert!(
            last_msg.content.starts_with("Thinking"),
            "Last message should be 'Thinking...' but was: {}",
            last_msg.content
        );
    }

    /// Test: verify response arrives via tick()
    #[tokio::test]
    async fn test_response_arrives_via_tick() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Test response arrived"))
            .build();

        // Send message
        harness.type_text("test").await;
        harness.press_enter().await;

        // Wait a bit for the spawned task to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now poll tick - this should receive the response
        harness.tick().await;

        // Check if response arrived
        let last_msg = harness.app.chat.messages.last().unwrap();

        // Should have received the response
        assert!(
            !harness.app.chat.awaiting_response,
            "Should not be awaiting after tick receives response"
        );
        assert_eq!(
            last_msg.content, "Test response arrived",
            "Response should have replaced Thinking..."
        );
    }

    #[tokio::test]
    async fn test_single_turn_shows_user_message() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Hello! How can I help you today?"))
            .build();

        // Type and send message
        harness.type_text("Hi there").await;
        harness.press_enter().await;

        // Wait for response
        harness.wait_for_response(Duration::from_secs(5)).await;

        // Render and verify
        let screen = harness.render();

        // User message should be visible
        screen.assert_contains("Hi there");

        // Should not be stuck on "Thinking"
        assert!(
            !harness.is_awaiting_response(),
            "Should not be awaiting response after receiving it"
        );
    }

    #[tokio::test]
    async fn test_single_turn_shows_assistant_response() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Hello! How can I help you today?"))
            .build();

        // Send message and wait
        let screen = harness
            .send_message_and_wait("Hi", Duration::from_secs(5))
            .await;

        // Assistant response should be visible (check parts due to wrapping)
        screen.assert_contains("Hello!");
        screen.assert_contains("today?");
    }

    #[tokio::test]
    async fn test_response_replaces_thinking_indicator() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Test response"))
            .build();

        // Type and send
        harness.type_text("test").await;
        harness.press_enter().await;

        // Wait for response
        harness.wait_for_response(Duration::from_secs(5)).await;

        // Verify response arrived
        let screen = harness.render();

        // Response should be visible
        screen.assert_contains("Test response");

        // "Thinking" should NOT be visible (replaced by actual response)
        screen.assert_not_contains("Thinking...");
    }

    #[tokio::test]
    async fn test_multi_turn_conversation() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("First response from Claude"))
            .with_response(CannedResponse::text("Second response from Claude"))
            .build();

        // First turn
        harness
            .send_message_and_wait("First message", Duration::from_secs(5))
            .await;

        // Second turn
        let screen = harness
            .send_message_and_wait("Second message", Duration::from_secs(5))
            .await;

        // Both user messages should be visible
        screen.assert_contains("First message");
        screen.assert_contains("Second message");

        // Both assistant responses should be visible
        screen.assert_contains("First response");
        screen.assert_contains("from Claude");
        screen.assert_contains("Second response");

        // Verify internal state matches (includes system welcome message)
        assert_eq!(
            harness.messages().len(),
            5,
            "Should have 5 messages (1 system + 2 user + 2 assistant)"
        );
    }

    #[tokio::test]
    async fn test_three_turn_conversation() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Response 1"))
            .with_response(CannedResponse::text("Response 2"))
            .with_response(CannedResponse::text("Response 3"))
            .build();

        // Three turns
        harness
            .send_message_and_wait("Message 1", Duration::from_secs(5))
            .await;
        harness
            .send_message_and_wait("Message 2", Duration::from_secs(5))
            .await;
        let screen = harness
            .send_message_and_wait("Message 3", Duration::from_secs(5))
            .await;

        // All messages visible
        screen.assert_contains("Message 1");
        screen.assert_contains("Message 2");
        screen.assert_contains("Message 3");
        screen.assert_contains("Response 1");
        screen.assert_contains("Response 2");
        screen.assert_contains("Response 3");

        // 7 total messages (1 system + 3 user + 3 assistant)
        assert_eq!(harness.messages().len(), 7);
    }

    #[tokio::test]
    async fn test_delayed_response_still_arrives() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::with_delay("Delayed response", 100))
            .build();

        // Type and send
        harness.type_text("test").await;
        harness.press_enter().await;

        // Immediately after sending, should be awaiting
        assert!(
            harness.is_awaiting_response(),
            "Should be awaiting response immediately after sending"
        );

        // Wait for response (longer than delay)
        harness.wait_for_response(Duration::from_secs(5)).await;

        // Response should arrive
        let screen = harness.render();
        screen.assert_contains("Delayed response");

        // No longer awaiting
        assert!(
            !harness.is_awaiting_response(),
            "Should not be awaiting after response arrives"
        );
    }


    #[tokio::test]
    async fn test_multiline_input() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Got your multiline message"))
            .build();

        // Type multiline input using Shift+Enter
        harness.type_text("line1").await;
        harness.press_shift_enter().await;
        harness.type_text("line2").await;

        // Verify input has newline
        assert!(harness.input().contains('\n'), "Should have newline");
        assert!(harness.input().contains("line1"), "Should have line1");
        assert!(harness.input().contains("line2"), "Should have line2");

        // Send and verify response
        harness.press_enter().await;
        harness.wait_for_response(Duration::from_secs(5)).await;

        let screen = harness.render();
        screen.assert_contains("Got your");
        screen.assert_contains("multiline");
    }

    #[tokio::test]
    async fn test_messages_have_correct_roles() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Assistant message"))
            .build();

        harness
            .send_message_and_wait("User message", Duration::from_secs(5))
            .await;

        let messages = harness.messages();
        // 1 system welcome + 1 user + 1 assistant = 3 messages
        assert_eq!(messages.len(), 3);

        assert!(
            matches!(messages[0].role, MessageRole::System),
            "First message should be System welcome"
        );
        assert!(
            matches!(messages[1].role, MessageRole::User),
            "Second message should be from User"
        );
        assert!(
            matches!(messages[2].role, MessageRole::Assistant),
            "Third message should be from Assistant"
        );
    }

    #[tokio::test]
    async fn test_app_stays_running_during_conversation() {
        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text("Response"))
            .build();

        harness
            .send_message_and_wait("test", Duration::from_secs(5))
            .await;

        assert!(
            harness.is_running(),
            "App should still be running after conversation"
        );
    }

    #[tokio::test]
    async fn test_long_message_renders() {
        let long_response = "This is a very long response message that should span multiple lines when rendered in the TUI. It contains enough text to test word wrapping and ensure that long messages are displayed correctly without truncation or rendering issues.";

        let mut harness = TuiTestBuilder::new()
            .with_response(CannedResponse::text(long_response))
            .build();

        harness
            .send_message_and_wait("test", Duration::from_secs(5))
            .await;

        let screen = harness.render();

        // At minimum, the start of the message should be visible
        screen.assert_contains("This is a very");
    }
}
