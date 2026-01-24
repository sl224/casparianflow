# Profiler Specification

**Status:** Draft
**Version:** 1.2
**Parent:** spec.md (Section: Developer Tools)

---

## 1. Problem Statement

When developing Casparian Flow, we need visibility into:
1. **Frame time** - Is the TUI responsive? Are we hitting our 250ms budget?
2. **Operation costs** - Which operation is slow? Scanner? Bridge? Render?
3. **Trends** - Is performance degrading over time?

We do NOT need:
- Nanosecond precision (we have 250ms frames)
- Lock-free data structures (TUI is single-threaded render)
- Complex zone hierarchies (flat zones with naming convention suffice)
- Baseline regression detection (premature - add when needed)

---

## 2. Design Principles

| Principle | Application |
|-----------|-------------|
| **Solve actual problem** | Frame timing + zone breakdown. Nothing more. |
| **Zero cost when off** | Feature-gated. No runtime overhead in release builds. |
| **Simple data structures** | VecDeque for history, HashMap for zones. No custom allocators. |
| **Self-documenting API** | Doc comments ARE the LLM instrumentation spec. |
| **Flat zones** | Names like `scanner.walk` imply hierarchy. Build tree at render time if needed. |

---

## 3. Core API

### 3.1 Profiler State

Uses `RefCell` for interior mutability, enabling nested zone profiling (multiple `ZoneGuard`s can coexist).

```rust
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Profiler state. Single instance, NOT thread-safe.
/// Uses RefCell for interior mutability (panics on concurrent borrow - intentional).
pub struct Profiler {
    /// Whether profiling overlay is visible
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
    zones: Vec<(&'static str, f64)>,  // (name, ms) - copied from zone_times at frame end
}

/// Accumulator for zone timing within a frame
#[derive(Default)]
struct ZoneAccum {
    total_ns: u64,
    calls: u32,
}
```

### 3.2 Ownership Model

The Profiler is **owned by the App struct**:

```rust
// In app.rs
pub struct App {
    // ... existing fields ...

    #[cfg(feature = "profiling")]
    pub profiler: casparian_profiler::Profiler,
}

impl App {
    pub fn new(args: TuiArgs) -> Self {
        Self {
            // ... existing fields ...
            #[cfg(feature = "profiling")]
            profiler: casparian_profiler::Profiler::new(250), // 250ms budget
        }
    }
}
```

**Why App-owned:**
- Clear ownership tied to application lifecycle
- Accessible via `&app.profiler` in draw functions (which take `&App`)
- No global state, no thread-local storage
- Testable (can inject mock profilers)

### 3.3 Frame Lifecycle

```rust
impl Profiler {
    /// Create new profiler with given frame budget
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

    /// Call at start of frame (before terminal.draw)
    pub fn begin_frame(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.frame_start = Some(Instant::now());
        inner.zone_times.clear();
    }

    /// Call at end of frame (after event handling)
    pub fn end_frame(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(start) = inner.frame_start.take() {
            let total = start.elapsed();
            let record = FrameRecord {
                total_ms: total.as_secs_f64() * 1000.0,
                zones: inner.zone_times.iter()
                    .map(|(k, v)| (*k, v.total_ns as f64 / 1_000_000.0))
                    .collect(),
            };
            inner.frame_times.push_back(record);
            if inner.frame_times.len() > FRAME_HISTORY {
                inner.frame_times.pop_front();
            }
        }
    }
}

const FRAME_HISTORY: usize = 120;  // 30 seconds at 250ms tick
```

### 3.4 Zone Timing

```rust
/// RAII guard that times a zone. Records elapsed time when dropped.
/// Holds &Profiler (shared ref), enabling nested zones.
pub struct ZoneGuard<'a> {
    profiler: &'a Profiler,
    zone: &'static str,
    start: Instant,
}

impl Drop for ZoneGuard<'_> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos() as u64;
        let mut inner = self.profiler.inner.borrow_mut();
        inner.zone_times
            .entry(self.zone)
            .or_default()
            .add(elapsed);
    }
}

impl Profiler {
    /// Time a named zone. Returns guard that records on drop.
    /// Takes &self (shared ref), enabling nested zones.
    ///
    /// # Example
    /// ```rust
    /// let _outer = profiler.zone("tui.draw");
    /// let _inner = profiler.zone("tui.discover");  // Nested - works!
    /// // both timings recorded when guards drop (LIFO order)
    /// ```
    ///
    /// # Panics
    /// Panics if called from multiple threads (RefCell runtime check).
    pub fn zone(&self, name: &'static str) -> ZoneGuard<'_> {
        ZoneGuard {
            profiler: self,
            zone: name,
            start: Instant::now(),
        }
    }
}
```

### 3.5 Query API (for overlay rendering)

```rust
impl Profiler {
    /// Get last N frame times for sparkline rendering
    pub fn frame_history(&self, count: usize) -> Vec<f64> {
        self.inner.borrow()
            .frame_times
            .iter()
            .rev()
            .take(count)
            .map(|r| r.total_ms)
            .collect()
    }

