//! Shared logging utilities for Casparian binaries.

use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

const DEFAULT_LOG_FILTER: &str = "casparian=info,casparian_sentinel=info,casparian_worker=info";
const MAX_LOG_FILES: usize = 5;
const MAX_LOG_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Logging configuration shared by Casparian binaries.
pub struct LogConfig<'a> {
    pub app_name: &'a str,
    pub verbose: bool,
    pub tui_mode: bool,
}

/// Initialize tracing with a rolling file writer and stderr output.
pub fn init_logging(config: LogConfig<'_>) -> Result<()> {
    let log_dir = ensure_logs_dir().context("Failed to ensure log directory")?;
    let file_writer = SharedRollingWriter::new(log_dir, config.app_name)
        .context("Failed to initialize rolling log writer")?;

    let file_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

    let console_filter = if config.verbose {
        file_filter.clone()
    } else if config.tui_mode {
        EnvFilter::new("warn")
    } else {
        file_filter.clone()
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_filter(file_filter),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(console_filter),
        )
        .init();

    Ok(())
}

/// Get the Casparian home directory: ~/.casparian_flow
pub fn casparian_home() -> PathBuf {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return PathBuf::from(override_path);
    }
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".casparian_flow")
}

/// Get the logs directory: ~/.casparian_flow/logs
pub fn logs_dir() -> PathBuf {
    casparian_home().join("logs")
}

/// Get the jobs logs directory: ~/.casparian_flow/logs/jobs
pub fn jobs_log_dir() -> PathBuf {
    logs_dir().join("jobs")
}

/// Ensure the logs directory exists.
pub fn ensure_logs_dir() -> Result<PathBuf> {
    let logs = logs_dir();
    fs::create_dir_all(&logs)
        .with_context(|| format!("Failed to create logs directory: {}", logs.display()))?;
    let jobs = jobs_log_dir();
    fs::create_dir_all(&jobs)
        .with_context(|| format!("Failed to create job logs directory: {}", jobs.display()))?;
    Ok(logs)
}

struct RollingFileAppender {
    dir: PathBuf,
    base_name: String,
    max_files: usize,
    max_size: u64,
    file: Option<File>,
    current_size: u64,
}

impl RollingFileAppender {
    fn new(dir: PathBuf, base_name: &str, max_files: usize, max_size: u64) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut appender = Self {
            dir,
            base_name: sanitize_name(base_name),
            max_files: max_files.max(1),
            max_size,
            file: None,
            current_size: 0,
        };
        let (file, size) = appender.open_current_file()?;
        appender.file = Some(file);
        appender.current_size = size;
        if appender.current_size > appender.max_size {
            appender.rotate()?;
        }
        Ok(appender)
    }

    fn open_current_file(&self) -> io::Result<(File, u64)> {
        let path = self.current_path();
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok((file, size))
    }

    fn current_path(&self) -> PathBuf {
        self.dir.join(format!("{}.log", self.base_name))
    }

    fn rotated_path(&self, index: usize) -> PathBuf {
        self.dir
            .join(format!("{}.log.{}", self.base_name, index))
    }

    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }

        self.rotate_files()?;

        let (file, size) = self.open_current_file()?;
        self.file = Some(file);
        self.current_size = size;
        Ok(())
    }

    fn rotate_files(&self) -> io::Result<()> {
        let max_index = self.max_files.saturating_sub(1);
        if max_index == 0 {
            return Ok(());
        }

        let oldest = self.rotated_path(max_index);
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }

        for idx in (1..max_index).rev() {
            let src = self.rotated_path(idx);
            if src.exists() {
                let dst = self.rotated_path(idx + 1);
                fs::rename(&src, &dst)?;
            }
        }

        let current = self.current_path();
        if current.exists() {
            let first = self.rotated_path(1);
            fs::rename(current, first)?;
        }

        Ok(())
    }
}

impl Write for RollingFileAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.current_size + buf.len() as u64 > self.max_size {
            self.rotate()?;
        }

        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "log file unavailable"))?;
        let bytes = file.write(buf)?;
        self.current_size += bytes as u64;
        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct SharedRollingWriter {
    inner: Arc<Mutex<RollingFileAppender>>,
}

impl SharedRollingWriter {
    fn new(dir: PathBuf, base_name: &str) -> Result<Self> {
        let appender = RollingFileAppender::new(dir, base_name, MAX_LOG_FILES, MAX_LOG_FILE_SIZE)
            .with_context(|| format!("Failed to open log file for {}", base_name))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(appender)),
        })
    }
}

struct SharedRollingWriterGuard {
    inner: Arc<Mutex<RollingFileAppender>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedRollingWriter {
    type Writer = SharedRollingWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedRollingWriterGuard {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Write for SharedRollingWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "log writer lock poisoned"))?;
        guard.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "log writer lock poisoned"))?;
        guard.flush()
    }
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .collect()
}
