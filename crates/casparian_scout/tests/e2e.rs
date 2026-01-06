//! End-to-end tests for Scout
//!
//! Scout is the File Discovery + Tagging layer.
//! These tests verify file scanning and tagging functionality.

use casparian_scout::{Database, Scanner, Source, SourceType, TaggingRule, Tagger, FileStatus};
use filetime::{FileTime, set_file_mtime};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a test environment with temp directories
struct TestEnv {
    /// Temp directory (cleaned up on drop)
    _temp: TempDir,
    /// Source directory for input files
    pub source_dir: PathBuf,
    /// Database path
    pub db_path: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let source_dir = temp.path().join("source");
        let db_path = temp.path().join("scout.db");

        fs::create_dir_all(&source_dir).expect("Failed to create source dir");

        Self {
            _temp: temp,
            source_dir,
            db_path,
        }
    }

    fn write_file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.source_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&path, content).expect("Failed to write file");
        path
    }
}

// ============================================================================
// Scanner Tests
// ============================================================================

#[tokio::test]
async fn test_scanner_discovers_files() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    // Create source
    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Write test files
    env.write_file("data.csv", "a,b,c\n1,2,3");
    env.write_file("data.json", r#"{"key": "value"}"#);
    env.write_file("subdir/nested.csv", "x,y\n1,2");

    // Scan
    let scanner = Scanner::new(db.clone());
    let result = scanner.scan_source(&source).await.unwrap();

    assert_eq!(result.stats.files_new, 3);
    assert_eq!(result.stats.files_discovered, 3);

    // Verify files in database
    let files = db.list_files_by_source("src", 100).await.unwrap();
    assert_eq!(files.len(), 3);

    // All should be pending (untagged)
    assert!(files.iter().all(|f| f.status == FileStatus::Pending));
    assert!(files.iter().all(|f| f.tag.is_none()));
}

#[tokio::test]
async fn test_scanner_detects_changes() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Initial file with explicit old mtime
    let path = env.write_file("data.csv", "a,b\n1,2");
    let old_mtime = FileTime::from_unix_time(1000000, 0);
    set_file_mtime(&path, old_mtime).unwrap();

    let scanner = Scanner::new(db.clone());
    let result1 = scanner.scan_source(&source).await.unwrap();
    assert_eq!(result1.stats.files_new, 1);

    // Modify file with a newer mtime
    fs::write(&path, "a,b,c\n1,2,3").unwrap();
    let new_mtime = FileTime::from_unix_time(2000000, 0);
    set_file_mtime(&path, new_mtime).unwrap();

    // Rescan
    let result2 = scanner.scan_source(&source).await.unwrap();
    assert_eq!(result2.stats.files_new, 0);
    assert_eq!(result2.stats.files_changed, 1);
}

#[tokio::test]
async fn test_scanner_handles_nested_directories() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Create nested structure
    env.write_file("root.csv", "a\n1");
    env.write_file("level1/file.csv", "b\n2");
    env.write_file("level1/level2/file.csv", "c\n3");
    env.write_file("level1/level2/level3/file.csv", "d\n4");

    let scanner = Scanner::new(db.clone());
    let result = scanner.scan_source(&source).await.unwrap();

    assert_eq!(result.stats.files_discovered, 4);
}

// ============================================================================
// Tagger Tests
// ============================================================================

