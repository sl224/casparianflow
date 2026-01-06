//! Event handling for the TUI

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::time::Duration;

/// Application events
#[derive(Debug)]
pub enum Event {
    /// Key press
    Key(KeyEvent),
    /// Periodic tick
    Tick,
    /// Terminal resize
    Resize(u16, u16),
}

/// Event handler
pub struct EventHandler {
    /// Tick rate
    tick_rate: Duration,
}

impl EventHandler {
    /// Create new event handler with given tick rate
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    /// Get next event (blocking with timeout)
    pub async fn next(&self) -> Event {
        // Use tokio's spawn_blocking to avoid blocking the async runtime
        let tick_rate = self.tick_rate;

        tokio::task::spawn_blocking(move || {
            if event::poll(tick_rate).unwrap_or(false) {
                match event::read() {
                    Ok(CrosstermEvent::Key(key)) => Event::Key(key),
                    Ok(CrosstermEvent::Resize(w, h)) => Event::Resize(w, h),
                    _ => Event::Tick,
                }
            } else {
                Event::Tick
            }
        })
        .await
        .unwrap_or(Event::Tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_handler_creation() {
        let handler = EventHandler::new(Duration::from_millis(100));
        assert_eq!(handler.tick_rate, Duration::from_millis(100));
    }
}
