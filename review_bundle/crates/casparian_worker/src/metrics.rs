//! Worker Metrics Module
//!
//! Provides in-memory metrics for monitoring Worker health and performance.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Global metrics instance
pub static METRICS: Metrics = Metrics::new();

/// Worker metrics - all fields are atomic for thread-safe access
pub struct Metrics {
    // Job counters
    pub jobs_received: AtomicU64,
    pub jobs_completed: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub jobs_rejected: AtomicU64,

    // Bridge counters
    pub bridge_executions: AtomicU64,
    pub bridge_timeouts: AtomicU64,
    pub bridge_errors: AtomicU64,

    // Parquet counters
    pub parquet_files_written: AtomicU64,
    pub parquet_rows_written: AtomicU64,

    // Message counters
    pub messages_received: AtomicU64,
    pub messages_sent: AtomicU64,

    // Timing (cumulative microseconds)
    pub job_execution_time_us: AtomicU64,
    pub bridge_time_us: AtomicU64,
    pub parquet_write_time_us: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            jobs_received: AtomicU64::new(0),
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            jobs_rejected: AtomicU64::new(0),
            bridge_executions: AtomicU64::new(0),
            bridge_timeouts: AtomicU64::new(0),
            bridge_errors: AtomicU64::new(0),
            parquet_files_written: AtomicU64::new(0),
            parquet_rows_written: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            job_execution_time_us: AtomicU64::new(0),
            bridge_time_us: AtomicU64::new(0),
            parquet_write_time_us: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn inc_jobs_received(&self) {
        self.jobs_received.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_jobs_completed(&self) {
        self.jobs_completed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_jobs_failed(&self) {
        self.jobs_failed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_jobs_rejected(&self) {
        self.jobs_rejected.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_bridge_executions(&self) {
        self.bridge_executions.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_bridge_timeouts(&self) {
        self.bridge_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_bridge_errors(&self) {
        self.bridge_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_parquet_files(&self) {
        self.parquet_files_written.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_parquet_rows(&self, rows: u64) {
        self.parquet_rows_written.fetch_add(rows, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_job_time(&self, start: Instant) {
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.job_execution_time_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_bridge_time(&self, start: Instant) {
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.bridge_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_parquet_time(&self, start: Instant) {
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.parquet_write_time_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            jobs_received: self.jobs_received.load(Ordering::Relaxed),
            jobs_completed: self.jobs_completed.load(Ordering::Relaxed),
            jobs_failed: self.jobs_failed.load(Ordering::Relaxed),
            jobs_rejected: self.jobs_rejected.load(Ordering::Relaxed),
            bridge_executions: self.bridge_executions.load(Ordering::Relaxed),
            bridge_timeouts: self.bridge_timeouts.load(Ordering::Relaxed),
            bridge_errors: self.bridge_errors.load(Ordering::Relaxed),
            parquet_files_written: self.parquet_files_written.load(Ordering::Relaxed),
            parquet_rows_written: self.parquet_rows_written.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            job_execution_time_us: self.job_execution_time_us.load(Ordering::Relaxed),
            bridge_time_us: self.bridge_time_us.load(Ordering::Relaxed),
            parquet_write_time_us: self.parquet_write_time_us.load(Ordering::Relaxed),
        }
    }

    /// Format as Prometheus exposition format
    pub fn prometheus_format(&self) -> String {
        let s = self.snapshot();
        format!(
            r#"# HELP casparian_worker_jobs_received_total Total jobs received from sentinel
# TYPE casparian_worker_jobs_received_total counter
casparian_worker_jobs_received_total {}

# HELP casparian_worker_jobs_completed_total Total jobs completed successfully
# TYPE casparian_worker_jobs_completed_total counter
casparian_worker_jobs_completed_total {}

# HELP casparian_worker_jobs_failed_total Total jobs that failed
# TYPE casparian_worker_jobs_failed_total counter
casparian_worker_jobs_failed_total {}

# HELP casparian_worker_bridge_executions_total Total bridge executions
# TYPE casparian_worker_bridge_executions_total counter
casparian_worker_bridge_executions_total {}

# HELP casparian_worker_bridge_timeouts_total Total bridge timeouts
# TYPE casparian_worker_bridge_timeouts_total counter
casparian_worker_bridge_timeouts_total {}

# HELP casparian_worker_parquet_files_written_total Total parquet files written
# TYPE casparian_worker_parquet_files_written_total counter
casparian_worker_parquet_files_written_total {}

# HELP casparian_worker_parquet_rows_written_total Total rows written to parquet
# TYPE casparian_worker_parquet_rows_written_total counter
casparian_worker_parquet_rows_written_total {}

# HELP casparian_worker_job_execution_time_microseconds_total Cumulative job execution time
# TYPE casparian_worker_job_execution_time_microseconds_total counter
casparian_worker_job_execution_time_microseconds_total {}

# HELP casparian_worker_bridge_time_microseconds_total Cumulative bridge execution time
# TYPE casparian_worker_bridge_time_microseconds_total counter
casparian_worker_bridge_time_microseconds_total {}
"#,
            s.jobs_received,
            s.jobs_completed,
            s.jobs_failed,
            s.bridge_executions,
            s.bridge_timeouts,
            s.parquet_files_written,
            s.parquet_rows_written,
            s.job_execution_time_us,
            s.bridge_time_us,
        )
    }
}

/// Immutable snapshot of metrics
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub jobs_received: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_rejected: u64,
    pub bridge_executions: u64,
    pub bridge_timeouts: u64,
    pub bridge_errors: u64,
    pub parquet_files_written: u64,
    pub parquet_rows_written: u64,
    pub messages_received: u64,
    pub messages_sent: u64,
    pub job_execution_time_us: u64,
    pub bridge_time_us: u64,
    pub parquet_write_time_us: u64,
}

impl MetricsSnapshot {
    pub fn summary(&self) -> String {
        format!(
            "Jobs: {} received, {} completed, {} failed | \
             Bridge: {} executions, {} timeouts | \
             Parquet: {} files, {} rows",
            self.jobs_received,
            self.jobs_completed,
            self.jobs_failed,
            self.bridge_executions,
            self.bridge_timeouts,
            self.parquet_files_written,
            self.parquet_rows_written,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_metrics() {
        let metrics = Metrics::new();
        metrics.inc_jobs_received();
        metrics.inc_jobs_completed();
        metrics.add_parquet_rows(1000);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.jobs_received, 1);
        assert_eq!(snapshot.jobs_completed, 1);
        assert_eq!(snapshot.parquet_rows_written, 1000);
    }
}