    /// Get last completed frame's zone breakdown, sorted by time descending.
    /// Note: Returns PREVIOUS frame's data, not current (current frame still in progress).
    pub fn last_frame_zones(&self) -> Vec<(&'static str, f64, u32)> {
        let inner = self.inner.borrow();
        if let Some(frame) = inner.frame_times.back() {
            let mut zones: Vec<_> = frame.zones.iter()
                .map(|(name, ms)| (*name, *ms, 1u32))  // Call count not tracked per-frame
                .collect();
            zones.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            zones
        } else {
            Vec::new()
        }
    }

    /// Get average frame time over last N frames
    pub fn avg_frame_time(&self, count: usize) -> f64 {
        let times = self.frame_history(count);
        if times.is_empty() { 0.0 } else { times.iter().sum::<f64>() / times.len() as f64 }
    }

    /// Budget utilization (0.0 - 1.0+)
    pub fn budget_utilization(&self) -> f64 {
        self.inner.borrow()
            .frame_times
            .back()
            .map(|r| r.total_ms / self.budget_ms as f64)
            .unwrap_or(0.0)
    }
}
```

---

## 4. Feature Gating

### 4.1 Cargo Feature

```toml
# crates/casparian_profiler/Cargo.toml
[features]
default = []
profiling = []  # Enable profiling. Zero overhead when disabled.
```

### 4.2 Module-Level Gating

The overlay module is conditionally compiled:

```rust
// In crates/casparian/src/cli/tui/mod.rs
#[cfg(feature = "profiling")]
mod profiler_overlay;
```

### 4.3 App Integration

```rust
// In app.rs
pub struct App {
    // ... existing fields ...

    #[cfg(feature = "profiling")]
    pub profiler: casparian_profiler::Profiler,
}
```

### 4.4 Main Loop Integration

```rust
// In mod.rs run_app()
while app.running {
    #[cfg(feature = "profiling")]
    app.profiler.begin_frame();

    terminal.draw(|frame| {
        ui::draw(frame, app);

        #[cfg(feature = "profiling")]
        if app.profiler.enabled {
            profiler_overlay::render(frame, &app.profiler);
        }
    })?;

    match events.next().await {
        Event::Key(key) => app.handle_key(key).await,
        Event::Tick => app.tick().await,
        Event::Resize(_, _) => {}
    }

    #[cfg(feature = "profiling")]
    app.profiler.end_frame();
}
```

### 4.5 Key Handler Integration

```rust
// In app.rs handle_key()
#[cfg(feature = "profiling")]
KeyCode::F12 => {
    self.profiler.enabled = !self.profiler.enabled;
}
```

---

## 5. TUI Overlay

### 5.1 Toggle

- **Key:** F12 (or backtick `` ` `` as fallback for Mac terminals)
- **State:** `profiler.enabled: bool`
- **Render order:** Last (overlays all other content)
- **Note:** Some Mac terminal emulators intercept F12. Use backtick as alternative.

### 5.2 Data Timing

The overlay displays **previous frame's** zone data, not current frame's:
- Current frame is still in progress when overlay renders
- Zone guards haven't dropped yet, so timings aren't recorded
- 250ms latency is acceptable for a development tool
- First frame shows empty zones (no previous frame yet)

### 5.3 Layout

```
┌─ Profiler ─────────────────────────────────────────────────────┐
│ Frame: 1234   Budget: 82% ████████░░   205ms / 250ms           │
├────────────────────────────────────────────────────────────────┤
│ ▁▂▃▄▅▆▇█▇▆▅▄▃▂▁▂▃  avg: 185ms  max: 248ms  min: 95ms          │
├────────────────────────────────────────────────────────────────┤
│ Zone                          │  Time  │ Calls │    %          │
│ scanner.walk                  │ 120ms  │     1 │  48% ████▊    │
│ tui.draw                      │  45ms  │     1 │  18% █▊       │
│   tui.discover                │  38ms  │     1 │  15% █▌       │
│ bridge.execute                │  35ms  │     2 │  14% █▍       │
│ db.query                      │   5ms  │    12 │   2% ▏        │
└────────────────────────────────────────────────────────────────┘
```

### 5.4 Visual Elements

| Element | Widget | Source |
|---------|--------|--------|
| Budget bar | Gauge | `budget_utilization()` |
| Frame sparkline | Sparkline | `frame_history(30)` |
| Zone table | Table | `current_zones()` sorted |
| Zone bars | Inline gauge | Zone ms / frame total |

### 5.5 Hierarchy Display (Render-Time)

