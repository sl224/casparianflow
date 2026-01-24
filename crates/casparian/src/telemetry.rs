//! Telemetry helpers for Tape domain events.

use casparian_protocol::telemetry::{self, TelemetryHasher};
use casparian_tape::{EventName, TapeWriter};
use serde::Serialize;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone)]
pub struct TelemetryRecorder {
    writer: Arc<TapeWriter>,
    hasher: TelemetryHasher,
}

impl TelemetryRecorder {
    pub fn new(writer: Arc<TapeWriter>) -> std::io::Result<Self> {
        let hasher = TelemetryHasher::load_or_create()?;
        Ok(Self { writer, hasher })
    }

    pub fn hasher(&self) -> &TelemetryHasher {
        &self.hasher
    }

    pub fn emit_domain<T: Serialize>(
        &self,
        event_name: &str,
        correlation_id: Option<&str>,
        parent_id: Option<&str>,
        payload: &T,
    ) -> Option<String> {
        let payload = match serde_json::to_value(payload) {
            Ok(value) => value,
            Err(err) => {
                warn!("Failed to serialize telemetry payload: {}", err);
                return None;
            }
        };

        match self.writer.emit(
            EventName::DomainEvent(event_name.to_string()),
            correlation_id,
            parent_id,
            payload,
        ) {
            Ok(event_id) => Some(event_id),
            Err(err) => {
                warn!("Failed to emit telemetry event {}: {}", event_name, err);
                None
            }
        }
    }
}

pub fn scan_config_telemetry(config: &crate::scout::ScanConfig) -> telemetry::ScanConfigTelemetry {
    telemetry::ScanConfigTelemetry {
        threads: config.threads,
        batch_size: config.batch_size,
        progress_interval: config.progress_interval,
        follow_symlinks: config.follow_symlinks,
        include_hidden: config.include_hidden,
        compute_stats: config.compute_stats,
    }
}
