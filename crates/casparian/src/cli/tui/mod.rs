//! Terminal User Interface for Casparian Flow
//!
//! Provides an interactive TUI with two main capabilities:
//! - Chat: Converse with LLM to scan files, define schemas, and build pipelines
//! - Monitor: Watch job status and pipeline health
//!
//! ## Chat View Features
//!
//! - **Multi-line input**: Use Shift+Enter to add newlines within your message
//! - **Input history**: Use Up/Down arrows to recall previous messages
//! - **Message scrolling**: Use Ctrl+Up/Down or PageUp/PageDown to scroll
//! - **Timestamps**: Each message displays when it was sent
//! - **Visual distinction**: Different colors for User, Assistant, System, Tool messages

pub mod app;
pub mod event;
pub mod llm;
pub mod ui;

#[cfg(test)]
pub mod test_harness;

use anyhow::Result;
use clap::Args;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::path::PathBuf;

use crate::cli::tui::app::App;
use crate::cli::tui::event::{Event, EventHandler};

/// TUI command arguments
#[derive(Debug, Args)]
pub struct TuiArgs {
    /// Database path override (defaults to ~/.casparian_flow/casparian_flow.sqlite3)
    #[arg(long)]
    pub database: Option<PathBuf>,

    /// API key for LLM provider (defaults to ANTHROPIC_API_KEY env var)
    #[arg(long, env = "ANTHROPIC_API_KEY")]
    pub api_key: Option<String>,

    /// LLM model to use
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    pub model: String,
}

/// Run the TUI
pub async fn run(args: TuiArgs) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(args);

    // Create event handler
    let mut events = EventHandler::new(std::time::Duration::from_millis(250));

    // Main loop
    let result = run_app(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Run the application loop
async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    events: &mut EventHandler,
) -> Result<()> {
    while app.running {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, app))?;

        // Handle events
        match events.next().await {
            Event::Key(key) => app.handle_key(key).await,
            Event::Tick => app.tick().await,
            Event::Resize(_, _) => {} // Ratatui handles resize
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_app_starts_in_chat_view() {
        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        };
        let app = App::new(args);
        assert!(matches!(app.view, app::View::Chat));
        assert!(app.running);
    }

    #[test]
    fn test_app_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        };
        let app = App::new(args);

        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

        // Should render without panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == 80);
        assert!(buffer.area.height == 24);
    }

    #[test]
    fn test_app_renders_with_multiline_input() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        };
        let mut app = App::new(args);
        app.chat.input = "line1\nline2\nline3".into();
        app.chat.cursor = app.chat.input.len();

        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

        // Should render without panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == 80);
    }

    #[test]
    fn test_app_renders_with_long_messages() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let args = TuiArgs {
            database: None,
            api_key: None,
            model: "test".into(),
        };
        let mut app = App::new(args);

        // Add many messages to test scrolling
        for i in 0..20 {
            app.chat.messages.push(app::Message::new(
                app::MessageRole::User,
                format!("Test message number {} with some long content that might wrap to multiple lines when rendered in the TUI", i),
            ));
        }

        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

        // Should render without panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == 80);
    }
}
