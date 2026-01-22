//! Metrics Module for Observability
//!
//! Provides in-memory metrics for monitoring Sentinel health and performance.
//! Designed for easy integration with Prometheus or other metrics systems.
//!
//! ## Design Principles (Data-Oriented)
//! - Plain data structures, no OOP
//! - Lock-free atomics where possible
//! - Single writer, multiple readers pattern

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Global metrics instance - lock-free atomics for counters
pub static METRICS: Metrics = Metrics::new();

/// Sentinel metrics - all fields are atomic for thread-safe access
pub struct Metrics {
    // Job counters
    pub jobs_dispatched: AtomicU64,
    pub jobs_completed: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub jobs_rejected: AtomicU64,
    pub jobs_retried: AtomicU64,

    // Worker counters
    pub workers_registered: AtomicU64,
    pub workers_cleaned_up: AtomicU64,

    // Message counters
    pub messages_received: AtomicU64,
    pub messages_sent: AtomicU64,

    // Error counters
    pub protocol_errors: AtomicU64,
    pub db_errors: AtomicU64,

    // Timing (cumulative microseconds for averaging)
    pub dispatch_time_us: AtomicU64,
    pub conclude_time_us: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            jobs_dispatched: AtomicU64::new(0),
            jobs_completed: AtomicU64::new(0),
            jobs_failed: AtomicU64::new(0),
            jobs_rejected: AtomicU64::new(0),
            jobs_retried: AtomicU64::new(0),
            workers_registered: AtomicU64::new(0),
            workers_cleaned_up: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            protocol_errors: AtomicU64::new(0),
            db_errors: AtomicU64::new(0),
            dispatch_time_us: AtomicU64::new(0),
            conclude_time_us: AtomicU64::new(0),
        }
    }

    /// Increment a counter atomically
    #[inline]
    pub fn inc_jobs_dispatched(&self) {
        self.jobs_dispatched.fetch_add(1, Ordering::Relaxed);
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
    pub fn inc_jobs_retried(&self) {
        self.jobs_retried.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_workers_registered(&self) {
        self.workers_registered.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_workers_cleaned_up(&self) {
        self.workers_cleaned_up.fetch_add(1, Ordering::Relaxed);
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
    pub fn inc_protocol_errors(&self) {
        self.protocol_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_db_errors(&self) {
        self.db_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record dispatch timing in microseconds
    #[inline]
    pub fn record_dispatch_time(&self, start: Instant) {
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.dispatch_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// Record conclude timing in microseconds
    #[inline]
    pub fn record_conclude_time(&self, start: Instant) {
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.conclude_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            jobs_dispatched: self.jobs_dispatched.load(Ordering::Relaxed),
            jobs_completed: self.jobs_completed.load(Ordering::Relaxed),
            jobs_failed: self.jobs_failed.load(Ordering::Relaxed),
            jobs_rejected: self.jobs_rejected.load(Ordering::Relaxed),
            jobs_retried: self.jobs_retried.load(Ordering::Relaxed),
            workers_registered: self.workers_registered.load(Ordering::Relaxed),
            workers_cleaned_up: self.workers_cleaned_up.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
            db_errors: self.db_errors.load(Ordering::Relaxed),
            dispatch_time_us: self.dispatch_time_us.load(Ordering::Relaxed),
            conclude_time_us: self.conclude_time_us.load(Ordering::Relaxed),
        }
    }

    /// Format as Prometheus exposition format
    pub fn prometheus_format(&self) -> String {
        let s = self.snapshot();
        format!(
            r#"# HELP casparian_jobs_dispatched_total Total jobs dispatched to workers
# TYPE casparian_jobs_dispatched_total counter
casparian_jobs_dispatched_total {}

# HELP casparian_jobs_completed_total Total jobs completed successfully
# TYPE casparian_jobs_completed_total counter
casparian_jobs_completed_total {}

# HELP casparian_jobs_failed_total Total jobs that failed
# TYPE casparian_jobs_failed_total counter
casparian_jobs_failed_total {}

# HELP casparian_jobs_rejected_total Total jobs rejected by workers
# TYPE casparian_jobs_rejected_total counter
casparian_jobs_rejected_total {}

# HELP casparian_jobs_retried_total Total jobs retried with exponential backoff
# TYPE casparian_jobs_retried_total counter
casparian_jobs_retried_total {}

# HELP casparian_workers_registered_total Total workers registered
# TYPE casparian_workers_registered_total counter
casparian_workers_registered_total {}

# HELP casparian_workers_cleaned_up_total Total stale workers cleaned up
# TYPE casparian_workers_cleaned_up_total counter
casparian_workers_cleaned_up_total {}

# HELP casparian_messages_received_total Total ZMQ messages received
# TYPE casparian_messages_received_total counter
casparian_messages_received_total {}

# HELP casparian_messages_sent_total Total ZMQ messages sent
# TYPE casparian_messages_sent_total counter
casparian_messages_sent_total {}

# HELP casparian_protocol_errors_total Total protocol parsing errors
# TYPE casparian_protocol_errors_total counter
casparian_protocol_errors_total {}

# HELP casparian_db_errors_total Total database errors
# TYPE casparian_db_errors_total counter
casparian_db_errors_total {}

# HELP casparian_dispatch_time_microseconds_total Cumulative dispatch time in microseconds
# TYPE casparian_dispatch_time_microseconds_total counter
casparian_dispatch_time_microseconds_total {}

# HELP casparian_conclude_time_microseconds_total Cumulative conclude time in microseconds
# TYPE casparian_conclude_time_microseconds_total counter
casparian_conclude_time_microseconds_total {}
"#,
            s.jobs_dispatched,
            s.jobs_completed,
            s.jobs_failed,
            s.jobs_rejected,
            s.jobs_retried,
            s.workers_registered,
            s.workers_cleaned_up,
            s.messages_received,
            s.messages_sent,
            s.protocol_errors,
            s.db_errors,
            s.dispatch_time_us,
            s.conclude_time_us,
        )
    }
}

/// Immutable snapshot of metrics for reading
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub jobs_dispatched: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_rejected: u64,
    pub jobs_retried: u64,
    pub workers_registered: u64,
    pub workers_cleaned_up: u64,
    pub messages_received: u64,
    pub messages_sent: u64,
    pub protocol_errors: u64,
    pub db_errors: u64,
    pub dispatch_time_us: u64,
    pub conclude_time_us: u64,
}

impl MetricsSnapshot {
    /// Calculate average dispatch time in milliseconds
    pub fn avg_dispatch_time_ms(&self) -> f64 {
        if self.jobs_dispatched == 0 {
            0.0
        } else {
            (self.dispatch_time_us as f64 / self.jobs_dispatched as f64) / 1000.0
        }
    }

    /// Calculate average conclude time in milliseconds
    pub fn avg_conclude_time_ms(&self) -> f64 {
        if self.jobs_completed + self.jobs_failed == 0 {
            0.0
        } else {
            let total_concluded = self.jobs_completed + self.jobs_failed;
            (self.conclude_time_us as f64 / total_concluded as f64) / 1000.0
        }
    }

    /// Format as human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Jobs: {} dispatched, {} completed, {} failed, {} rejected | \
             Workers: {} registered, {} cleaned | \
             Avg dispatch: {:.2}ms, Avg conclude: {:.2}ms",
            self.jobs_dispatched,
            self.jobs_completed,
            self.jobs_failed,
            self.jobs_rejected,
            self.workers_registered,
            self.workers_cleaned_up,
            self.avg_dispatch_time_ms(),
            self.avg_conclude_time_ms(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_increment() {
        let metrics = Metrics::new();
        metrics.inc_jobs_dispatched();
        metrics.inc_jobs_dispatched();
        metrics.inc_jobs_completed();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.jobs_dispatched, 2);
        assert_eq!(snapshot.jobs_completed, 1);
    }

    #[test]
    fn test_metrics_timing() {
        let metrics = Metrics::new();
        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        metrics.record_dispatch_time(start);

        let snapshot = metrics.snapshot();
        assert!(snapshot.dispatch_time_us > 0);
    }

    #[test]
    fn test_prometheus_format() {
        let metrics = Metrics::new();
        metrics.inc_jobs_completed();
        let output = metrics.prometheus_format();
        assert!(output.contains("casparian_jobs_completed_total 1"));
    }
}
