//! Terminal User Interface for Casparian Flow
//!
//! Provides an interactive TUI to scan sources, manage jobs, and build rules.

pub mod app;
pub mod event;
pub mod extraction;
pub mod pattern_query;
pub mod ui;

#[cfg(feature = "profiling")]
mod profiler_overlay;

use anyhow::Result;
use clap::Args;
use crossterm::{
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
    /// Database path override (defaults to active backend path)
    #[arg(long)]
    pub database: Option<PathBuf>,
}

/// Run the TUI
pub async fn run(args: TuiArgs) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
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
        LeaveAlternateScreen
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
        // Begin profiling frame
        #[cfg(feature = "profiling")]
        app.profiler.begin_frame();

        // Draw UI
        terminal.draw(|frame| {
            ui::draw(frame, app);

            // Render profiler overlay if enabled
            #[cfg(feature = "profiling")]
            if app.profiler.enabled {
                profiler_overlay::render(frame, &app.profiler);
            }
        })?;

        // Handle events
        match events.next().await {
            Event::Key(key) => app.handle_key(key).await,
            Event::Tick => app.tick().await,
            Event::Resize(_, _) => {} // Ratatui handles resize
        }

        // End profiling frame
        #[cfg(feature = "profiling")]
        app.profiler.end_frame();

        // Check for profiler dump trigger (testing integration)
        #[cfg(feature = "profiling")]
        app.check_profiler_dump();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_app_starts_in_home_mode() {
        let args = TuiArgs {
            database: None,
        };
        let app = App::new(args);
        assert!(matches!(app.mode, app::TuiMode::Home));
        assert!(app.running);
    }

    #[test]
    fn test_app_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let args = TuiArgs {
            database: None,
        };
        let app = App::new(args);

        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

        // Should render without panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == 80);
        assert!(buffer.area.height == 24);
    }

}
