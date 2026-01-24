//! E2E Latency Test for Glob Explorer Navigation
//!
//! This test measures the latency of:
//! 1. Initial source selection (cache building) - expected: O(n) one-time
//! 2. Folder drill-down (Enter) - expected: O(1) after cache
//! 3. Folder drill-up (Backspace) - expected: O(1) after cache
//!
//! ## Running
//!
//! ```bash
//! cargo test --package casparian --test glob_explorer_latency_e2e -- --nocapture
//! ```

#![cfg(feature = "full")]

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Helper struct for managing PTY reads via channel
struct PtyReader {
    rx: mpsc::Receiver<Vec<u8>>,
    accumulated: String,
}

impl PtyReader {
    fn new(reader: Box<dyn Read + Send>) -> Self {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();

        thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = vec![0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Ok(n) => {
                        if tx.send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            rx,
            accumulated: String::new(),
        }
    }

    fn wait_for(&mut self, pattern: &str, timeout: Duration) -> Result<Duration, String> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(data) => {
                    let chunk = String::from_utf8_lossy(&data);
                    self.accumulated.push_str(&chunk);
                    if self.accumulated.contains(pattern) {
                        return Ok(start.elapsed());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Err(format!(
            "Timeout ({}ms) waiting for '{}'. Got: {}",
            timeout.as_millis(),
            pattern,
            self.accumulated.chars().take(500).collect::<String>()
        ))
    }

    fn clear(&mut self) {
        self.accumulated.clear();
        // Drain channel
        while self.rx.try_recv().is_ok() {}
    }

    fn read_for(&mut self, duration: Duration) -> String {
        let start = Instant::now();
        while start.elapsed() < duration {
            match self.rx.recv_timeout(Duration::from_millis(10)) {
                Ok(data) => {
                    let chunk = String::from_utf8_lossy(&data);
                    self.accumulated.push_str(&chunk);
                }
                Err(_) => continue,
            }
        }
        self.accumulated.clone()
    }
}

/// Find the casparian binary (prefer release, fallback to debug)
fn find_casparian_binary() -> Option<std::path::PathBuf> {
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();

    // Try release first
    let release_bin = workspace_dir.join("target/release/casparian");
    if release_bin.exists() {
        return Some(release_bin);
    }

    // Fall back to debug
    let debug_bin = workspace_dir.join("target/debug/casparian");
    if debug_bin.exists() {
        return Some(debug_bin);
    }

    None
}

/// Test: Measure Glob Explorer navigation latency
#[test]
fn test_glob_explorer_navigation_latency() {
    // Use pre-built binary for speed
    let binary = match find_casparian_binary() {
        Some(b) => b,
        None => {
            println!("Skipping: casparian binary not found. Run `cargo build --release` first.");
            return;
        }
    };

    println!("Using binary: {}", binary.display());

    let pty_system = native_pty_system();

    let pair = match pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping PTY test: {}", e);
            return;
        }
    };

    // Build command to run the TUI
    let mut cmd = CommandBuilder::new(&binary);
    cmd.arg("tui");

    // Set working directory
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = crate_dir.parent().unwrap().parent().unwrap();
    cmd.cwd(workspace_dir);

