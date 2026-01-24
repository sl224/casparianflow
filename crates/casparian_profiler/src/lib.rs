//! # Casparian Profiler
//!
//! A simple, feature-gated profiler for TUI frame timing and zone breakdown.
//!
//! ## Design Principles
//!
//! - **Zero cost when off**: Feature-gated. No runtime overhead in release builds.
//! - **Simple data structures**: VecDeque for history, HashMap for zones.
//! - **Self-documenting API**: Doc comments ARE the LLM instrumentation spec.
//! - **Flat zones**: Names like `scanner.walk` imply hierarchy. Build tree at render time.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use casparian_profiler::Profiler;
//!
//! let profiler = Profiler::new(250); // 250ms frame budget
//!
//! // Main loop
//! profiler.begin_frame();
//! {
//!     let _zone = profiler.zone("tui.draw");
//!     // ... render TUI ...
//! }
//! profiler.end_frame();
//!
//! // Query data
//! let avg = profiler.avg_frame_time(10);
//! let utilization = profiler.budget_utilization();
//! ```
//!
//! ## Zone Naming Convention
//!
//! Use `{module}.{operation}[.{suboperation}]` pattern:
//! - `tui.draw` - Total render time
//! - `tui.discover` - Discover screen render
//! - `scanner.walk` - Filesystem traversal
//! - `db.query` - Database queries
//!
//! **Reserved characters**: Zone names MUST NOT contain `:` or `,` (used in TSV export).

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Number of frames to keep in history (30 seconds at 250ms tick rate)
const FRAME_HISTORY: usize = 120;

/// Profiler state. Single instance, NOT thread-safe.
/// Uses RefCell for interior mutability (panics on concurrent borrow - intentional).
///
/// The profiler is designed to be owned by the App struct and accessed via shared
/// reference in draw functions.
pub struct Profiler {
    /// Whether profiling overlay is visible (toggled by F12)
    pub enabled: bool,
    /// Frame budget in milliseconds (default: 250ms for TUI tick rate)
    budget_ms: u64,
    /// Interior mutable state
    inner: RefCell<ProfilerInner>,
}

struct ProfilerInner {
    /// Last N frame times (ring buffer behavior via VecDeque)
    frame_times: VecDeque<FrameRecord>,
    /// Zone timings for CURRENT frame only (cleared each frame)
    zone_times: HashMap<&'static str, ZoneAccum>,
    /// When current frame started
    frame_start: Option<Instant>,
}

/// Record of a single frame
struct FrameRecord {
    total_ms: f64,
    /// Zone timings: (name, milliseconds)
    zones: Vec<(&'static str, f64)>,
}

/// Accumulator for zone timing within a frame
#[derive(Default)]
struct ZoneAccum {
    total_ns: u64,
    calls: u32,
}

impl ZoneAccum {
    fn add(&mut self, elapsed_ns: u64) {
        self.total_ns += elapsed_ns;
        self.calls += 1;
    }
}

/// RAII guard that times a zone. Records elapsed time when dropped.
/// Holds &Profiler (shared ref), enabling nested zones.
///
/// # Example
/// ```rust,ignore
/// let _outer = profiler.zone("tui.draw");
/// let _inner = profiler.zone("tui.discover");  // Nested - works!
/// // both timings recorded when guards drop (LIFO order)
/// ```
pub struct ZoneGuard<'a> {
    profiler: &'a Profiler,
    zone: &'static str,
    start: Instant,
}

impl Drop for ZoneGuard<'_> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos() as u64;
        let mut inner = self.profiler.inner.borrow_mut();
        inner.zone_times.entry(self.zone).or_default().add(elapsed);
    }
}

impl Profiler {
    /// Create new profiler with given frame budget in milliseconds.
    ///
    /// # Arguments
    /// * `budget_ms` - Frame time budget (typically 250ms for TUI)
    ///
    /// # Example
    /// ```rust
    /// use casparian_profiler::Profiler;
    /// let profiler = Profiler::new(250);
    /// ```
    pub fn new(budget_ms: u64) -> Self {
        Self {
            enabled: false,
            budget_ms,
            inner: RefCell::new(ProfilerInner {
                frame_times: VecDeque::with_capacity(FRAME_HISTORY),
                zone_times: HashMap::new(),
                frame_start: None,
            }),
        }
    }

