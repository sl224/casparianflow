use casparian_sentinel::ControlClient;
use std::time::Duration;

pub enum BackendMode {
    Connected(ControlClient),
    Offline,
}

#[derive(Debug)]
pub enum BackendError {
    ControlUnavailable(String),
    ControlNotConfigured,
    ReadOnly,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::ControlUnavailable(message) => {
                write!(f, "Sentinel not reachable: {}", message)
            }
            BackendError::ControlNotConfigured => {
                write!(f, "Control API not configured; run with --standalone-writer")
            }
            BackendError::ReadOnly => write!(f, "Database is read-only"),
        }
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone)]
pub struct BackendRouter {
    control_addr: Option<String>,
    standalone_writer: bool,
    db_read_only: bool,
}

impl BackendRouter {
    pub fn new(
        control_addr: Option<String>,
        standalone_writer: bool,
        db_read_only: bool,
    ) -> Self {
        Self {
            control_addr,
            standalone_writer,
            db_read_only,
        }
    }

    pub fn mutations_blocked(&self, control_connected: bool) -> bool {
        if control_connected {
            return false;
        }
        if self.db_read_only {
            return true;
        }
        !self.standalone_writer
    }

    pub fn blocked_message(&self, action: &str) -> String {
        if self.db_read_only {
            format!("Database is read-only; cannot {}", action)
        } else {
            format!("Sentinel not reachable; cannot {}", action)
        }
    }

    pub fn for_mutation(&self, timeout: Duration) -> Result<BackendMode, BackendError> {
        if let Some(addr) = self.control_addr.as_deref() {
            match ControlClient::connect_with_timeout(addr, timeout) {
                Ok(client) => return Ok(BackendMode::Connected(client)),
                Err(err) => {
                    if self.standalone_writer && !self.db_read_only {
                        return Ok(BackendMode::Offline);
                    }
                    return Err(BackendError::ControlUnavailable(err.to_string()));
                }
            }
        }

        if self.standalone_writer && !self.db_read_only {
            Ok(BackendMode::Offline)
        } else if self.db_read_only {
            Err(BackendError::ReadOnly)
        } else {
            Err(BackendError::ControlNotConfigured)
        }
    }
}