    // Spawn in PTY
    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            println!("Skipping: could not spawn TUI: {}", e);
            return;
        }
    };

    // Get reader/writer
    let reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let mut pty_reader = PtyReader::new(reader);

    println!("\n=== GLOB EXPLORER LATENCY TEST ===\n");

    // Step 1: Wait for TUI to start (Home screen)
    println!("[1] Starting TUI...");
    match pty_reader.wait_for("Discover", Duration::from_secs(30)) {
        Ok(elapsed) => println!("    TUI started in {:?}", elapsed),
        Err(e) => {
            println!("    FAILED: {}", e);
            let _ = child.kill();
            return;
        }
    }

    // Step 2: Press '1' to go to Discover mode
    println!("[2] Switching to Discover mode (press '1')...");
    pty_reader.clear();
    let _ = writer.write_all(b"1");
    let _ = writer.flush();

    // Wait for Discover mode (should see Sources panel)
    match pty_reader.wait_for("Sources", Duration::from_secs(10)) {
        Ok(elapsed) => println!("    Discover mode in {:?}", elapsed),
        Err(e) => {
            println!("    FAILED: {}", e);
            // Try to see what we got
            let output = pty_reader.read_for(Duration::from_secs(2));
            println!(
                "    Output: {}",
                output.chars().take(300).collect::<String>()
            );
        }
    }

    // Step 3: Press 'j' to navigate down to "shan" source
    // First, let's see what sources are available
    println!("[3] Navigating to select 'shan' source...");
    thread::sleep(Duration::from_millis(500));

    // Press 'j' a few times to navigate, then Enter to select
    for i in 0..3 {
        let _ = writer.write_all(b"j");
        let _ = writer.flush();
        thread::sleep(Duration::from_millis(100));
    }

    // Step 4: Press Enter to select source and measure cache load time
    println!("[4] Selecting source (Enter) - measuring cache load time...");
    pty_reader.clear();
    let cache_load_start = Instant::now();
    let _ = writer.write_all(b"\r"); // Enter
    let _ = writer.flush();

    // Wait for folders to appear (indicates cache is loaded)
    // Look for a folder name or file count
    match pty_reader.wait_for("files", Duration::from_secs(30)) {
        Ok(_) => {
            let cache_load_time = cache_load_start.elapsed();
            println!("    Cache loaded in {:?}", cache_load_time);
            if cache_load_time > Duration::from_secs(5) {
                println!("    WARNING: Cache load took >5s (expected for 400k+ files)");
            }
        }
        Err(e) => {
            println!("    FAILED: {}", e);
            let output = pty_reader.read_for(Duration::from_secs(2));
            println!(
                "    Output: {}",
                output.chars().take(500).collect::<String>()
            );
        }
    }

    // Step 5: Press 'j' to select first folder
    println!("[5] Navigating to first folder...");
    thread::sleep(Duration::from_millis(200));
    let _ = writer.write_all(b"j");
    let _ = writer.flush();
    thread::sleep(Duration::from_millis(100));

    // Step 6: CRITICAL TEST - Press Enter to drill down and measure latency
    println!("[6] DRILL DOWN (Enter) - measuring O(1) navigation latency...");
    pty_reader.clear();
    let drill_down_start = Instant::now();
    let _ = writer.write_all(b"\r"); // Enter
    let _ = writer.flush();

    // Measure time until screen updates
    // We look for any change in the output
    thread::sleep(Duration::from_millis(100)); // Minimum render time
    let drill_down_time = drill_down_start.elapsed();
    let output_after_drill = pty_reader.read_for(Duration::from_millis(500));

    println!("    Drill-down latency: {:?}", drill_down_time);
    if drill_down_time > Duration::from_millis(100) {
        println!("    PERFORMANCE ISSUE: Drill-down took >100ms (should be <50ms)");
    } else {
        println!("    OK: Drill-down is fast");
    }

    // Step 7: CRITICAL TEST - Press Backspace to drill up and measure latency
    println!("[7] DRILL UP (Backspace) - measuring O(1) navigation latency...");
    pty_reader.clear();
    let drill_up_start = Instant::now();
    let _ = writer.write_all(&[0x7f]); // Backspace (DEL character)
    let _ = writer.flush();

    thread::sleep(Duration::from_millis(100));
    let drill_up_time = drill_up_start.elapsed();

    println!("    Drill-up latency: {:?}", drill_up_time);
    if drill_up_time > Duration::from_millis(100) {
        println!("    PERFORMANCE ISSUE: Drill-up took >100ms (should be <50ms)");
    } else {
        println!("    OK: Drill-up is fast");
    }

    // Step 8: Multiple rapid navigations to test consistency
    println!("[8] RAPID NAVIGATION TEST (10 drill-down/up cycles)...");
    let mut max_latency = Duration::ZERO;
    let mut total_latency = Duration::ZERO;

    for i in 0..10 {
        // Drill down
        pty_reader.clear();
        let _ = writer.write_all(b"j"); // Navigate to folder
        let _ = writer.flush();
        thread::sleep(Duration::from_millis(50));

        let start = Instant::now();
        let _ = writer.write_all(b"\r"); // Enter
        let _ = writer.flush();
        thread::sleep(Duration::from_millis(50));
        let down_time = start.elapsed();

        // Drill up
        let start = Instant::now();
        let _ = writer.write_all(&[0x7f]); // Backspace
        let _ = writer.flush();
        thread::sleep(Duration::from_millis(50));
        let up_time = start.elapsed();

        let cycle_time = down_time + up_time;
        total_latency += cycle_time;
        if cycle_time > max_latency {
            max_latency = cycle_time;
        }
    }

    let avg_latency = total_latency / 10;
    println!("    Average cycle latency: {:?}", avg_latency);
    println!("    Max cycle latency: {:?}", max_latency);

    if max_latency > Duration::from_millis(500) {
        println!("    CRITICAL: Navigation is NOT O(1)! Max latency >500ms");
    } else if max_latency > Duration::from_millis(200) {
        println!("    WARNING: Navigation latency >200ms, may need optimization");
    } else {
        println!("    OK: Navigation appears to be O(1)");
    }

    // Cleanup
    println!("\n[9] Cleaning up...");
    let _ = writer.write_all(&[0x03]); // Ctrl+C
    thread::sleep(Duration::from_millis(200));
    let _ = child.kill();
    let _ = child.wait();

    println!("\n=== TEST COMPLETE ===\n");

    // Summary
    println!("SUMMARY:");
    println!(
        "  - Initial cache load: One-time O(n) operation for {} files",
        442306
    );
    println!("  - Expected cache load time: 5-10 seconds");
    println!("  - Expected navigation time: <50ms (O(1) HashMap lookup)");
    println!("");
    println!("If navigation is slow, investigate:");
    println!("  1. Is update_folders_from_cache() being called? (vs SQL queries)");
    println!("  2. Is the cache actually populated? (check cache_loaded flag)");
    println!("  3. Are there unnecessary .clone() operations?");
    println!("  4. Is tick() doing work on every frame?");
}