    /// Call at start of frame (before terminal.draw).
    /// Clears zone timings from previous frame and starts the frame timer.
    pub fn begin_frame(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.frame_start = Some(Instant::now());
        inner.zone_times.clear();
    }

    /// Call at end of frame (after event handling).
    /// Records total frame time and zone breakdown.
    pub fn end_frame(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(start) = inner.frame_start.take() {
            let total = start.elapsed();
            let record = FrameRecord {
                total_ms: total.as_secs_f64() * 1000.0,
                zones: inner
                    .zone_times
                    .iter()
                    .map(|(k, v)| (*k, v.total_ns as f64 / 1_000_000.0))
                    .collect(),
            };
            inner.frame_times.push_back(record);
            if inner.frame_times.len() > FRAME_HISTORY {
                inner.frame_times.pop_front();
            }
        }
    }

    /// Time a named zone. Returns guard that records on drop.
    /// Takes &self (shared ref), enabling nested zones.
    ///
    /// # Zone Naming Convention
    /// Use `{module}.{operation}` pattern:
    /// - `tui.draw` - Total render time
    /// - `tui.discover` - Discover screen render
    /// - `scanner.walk` - Filesystem traversal
    ///
    /// **Reserved characters**: Zone names MUST NOT contain `:` or `,`.
    ///
    /// # Panics
    /// Panics if called from multiple threads (RefCell runtime check).
    ///
    /// # Example
    /// ```rust,ignore
    /// let _guard = profiler.zone("scanner.walk");
    /// // ... do work ...
    /// // timing recorded when _guard drops
    /// ```
    pub fn zone(&self, name: &'static str) -> ZoneGuard<'_> {
        ZoneGuard {
            profiler: self,
            zone: name,
            start: Instant::now(),
        }
    }

    /// Get last N frame times for sparkline rendering.
    /// Returns times in reverse chronological order (most recent first).
    pub fn frame_history(&self, count: usize) -> Vec<f64> {
        self.inner
            .borrow()
            .frame_times
            .iter()
            .rev()
            .take(count)
            .map(|r| r.total_ms)
            .collect()
    }

    /// Get last completed frame's zone breakdown, sorted by time descending.
    ///
    /// Returns: Vec of (zone_name, milliseconds, call_count)
    ///
    /// Note: Returns PREVIOUS frame's data, not current (current frame still in progress).
    pub fn last_frame_zones(&self) -> Vec<(&'static str, f64, u32)> {
        let inner = self.inner.borrow();
        if let Some(frame) = inner.frame_times.back() {
            let mut zones: Vec<_> = frame
                .zones
                .iter()
                .map(|(name, ms)| (*name, *ms, 1u32)) // Call count not tracked per-frame
                .collect();
            zones.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
            zones
        } else {
            Vec::new()
        }
    }

    /// Get average frame time over last N frames.
    pub fn avg_frame_time(&self, count: usize) -> f64 {
        let times = self.frame_history(count);
        if times.is_empty() {
            0.0
        } else {
            times.iter().sum::<f64>() / times.len() as f64
        }
    }

    /// Budget utilization (0.0 - 1.0+).
    /// Values > 1.0 indicate frame time exceeds budget.
    pub fn budget_utilization(&self) -> f64 {
        self.inner
            .borrow()
            .frame_times
            .back()
            .map(|r| r.total_ms / self.budget_ms as f64)
            .unwrap_or(0.0)
    }

    /// Get the frame budget in milliseconds.
    pub fn budget_ms(&self) -> u64 {
        self.budget_ms
    }

    /// Get total number of frames recorded.
    pub fn frame_count(&self) -> usize {
        self.inner.borrow().frame_times.len()
    }

    /// Get the last frame's total time in milliseconds.
    pub fn last_frame_time(&self) -> Option<f64> {
        self.inner.borrow().frame_times.back().map(|r| r.total_ms)
    }

    // =========================================================================
    // Testing Integration - Export API
    // =========================================================================

    /// Export last N frames as tab-separated values.
    ///
    /// Format: `frame_number\ttotal_ms\tbudget_pct\tzones_csv`
    ///
    /// Example output:
    /// ```text
    /// 0    205.3    82.1    scanner.walk:120.1,tui.draw:45.2
    /// 1    198.7    79.5    scanner.walk:115.0,tui.draw:42.0
    /// ```
    ///
    /// Suitable for shell script parsing with `cut` and `grep`.
    pub fn export_frames_tsv(&self, count: usize) -> String {
        let inner = self.inner.borrow();
        let mut output = String::new();
        for (i, frame) in inner.frame_times.iter().rev().take(count).enumerate() {
            let zones_csv: String = frame
                .zones
                .iter()
                .map(|(name, ms)| format!("{}:{:.1}", name, ms))
                .collect::<Vec<_>>()
                .join(",");
            let budget_pct = (frame.total_ms / self.budget_ms as f64) * 100.0;
            output.push_str(&format!(
                "{}\t{:.1}\t{:.1}\t{}\n",
                i, frame.total_ms, budget_pct, zones_csv
            ));
        }
        output
    }

    /// Export summary statistics as key=value lines.
    ///
    /// Suitable for grep assertions in shell scripts:
    /// ```bash
    /// MAX_MS=$(grep "max_ms" profile_data.txt | cut -d= -f2)
    /// ```
    ///
    /// Output format:
    /// ```text
    /// frame_count=120
    /// avg_ms=185.3
    /// max_ms=248.1
    /// min_ms=95.2
    /// over_budget_count=3
    /// budget_ms=250
    /// ```
    pub fn export_summary(&self) -> String {
        let inner = self.inner.borrow();
        let frame_count = inner.frame_times.len();
        if frame_count == 0 {
            return "frame_count=0\n".to_string();
        }

        let times: Vec<f64> = inner.frame_times.iter().map(|f| f.total_ms).collect();
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let max = times.iter().cloned().fold(f64::MIN, f64::max);
        let min = times.iter().cloned().fold(f64::MAX, f64::min);
        let over_budget = times.iter().filter(|t| **t > self.budget_ms as f64).count();

        format!(
            "frame_count={}\navg_ms={:.1}\nmax_ms={:.1}\nmin_ms={:.1}\nover_budget_count={}\nbudget_ms={}\n",
            frame_count, avg, max, min, over_budget, self.budget_ms
        )
    }

    /// Export zone breakdown from last frame as key=value lines.
    ///
    /// Output format (sorted by time descending):
    /// ```text
    /// zone.scanner.walk=120.1
    /// zone.tui.draw=45.2
    /// zone.db.query=5.3
    /// ```
    pub fn export_zones(&self) -> String {
        let inner = self.inner.borrow();
        if let Some(frame) = inner.frame_times.back() {
            let mut zones: Vec<_> = frame.zones.iter().collect();
            zones.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
            zones
                .iter()
                .map(|(name, ms)| format!("zone.{}={:.1}\n", name, ms))
                .collect()
        } else {
            String::new()
        }
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new(250) // 250ms default budget for TUI
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_creation() {
        let profiler = Profiler::new(250);
        assert!(!profiler.enabled);
        assert_eq!(profiler.budget_ms(), 250);
        assert_eq!(profiler.frame_count(), 0);
    }

    #[test]
    fn test_frame_lifecycle() {
        let profiler = Profiler::new(250);

        profiler.begin_frame();
        thread::sleep(Duration::from_millis(10));
        profiler.end_frame();

        assert_eq!(profiler.frame_count(), 1);
        assert!(profiler.last_frame_time().unwrap() >= 10.0);
    }

    #[test]
    fn test_zone_timing() {
        let profiler = Profiler::new(250);

        profiler.begin_frame();
        {
            let _zone = profiler.zone("test.zone");
            thread::sleep(Duration::from_millis(5));
        }
        profiler.end_frame();

        let zones = profiler.last_frame_zones();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].0, "test.zone");
        assert!(zones[0].1 >= 5.0);
    }

