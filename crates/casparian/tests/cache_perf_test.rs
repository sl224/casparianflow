//! Direct Performance Test for Cache Building
//!
//! This test directly measures the performance of cache operations
//! without the TUI overhead.

use std::collections::HashMap;
use std::time::Instant;

/// Folder info for hierarchical browsing
#[derive(Debug, Clone)]
struct FolderInfo {
    name: String,
    file_count: usize,
    is_file: bool,
}

/// Test cache building performance with real database
#[tokio::test]
async fn test_cache_build_performance() {
    use sqlx::SqlitePool;

    let db_path = dirs::home_dir()
        .map(|h| h.join(".casparian_flow/casparian_flow.sqlite3"))
        .unwrap();

    if !db_path.exists() {
        println!("Skipping: database not found at {}", db_path.display());
        return;
    }

    println!("\n=== CACHE BUILD PERFORMANCE TEST ===\n");

    // Find the largest source
    let db_url = format!("sqlite:{}?mode=ro", db_path.display());

    let connect_start = Instant::now();
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    println!("[PERF] DB connect: {:?}", connect_start.elapsed());

    // Get source with most files
    let source_query = r#"
        SELECT id, name, (SELECT COUNT(*) FROM scout_files WHERE source_id = s.id) as file_count
        FROM scout_sources s
        WHERE enabled = 1
        ORDER BY file_count DESC
        LIMIT 1
    "#;
    let (source_id, source_name, file_count): (String, String, i64) =
        sqlx::query_as(source_query).fetch_one(&pool).await.unwrap();

    println!("Testing with source '{}' ({} files)", source_name, file_count);

    // Measure query time
    let query_start = Instant::now();
    let paths: Vec<(String,)> = sqlx::query_as(
        "SELECT rel_path FROM scout_files WHERE source_id = ?"
    )
    .bind(&source_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let query_time = query_start.elapsed();
    println!("[PERF] SQL query ({} rows): {:?}", paths.len(), query_time);

    // Measure cache building - OPTIMIZED VERSION
    // Uses nested HashMap for O(1) lookup during build
    let build_start = Instant::now();
    let mut build_cache: HashMap<String, HashMap<String, (usize, bool)>> = HashMap::new();

    for (path,) in &paths {
        let segments: Vec<&str> = path.split('/').collect();
        let mut current_prefix = String::new();

        for (i, segment) in segments.iter().enumerate() {
            if segment.is_empty() {
                continue;
            }
            let is_file = i == segments.len() - 1;
            let level = build_cache.entry(current_prefix.clone()).or_default();

            // O(1) lookup and update
            level.entry(segment.to_string())
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, is_file));

            if !is_file {
                current_prefix = format!("{}{}/", current_prefix, segment);
            }
        }
    }
    let build_time = build_start.elapsed();
    println!("[PERF] Cache build ({} prefixes): {:?}", build_cache.len(), build_time);

    // Convert and sort
    let sort_start = Instant::now();
    let mut cache: HashMap<String, Vec<FolderInfo>> = HashMap::with_capacity(build_cache.len());
    for (prefix, entries) in build_cache {
        let mut folder_vec: Vec<FolderInfo> = entries
            .into_iter()
            .map(|(name, (file_count, is_file))| FolderInfo {
                name,
                file_count,
                is_file,
            })
            .collect();
        folder_vec.sort_by(|a, b| b.file_count.cmp(&a.file_count));
        cache.insert(prefix, folder_vec);
    }
    let sort_time = sort_start.elapsed();
    println!("[PERF] Cache convert+sort: {:?}", sort_time);

    // Measure lookup time
    let lookup_iterations = 10000;
    let lookup_start = Instant::now();
    for _ in 0..lookup_iterations {
        let _ = cache.get("");
        // Get a random deep prefix if available
        if let Some(first_key) = cache.keys().next() {
            let _ = cache.get(first_key);
        }
    }
    let lookup_time = lookup_start.elapsed();
    let per_lookup = lookup_time / (lookup_iterations * 2);
    println!("[PERF] Lookup ({} iterations): {:?} ({:?} per lookup)", lookup_iterations * 2, lookup_time, per_lookup);

    // Summary
    let total_time = query_time + build_time + sort_time;
    println!("\n=== SUMMARY ===");
    println!("Total cache init time: {:?}", total_time);
    println!("  - SQL query: {:?} ({:.1}%)", query_time, 100.0 * query_time.as_secs_f64() / total_time.as_secs_f64());
    println!("  - Cache build: {:?} ({:.1}%)", build_time, 100.0 * build_time.as_secs_f64() / total_time.as_secs_f64());
    println!("  - Sort: {:?} ({:.1}%)", sort_time, 100.0 * sort_time.as_secs_f64() / total_time.as_secs_f64());
    println!("Navigation lookup: {:?}", per_lookup);
    println!("");

    if per_lookup > std::time::Duration::from_micros(100) {
        println!("WARNING: Lookup time >100μs - should investigate");
    } else {
        println!("OK: Lookup time is fast (<100μs)");
    }

    if total_time > std::time::Duration::from_secs(10) {
        println!("WARNING: Total cache init >10s - UI will freeze during load");
        println!("RECOMMENDATION: Make cache loading async/background");
    }

    println!("\n=== TEST COMPLETE ===\n");
}

/// Test navigation path computation
#[test]
fn test_prefix_computation() {
    // Test that prefix computation is correct
    let folder_name = "logs";
    let current_prefix = "";

    let new_prefix = format!("{}{}/", current_prefix, folder_name);
    assert_eq!(new_prefix, "logs/");

    let folder_name = "errors";
    let current_prefix = "logs/";
    let new_prefix = format!("{}{}/", current_prefix, folder_name);
    assert_eq!(new_prefix, "logs/errors/");

    println!("Prefix computation tests passed!");
}

/// Test that cache keys match navigation keys
#[test]
fn test_cache_key_consistency() {
    let mut cache: HashMap<String, Vec<FolderInfo>> = HashMap::new();

    // Simulate building cache for path "logs/errors/crash.log"
    let path = "logs/errors/crash.log";
    let segments: Vec<&str> = path.split('/').collect();
    let mut current_prefix = String::new();

    for (i, segment) in segments.iter().enumerate() {
        let is_file = i == segments.len() - 1;
        let entry = cache.entry(current_prefix.clone()).or_default();
        entry.push(FolderInfo {
            name: segment.to_string(),
            file_count: 1,
            is_file,
        });

        if !is_file {
            current_prefix = format!("{}{}/", current_prefix, segment);
        }
    }

    // Verify cache keys
    assert!(cache.contains_key(""), "Root should exist");
    assert!(cache.contains_key("logs/"), "logs/ should exist");
    assert!(cache.contains_key("logs/errors/"), "logs/errors/ should exist");

    // Simulate navigation
    // User at root, drills into "logs"
    let nav_prefix = format!("{}{}/", "", "logs");
    assert_eq!(nav_prefix, "logs/");
    assert!(cache.contains_key(&nav_prefix), "Navigation key should match cache key");

    // User at "logs/", drills into "errors"
    let nav_prefix = format!("{}{}/", "logs/", "errors");
    assert_eq!(nav_prefix, "logs/errors/");
    assert!(cache.contains_key(&nav_prefix), "Navigation key should match cache key");

    println!("Cache key consistency tests passed!");
}
