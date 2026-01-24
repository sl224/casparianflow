use std::fs;

use casparian::scout::{Database, FileStatus, Scanner, Source, SourceId, SourceType};
use tempfile::TempDir;

/// Workspace smoke test: ensure scans are scoped by workspace.
#[test]
fn test_workspace_scoping_smoke() {
    let db = Database::open_in_memory().expect("open test db");

    let workspace_a = db.create_workspace("Case A").expect("create workspace A");
    let workspace_b = db.create_workspace("Case B").expect("create workspace B");

    let dir_a = TempDir::new().expect("create dir A");
    let dir_b = TempDir::new().expect("create dir B");

    fs::write(dir_a.path().join("alpha.txt"), "alpha").expect("write file A");
    fs::write(dir_b.path().join("bravo.txt"), "bravo").expect("write file B");

    let source_a = Source {
        workspace_id: workspace_a.id,
        id: SourceId::new(),
        name: "Source A".to_string(),
        source_type: SourceType::Local,
        path: dir_a.path().display().to_string(),
        poll_interval_secs: 0,
        enabled: true,
    };

    let source_b = Source {
        workspace_id: workspace_b.id,
        id: SourceId::new(),
        name: "Source B".to_string(),
        source_type: SourceType::Local,
        path: dir_b.path().display().to_string(),
        poll_interval_secs: 0,
        enabled: true,
    };

    db.upsert_source(&source_a).expect("insert source A");
    db.upsert_source(&source_b).expect("insert source B");

    let scanner = Scanner::new(db.clone());
    scanner.scan_source(&source_a).expect("scan workspace A");
    scanner.scan_source(&source_b).expect("scan workspace B");

    let files_a = db
        .list_files_by_status(&workspace_a.id, FileStatus::Pending, 100)
        .expect("list files A");
    let files_b = db
        .list_files_by_status(&workspace_b.id, FileStatus::Pending, 100)
        .expect("list files B");

    assert!(
        files_a.iter().any(|f| f.path.ends_with("alpha.txt")),
        "workspace A should include alpha.txt"
    );
    assert!(
        files_b.iter().any(|f| f.path.ends_with("bravo.txt")),
        "workspace B should include bravo.txt"
    );

    assert!(
        files_a.iter().all(|f| f.workspace_id == workspace_a.id),
        "workspace A listing should only include workspace A files"
    );
    assert!(
        files_b.iter().all(|f| f.workspace_id == workspace_b.id),
        "workspace B listing should only include workspace B files"
    );
}