#[tokio::test]
async fn test_tagger_matches_patterns() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Create tagging rules
    let rules = vec![
        TaggingRule {
            id: "r1".to_string(),
            name: "CSV Files".to_string(),
            source_id: "src".to_string(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        },
        TaggingRule {
            id: "r2".to_string(),
            name: "JSON Files".to_string(),
            source_id: "src".to_string(),
            pattern: "*.json".to_string(),
            tag: "json_data".to_string(),
            priority: 10,
            enabled: true,
        },
    ];

    let tagger = Tagger::new(rules).unwrap();

    // Write and scan files
    env.write_file("data.csv", "a,b\n1,2");
    env.write_file("config.json", r#"{"key": "val"}"#);
    env.write_file("readme.txt", "hello");

    let scanner = Scanner::new(db.clone());
    scanner.scan_source(&source).await.unwrap();

    // Get files and check tagging
    let files = db.list_files_by_source("src", 100).await.unwrap();

    for file in &files {
        if let Some(tag) = tagger.get_tag(file) {
            db.tag_file(file.id.unwrap(), tag).await.unwrap();
        }
    }

    // Verify tags were applied
    let csv_files = db.list_files_by_tag("csv_data", 10).await.unwrap();
    assert_eq!(csv_files.len(), 1);
    assert!(csv_files[0].path.ends_with("data.csv"));

    let json_files = db.list_files_by_tag("json_data", 10).await.unwrap();
    assert_eq!(json_files.len(), 1);
    assert!(json_files[0].path.ends_with("config.json"));

    // txt file should remain untagged
    let untagged = db.list_untagged_files("src", 10).await.unwrap();
    assert_eq!(untagged.len(), 1);
    assert!(untagged[0].path.ends_with("readme.txt"));
}

#[test]
fn test_tagger_priority_ordering() {
    let rules = vec![
        TaggingRule {
            id: "r1".to_string(),
            name: "All CSV".to_string(),
            source_id: "src".to_string(),
            pattern: "*.csv".to_string(),
            tag: "generic_csv".to_string(),
            priority: 10,
            enabled: true,
        },
        TaggingRule {
            id: "r2".to_string(),
            name: "Sales CSV".to_string(),
            source_id: "src".to_string(),
            pattern: "sales*.csv".to_string(),
            tag: "sales_data".to_string(),
            priority: 20, // Higher priority
            enabled: true,
        },
    ];

    // Sort by priority descending (as db.list_tagging_rules_for_source does)
    let mut sorted_rules = rules;
    sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

    let tagger = Tagger::new(sorted_rules).unwrap();

    // Create test file
    let file = casparian_scout::ScannedFile::new(
        "src",
        "/data/sales_2024.csv",
        "sales_2024.csv",
        1000,
        12345,
    );

    // Should match higher priority rule first
    let tag = tagger.get_tag(&file);
    assert_eq!(tag, Some("sales_data"));
}

#[test]
fn test_glob_star_patterns() {
    let rules = vec![
        TaggingRule {
            id: "r1".to_string(),
            name: "All Nested CSV".to_string(),
            source_id: "src".to_string(),
            pattern: "**/*.csv".to_string(),
            tag: "nested_csv".to_string(),
            priority: 10,
            enabled: true,
        },
    ];

    let tagger = Tagger::new(rules).unwrap();

    // Test various paths
    let cases = vec![
        ("root.csv", true),
        ("dir/file.csv", true),
        ("a/b/c/deep.csv", true),
        ("file.json", false),
    ];

    for (rel_path, should_match) in cases {
        let file = casparian_scout::ScannedFile::new(
            "src",
            &format!("/data/{}", rel_path),
            rel_path,
            1000,
            12345,
        );
        let has_tag = tagger.get_tag(&file).is_some();
        assert_eq!(has_tag, should_match, "Pattern match failed for {}", rel_path);
    }
}

// ============================================================================
// Database Tests
// ============================================================================

#[tokio::test]
async fn test_file_tagging_workflow() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Write files
    env.write_file("file1.csv", "a\n1");
    env.write_file("file2.csv", "b\n2");
    env.write_file("file3.csv", "c\n3");

    // Scan
    let scanner = Scanner::new(db.clone());
    scanner.scan_source(&source).await.unwrap();

    // All files should be pending
    let files = db.list_files_by_source("src", 100).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().all(|f| f.status == FileStatus::Pending));

    // Tag files
    for file in &files {
        db.tag_file(file.id.unwrap(), "csv_data").await.unwrap();
    }

    // Verify tagged status
    let files = db.list_files_by_source("src", 100).await.unwrap();
    assert!(files.iter().all(|f| f.status == FileStatus::Tagged));
    assert!(files.iter().all(|f| f.tag == Some("csv_data".to_string())));

    // Verify list_tagged_files works
    let tagged = db.list_tagged_files("src", 100).await.unwrap();
    assert_eq!(tagged.len(), 3);
}

#[tokio::test]
async fn test_tag_stats() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Write files
    for i in 0..10 {
        env.write_file(&format!("file{}.csv", i), &format!("data{}", i));
    }

    // Scan
    let scanner = Scanner::new(db.clone());
    scanner.scan_source(&source).await.unwrap();

    // Tag some files
    let files = db.list_files_by_source("src", 100).await.unwrap();
    for (i, file) in files.iter().enumerate() {
        if i < 5 {
            db.tag_file(file.id.unwrap(), "csv_data").await.unwrap();
        } else if i < 7 {
            db.tag_file(file.id.unwrap(), "json_data").await.unwrap();
        }
        // 3 remain untagged
    }

    // Check stats
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_files, 10);
    assert_eq!(stats.files_tagged, 7);
    assert_eq!(stats.files_pending, 3);

    // Check tag stats
    let tag_stats = db.get_tag_stats("src").await.unwrap();
    assert!(tag_stats.iter().any(|(tag, count, _, _)| tag == "csv_data" && *count == 5));
    assert!(tag_stats.iter().any(|(tag, count, _, _)| tag == "json_data" && *count == 2));
    assert!(tag_stats.iter().any(|(tag, count, _, _)| tag == "(untagged)" && *count == 3));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_file_with_no_matching_rule() {
    let env = TestEnv::new();
    let db = Database::open(&env.db_path).await.unwrap();

    let source = Source {
        id: "src".to_string(),
        name: "Test".to_string(),
        source_type: SourceType::Local,
        path: env.source_dir.to_string_lossy().to_string(),
        poll_interval_secs: 60,
        enabled: true,
    };
    db.upsert_source(&source).await.unwrap();

    // Only CSV rule
    let rules = vec![
        TaggingRule {
            id: "r1".to_string(),
            name: "CSV".to_string(),
            source_id: "src".to_string(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        },
    ];
    let tagger = Tagger::new(rules).unwrap();

    // Write various file types
    env.write_file("data.csv", "a\n1");
    env.write_file("data.json", "{}");
    env.write_file("data.xml", "<data/>");
    env.write_file("weird_format_no_extension", "???");

    let scanner = Scanner::new(db.clone());
    scanner.scan_source(&source).await.unwrap();

    // Apply tagging
    let files = db.list_files_by_source("src", 100).await.unwrap();
    for file in &files {
        if let Some(tag) = tagger.get_tag(file) {
            db.tag_file(file.id.unwrap(), tag).await.unwrap();
        }
    }

    // Only CSV should be tagged
    let tagged = db.list_files_by_tag("csv_data", 10).await.unwrap();
    assert_eq!(tagged.len(), 1);

    // 3 files should remain untagged
    let untagged = db.list_untagged_files("src", 10).await.unwrap();
    assert_eq!(untagged.len(), 3);
}