Zone names use dot notation: `tui.draw`, `tui.discover`.

At render time, indent zones that share a prefix:
```
tui.draw          →  tui.draw
tui.discover      →    tui.discover  (indented, child of tui)
scanner.walk      →  scanner.walk    (not indented, different root)
```

This is purely visual - no runtime parent/child tracking needed.

---

## 6. Zone Naming Convention

### 6.1 Pattern

```
{module}.{operation}[.{suboperation}]
```

### 6.2 Standard Zones

| Zone | Location | Description |
|------|----------|-------------|
| `tui.draw` | TUI main loop | Total render time |
| `tui.discover` | ui.rs | Discover screen render |
| `tui.jobs` | ui.rs | Jobs screen render |
| `scanner.walk` | scanner.rs | Filesystem traversal |
| `scanner.persist` | scanner.rs | Database persistence |
| `bridge.execute` | bridge.rs | Full bridge execution |
| `bridge.spawn` | bridge.rs | Python process spawn |
| `bridge.ipc` | bridge.rs | IPC data transfer |
| `db.query` | Generic | Database queries |
| `inference.solve` | solver.rs | Type inference |

### 6.3 Reserved Characters

Zone names **MUST NOT** contain:
- `:` (colon) - used as key:value delimiter in TSV export
- `,` (comma) - used as zone separator in TSV export

The `{module}.{operation}` pattern naturally avoids these characters.

### 6.4 Adding New Zones

When profiling a new area, choose a name following the pattern. The doc comments on `Profiler::zone()` serve as the instrumentation guide - no separate spec file needed.

---

## 7. File Structure

```
crates/casparian_profiler/
├── Cargo.toml
├── src/
│   └── lib.rs          # Everything in one file (~250 lines)
│
crates/casparian/src/cli/tui/
├── profiler_overlay.rs  # TUI overlay rendering
├── mod.rs               # Frame timing integration
└── app.rs               # F12 toggle, show_profiler state
```

**Rationale:** One file for the profiler. It's ~250 lines of straightforward code. Splitting into zone.rs, frame.rs, etc. is premature modularization.

---

## 8. Testing Integration

This section defines how the profiler integrates with automated TUI testing for performance regression detection.

### 8.1 Export API

Plain-text export methods for shell script integration. No serde dependency.

```rust
impl Profiler {
    /// Export last N frames as tab-separated values.
    /// Format: frame_number\ttotal_ms\tbudget_pct\tzones_csv
    /// Example: 0\t205.3\t82.1\tscanner.walk:120.1,tui.draw:45.2
    pub fn export_frames_tsv(&self, count: usize) -> String {
        let inner = self.inner.borrow();
        let mut output = String::new();
        for (i, frame) in inner.frame_times.iter().rev().take(count).enumerate() {
            let zones_csv: String = frame.zones.iter()
                .map(|(name, ms)| format!("{}:{:.1}", name, ms))
                .collect::<Vec<_>>()
                .join(",");
            let budget_pct = (frame.total_ms / self.budget_ms as f64) * 100.0;
            output.push_str(&format!("{}\t{:.1}\t{:.1}\t{}\n",
                i, frame.total_ms, budget_pct, zones_csv));
        }
        output
    }

    /// Export summary statistics as key=value lines.
    /// Suitable for grep assertions in shell scripts.
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
    pub fn export_zones(&self) -> String {
        let inner = self.inner.borrow();
        if let Some(frame) = inner.frame_times.back() {
            let mut zones: Vec<_> = frame.zones.iter().collect();
            zones.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            zones.iter()
                .map(|(name, ms)| format!("zone.{}={:.1}\n", name, ms))
                .collect()
        } else {
            String::new()
        }
    }
}
```

### 8.2 File-Based Dump Trigger

On-demand profiler data capture via filesystem trigger. No new dependencies.

```rust
const DUMP_TRIGGER: &str = "/tmp/casparian_profile_dump";
const DUMP_OUTPUT: &str = "/tmp/casparian_profile_data.txt";

// In main loop tick() or after end_frame():
#[cfg(feature = "profiling")]
if std::path::Path::new(DUMP_TRIGGER).exists() {
    let _ = std::fs::remove_file(DUMP_TRIGGER);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let data = format!(
        "=== PROFILER DUMP ===\ntimestamp={}\n{}\n=== ZONES ===\n{}\n=== FRAMES ===\n{}\n",
        timestamp,
        self.profiler.export_summary(),
        self.profiler.export_zones(),
        self.profiler.export_frames_tsv(30)
    );
    let _ = std::fs::write(DUMP_OUTPUT, data);
}
```

**Usage from shell:**

