//! Recording support for TUI flows.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use blake3::Hasher;
use clap::ValueEnum;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::cli::config::casparian_home;
use crate::cli::tui::app::{App, TuiMode};
use crate::cli::tui::flow::{FlowAssertion, FlowEnv, FlowStep, FlowKey, TerminalSize, TuiFlow};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RecordRedaction {
    Plaintext,
    Hash,
    Omit,
}

pub struct FlowRecorder {
    path: PathBuf,
    redaction: RecordRedaction,
    checkpoint_every: Option<Duration>,
    flow: TuiFlow,
    pending_text: String,
    pending_len: usize,
    pending_hasher: Hasher,
    last_checkpoint_at: Instant,
}

impl FlowRecorder {
    pub fn new(
        path: PathBuf,
        redaction: RecordRedaction,
        terminal: TerminalSize,
        database: Option<PathBuf>,
        checkpoint_every_ms: Option<u64>,
    ) -> Self {
        let checkpoint_every = checkpoint_every_ms.and_then(|ms| {
            if ms == 0 {
                None
            } else {
                Some(Duration::from_millis(ms))
            }
        });

        let env = FlowEnv {
            casparian_home: Some(casparian_home()),
            database,
            terminal: Some(terminal),
            seed: None,
            fixture: None,
        };

        Self {
            path,
            redaction,
            checkpoint_every,
            flow: TuiFlow {
                version: 1,
                env,
                steps: Vec::new(),
            },
            pending_text: String::new(),
            pending_len: 0,
            pending_hasher: Hasher::new(),
            last_checkpoint_at: Instant::now(),
        }
    }

    pub fn record_key(&mut self, key: KeyEvent, app: &App) {
        if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
            return;
        }

        if let Some(ch) = Self::text_char(key) {
            self.push_text_char(ch);
            self.maybe_checkpoint(app);
            return;
        }

        self.flush_text();

        if let Some(flow_key) = FlowKey::from_key_event(key) {
            self.flow
                .steps
                .push(FlowStep::Key { key: flow_key, label: None });
        }

        self.maybe_checkpoint(app);
    }

    pub fn finish(mut self, app: &App) -> Result<()> {
        self.flush_text();

        if self.flow.steps.is_empty() {
            return Ok(());
        }

        if self.checkpoint_every.is_some() {
            self.push_checkpoint(app, "final");
        }

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
        }

        let payload = serde_json::to_string_pretty(&self.flow)?;
        std::fs::write(&self.path, payload)
            .with_context(|| format!("write {}", self.path.display()))?;
        Ok(())
    }

    fn text_char(key: KeyEvent) -> Option<char> {
        match key.code {
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(ch)
            }
            _ => None,
        }
    }

    fn push_text_char(&mut self, ch: char) {
        self.pending_len = self.pending_len.saturating_add(1);
        match self.redaction {
            RecordRedaction::Plaintext => {
                self.pending_text.push(ch);
            }
            RecordRedaction::Hash => {
                self.pending_text.push('*');
                let mut buf = [0; 4];
                let bytes = ch.encode_utf8(&mut buf);
                self.pending_hasher.update(bytes.as_bytes());
            }
            RecordRedaction::Omit => {
                let mut buf = [0; 4];
                let bytes = ch.encode_utf8(&mut buf);
                self.pending_hasher.update(bytes.as_bytes());
            }
        }
    }

    fn flush_text(&mut self) {
        if self.pending_len == 0 {
            return;
        }

        let (text, label) = match self.redaction {
            RecordRedaction::Plaintext => (self.pending_text.clone(), None),
            RecordRedaction::Hash => {
                let hash = self.pending_hasher.finalize().to_hex().to_string();
                (
                    self.pending_text.clone(),
                    Some(format!("redacted=hash len={} blake3={}", self.pending_len, hash)),
                )
            }
            RecordRedaction::Omit => (
                String::new(),
                Some(format!("redacted=omit len={}", self.pending_len)),
            ),
        };

        self.flow
            .steps
            .push(FlowStep::Text { text, label });
        self.pending_text.clear();
        self.pending_len = 0;
        self.pending_hasher = Hasher::new();
    }

    fn maybe_checkpoint(&mut self, app: &App) {
        let Some(every) = self.checkpoint_every else {
            return;
        };

        if self.last_checkpoint_at.elapsed() >= every {
            self.flush_text();
            self.push_checkpoint(app, "checkpoint");
            self.last_checkpoint_at = Instant::now();
        }
    }

    fn push_checkpoint(&mut self, app: &App, label: &str) {
        let view = mode_label(app.mode);
        let assert = FlowAssertion {
            plain_contains: vec![format!("View: {}", view)],
            ..FlowAssertion::default()
        };
        self.flow.steps.push(FlowStep::Assert {
            assert,
            label: Some(label.to_string()),
        });
    }
}

fn mode_label(mode: TuiMode) -> &'static str {
    match mode {
        TuiMode::Home => "Home",
        TuiMode::Discover => "Discover",
        TuiMode::Jobs => "Jobs",
        TuiMode::Sources => "Sources",
        TuiMode::Approvals => "Approvals",
        TuiMode::ParserBench => "Parser Bench",
        TuiMode::Query => "Query",
        TuiMode::Settings => "Settings",
        TuiMode::Sessions => "Sessions",
        TuiMode::Triage => "Triage",
        TuiMode::Catalog => "Catalog",
    }
}