    #[test]
    fn test_nested_zones() {
        let profiler = Profiler::new(250);

        profiler.begin_frame();
        {
            let _outer = profiler.zone("outer");
            thread::sleep(Duration::from_millis(5));
            {
                let _inner = profiler.zone("inner");
                thread::sleep(Duration::from_millis(5));
            }
        }
        profiler.end_frame();

        let zones = profiler.last_frame_zones();
        assert_eq!(zones.len(), 2);

        let outer = zones.iter().find(|z| z.0 == "outer").unwrap();
        let inner = zones.iter().find(|z| z.0 == "inner").unwrap();

        // Outer should be >= inner since it includes inner's time
        assert!(outer.1 >= inner.1);
    }

    #[test]
    fn test_frame_history() {
        let profiler = Profiler::new(250);

        for _ in 0..5 {
            profiler.begin_frame();
            thread::sleep(Duration::from_millis(1));
            profiler.end_frame();
        }

        let history = profiler.frame_history(3);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_avg_frame_time() {
        let profiler = Profiler::new(250);

        for _ in 0..3 {
            profiler.begin_frame();
            thread::sleep(Duration::from_millis(10));
            profiler.end_frame();
        }

        let avg = profiler.avg_frame_time(3);
        assert!(avg >= 10.0);
    }

    #[test]
    fn test_budget_utilization() {
        let profiler = Profiler::new(100); // 100ms budget

        profiler.begin_frame();
        thread::sleep(Duration::from_millis(50));
        profiler.end_frame();

        let util = profiler.budget_utilization();
        assert!((0.5..=1.0).contains(&util)); // Should be around 50%
    }

    #[test]
    fn test_export_summary() {
        let profiler = Profiler::new(250);

        for _ in 0..3 {
            profiler.begin_frame();
            thread::sleep(Duration::from_millis(10));
            profiler.end_frame();
        }

        let summary = profiler.export_summary();
        assert!(summary.contains("frame_count=3"));
        assert!(summary.contains("avg_ms="));
        assert!(summary.contains("max_ms="));
        assert!(summary.contains("min_ms="));
        assert!(summary.contains("budget_ms=250"));
    }

    #[test]
    fn test_export_frames_tsv() {
        let profiler = Profiler::new(250);

        profiler.begin_frame();
        {
            let _z = profiler.zone("test.zone");
            thread::sleep(Duration::from_millis(5));
        }
        profiler.end_frame();

        let tsv = profiler.export_frames_tsv(1);
        assert!(tsv.contains("test.zone:"));
        // Should have tab-separated values
        assert!(tsv.contains('\t'));
    }

    #[test]
    fn test_export_zones() {
        let profiler = Profiler::new(250);

        profiler.begin_frame();
        {
            let _z = profiler.zone("scanner.walk");
            thread::sleep(Duration::from_millis(5));
        }
        profiler.end_frame();

        let zones = profiler.export_zones();
        assert!(zones.contains("zone.scanner.walk="));
    }

    #[test]
    fn test_empty_profiler() {
        let profiler = Profiler::new(250);

        assert_eq!(profiler.frame_count(), 0);
        assert_eq!(profiler.avg_frame_time(10), 0.0);
        assert_eq!(profiler.budget_utilization(), 0.0);
        assert!(profiler.last_frame_zones().is_empty());
        assert_eq!(profiler.export_summary(), "frame_count=0\n");
    }

    #[test]
    fn test_ring_buffer_limit() {
        let profiler = Profiler::new(250);

        // Add more than FRAME_HISTORY frames
        for _ in 0..150 {
            profiler.begin_frame();
            profiler.end_frame();
        }

        assert_eq!(profiler.frame_count(), FRAME_HISTORY);
    }
}