```bash
# Cleanup stale data
rm -f /tmp/casparian_profile_dump /tmp/casparian_profile_data.txt

# Trigger dump with polling (handles TUI under load)
touch /tmp/casparian_profile_dump
for i in {1..20}; do
    sleep 0.25
    if [ -f /tmp/casparian_profile_data.txt ]; then
        break
    fi
done

# Read results
cat /tmp/casparian_profile_data.txt
```

### 8.3 Performance Thresholds

| Threshold | Value | Action |
|-----------|-------|--------|
| OK | < 150ms | Silent pass |
| WARNING | 150-250ms | Log warning, don't fail |
| FAIL | > 250ms | Fail test |
| CRITICAL | > 500ms | Fail test, dump zone breakdown |

### 8.4 Shell Script Assertion Pattern

```bash
#!/bin/bash
# scripts/tui-perf-check.sh

check_frame_budget() {
    local MAX_MS="$1"
    local MAX_INT=${MAX_MS%.*}  # Truncate decimal for integer comparison

    if [ "$MAX_INT" -gt 500 ]; then
        echo "CRITICAL: Frame time ${MAX_MS}ms - dumping zone breakdown"
        grep "^zone\." /tmp/casparian_profile_data.txt
        return 2
    elif [ "$MAX_INT" -gt 250 ]; then
        echo "FAIL: Frame budget exceeded (${MAX_MS}ms > 250ms)"
        return 1
    elif [ "$MAX_INT" -gt 150 ]; then
        echo "WARNING: Frame time elevated (${MAX_MS}ms) - may regress"
        return 0
    else
        echo "OK: Frame time ${MAX_MS}ms"
        return 0
    fi
}

# Trigger dump with polling
trigger_dump() {
    rm -f /tmp/casparian_profile_data.txt
    touch /tmp/casparian_profile_dump

    for i in {1..20}; do
        sleep 0.25
        if [ -f /tmp/casparian_profile_data.txt ]; then
            return 0
        fi
    done

    echo "ERROR: Profile dump timed out after 5s"
    return 1
}
```

### 8.5 TUI Testing Workflow Integration

Add **Phase 7: Performance Validation** to `specs/meta/tui_testing_workflow.md`:

1. **Enable profiling build:** `cargo build -p casparian --release --features profiling`
2. **Run test scenario** via tmux
3. **Trigger dump:** `touch /tmp/casparian_profile_dump && sleep 0.5`
4. **Validate:** Check `max_ms` against thresholds
5. **Capture on failure:** Copy zone breakdown to test artifacts

---

## 9. Implementation Phases

### Phase 1: Core Profiler (MVP)
- [ ] Create `crates/casparian_profiler/src/lib.rs`
- [ ] Implement `Profiler`, `ZoneGuard`, `FrameRecord`
- [ ] Add to workspace Cargo.toml
- [ ] Verify compiles with and without `profiling` feature

### Phase 2: TUI Integration
- [ ] Add `profiler_overlay.rs` with overlay rendering
- [ ] Integrate `begin_frame()`/`end_frame()` in main loop
- [ ] Add F12 toggle in `app.rs`
- [ ] Test overlay displays correctly

### Phase 3: Initial Instrumentation
- [ ] Add zones to TUI draw functions
- [ ] Add zones to scanner hot paths
- [ ] Verify zone times appear in overlay

### Phase 4: Polish
- [ ] Tune sparkline scaling
- [ ] Add color coding for budget overruns (red when >100%)
- [ ] Test with real workloads

---

## 10. What's NOT in Scope

| Feature | Reason | Add Later When... |
|---------|--------|-------------------|
| Memory tracking | Requires allocator hook or sampling | Actual memory issues arise |
| Multi-threaded zones | TUI is single-threaded | Worker profiling needed |
| Zone hierarchy (runtime) | Name convention suffices | Complex nesting visualization needed |
| Automatic baseline generation | Manual thresholds suffice | CI needs auto-generated baselines |
| Statistical analysis (p95, stddev) | Min/max/avg sufficient | Large sample analysis needed |
| Cross-platform scripts | TUI testing is Unix-only | Windows CI needed |

**Now IN scope (v1.2):**
- Export to file (via dump trigger)
- Basic regression detection (via shell script thresholds)

---

## 11. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2025-01-14 | 1.0 | Initial spec after philosophy review |
| 2025-01-14 | 1.1 | Spec refinement Round 1: Added RefCell for interior mutability (GAP-API-002), specified App ownership model (GAP-API-001), improved feature gating with module-level patterns (GAP-API-003), clarified overlay shows previous frame data, added ZoneAccum Default derive |
| 2026-01-14 | 1.2 | Spec refinement Round 2: Added Section 8 Testing Integration with export API (`export_frames_tsv`, `export_summary`, `export_zones`), file-based dump trigger with timestamp, polling-with-timeout pattern, performance thresholds, shell script assertion patterns. Added reserved characters constraint to zone naming (Section 6.3). Updated scope: export and basic regression detection now IN scope. |
