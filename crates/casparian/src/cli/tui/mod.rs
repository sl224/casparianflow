//! Terminal User Interface for Casparian Flow
//!
//! Provides an interactive TUI to scan sources, manage jobs, and build rules.

pub mod app;
pub mod event;
pub mod extraction;
pub mod flow;
pub mod flow_assert;
pub mod flow_record;
pub mod flow_runner;
pub mod nav;
pub mod pattern_query;
pub mod snapshot;
pub mod snapshot_export;
pub mod snapshot_states;
pub mod state_graph;
pub mod ui;
pub mod ui_signature;
pub mod ux_lint;

#[cfg(feature = "profiling")]
mod profiler_overlay;

use anyhow::Result;
use clap::Args;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::*, Terminal};
use std::io::stdout;
use std::path::PathBuf;

use crate::cli::tui::app::App;
use crate::cli::tui::event::{Event, EventHandler};
use crate::cli::tui::flow::TerminalSize;
use crate::cli::tui::flow_record::{FlowRecorder, RecordRedaction};
use casparian::telemetry::TelemetryRecorder;

/// TUI command arguments
#[derive(Debug, Args)]
pub struct TuiArgs {
    /// Database path override (defaults to active backend path)
    #[arg(long)]
    pub database: Option<PathBuf>,
    /// Record a TUI flow to the given path (JSON)
    #[arg(long)]
    pub record_flow: Option<PathBuf>,
    /// Redact recorded text input (plaintext, hash, omit)
    #[arg(long, value_enum, default_value = "plaintext")]
    pub record_redaction: RecordRedaction,
    /// Insert a checkpoint assertion every N milliseconds (0 to disable)
    #[arg(long)]
    pub record_checkpoint_every: Option<u64>,
}

/// Run the TUI
pub fn run(args: TuiArgs, telemetry: Option<TelemetryRecorder>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let terminal_area = terminal.size()?;

    let record_flow = args.record_flow.clone();
    let record_redaction = args.record_redaction;
    let record_checkpoint_every = args.record_checkpoint_every;
    let database = args.database.clone();

    // Create app state
    let mut app = App::new(args, telemetry);

    // Create event handler
    let mut events = EventHandler::new(std::time::Duration::from_millis(250));

    let checkpoint_every_ms = if record_flow.is_some() {
        match record_checkpoint_every {
            Some(0) => None,
            Some(ms) => Some(ms),
            None => Some(2000),
        }
    } else {
        None
    };

    let mut recorder = record_flow.as_ref().map(|path| {
        FlowRecorder::new(
            path.clone(),
            record_redaction,
            TerminalSize {
                width: terminal_area.width,
                height: terminal_area.height,
            },
            database.clone(),
            checkpoint_every_ms,
        )
    });

    // Main loop
    let result = run_app(&mut terminal, &mut app, &mut events, recorder.as_mut());

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let record_result = if let Some(recorder) = recorder {
        recorder.finish(&app)
    } else {
        Ok(())
    };

    if let Err(err) = result {
        if let Err(record_err) = record_result {
            eprintln!("Failed to write recorded flow: {:?}", record_err);
        }
        return Err(err);
    }

    record_result
}

/// Run the application loop
fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    events: &mut EventHandler,
    mut recorder: Option<&mut FlowRecorder>,
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
        match events.next() {
            Event::Key(key) => {
                app.handle_key(key);
                if let Some(recorder) = recorder.as_mut() {
                    recorder.record_key(key, app);
                }
            }
            Event::Tick => app.tick(),
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;

    #[test]
    fn test_app_starts_in_home_mode() {
        let args = TuiArgs {
            database: None,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        };
        let app = App::new(args, None);
        assert!(matches!(app.mode, app::TuiMode::Home));
        assert!(app.running);
    }

    #[test]
    fn test_app_quit_flow() {
        let args = TuiArgs {
            database: None,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        };
        let mut app = App::new(args, None);
        app.tick();
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.running);
    }

    #[test]
    fn test_app_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let args = TuiArgs {
            database: None,
            record_flow: None,
            record_redaction: RecordRedaction::Plaintext,
            record_checkpoint_every: None,
        };
        let app = App::new(args, None);

        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

        // Should render without panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == 80);
        assert!(buffer.area.height == 24);
    }
}