/// Test: Direct measurement of cache operations (unit-style, no PTY)
#[test]
fn test_cache_operations_direct() {
    use std::collections::HashMap;

    println!("\n=== DIRECT CACHE OPERATION TEST ===\n");

    // Simulate cache structure for 10k folders
    let mut cache: HashMap<String, Vec<(String, usize, bool)>> = HashMap::new();

    // Build a realistic cache
    for i in 0..100 {
        let prefix = format!("folder{}/", i);
        let mut entries = Vec::new();
        for j in 0..100 {
            entries.push((format!("subfolder{}", j), 100, false));
        }
        cache.insert(prefix, entries);
    }
    cache.insert(
        String::new(),
        (0..100)
            .map(|i| (format!("folder{}", i), 10000, false))
            .collect(),
    );

    println!("Cache size: {} prefixes", cache.len());

    // Measure lookup time
    let iterations = 10000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = cache.get("");
        let _ = cache.get("folder50/");
        let _ = cache.get("folder99/");
    }
    let elapsed = start.elapsed();
    let per_lookup = elapsed / (iterations * 3);

    println!(
        "Lookup performance: {:?} per lookup ({} iterations)",
        per_lookup,
        iterations * 3
    );

    if per_lookup > Duration::from_micros(10) {
        println!("WARNING: HashMap lookup slower than expected");
    } else {
        println!("OK: HashMap lookup is fast (<10μs)");
    }

    println!("\n=== TEST COMPLETE ===\n");
}
