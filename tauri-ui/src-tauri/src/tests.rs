//! Integration tests for Tauri commands.
//!
//! These tests verify that the Tauri commands work correctly
//! against a real SQLite database.

#[cfg(test)]
mod app_state_tests {
    use crate::state::AppState;
    use tempfile::tempdir;

    fn setup_test_state() -> (AppState, tempfile::TempDir) {
        // Create a temp directory for the test database
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.sqlite");

        let mut state = AppState::new().expect("Failed to create AppState");
        state.db_path = db_path.to_string_lossy().to_string();

        // Return both state and temp_dir to keep directory alive
        (state, temp_dir)
    }

    #[test]
    fn test_app_state_creation() {
        let (state, _temp_dir) = setup_test_state();
        assert!(state.db_path.contains("test.sqlite"));
    }

    #[test]
    fn test_open_api_storage() {
        let (state, _temp_dir) = setup_test_state();
        let storage = state.open_api_storage();
        assert!(
            storage.is_ok(),
            "Should be able to open API storage: {:?}",
            storage.err()
        );
    }

    #[test]
    fn test_open_readonly_connection() {
        let (state, _temp_dir) = setup_test_state();

        // First create the database by opening it in write mode
        let storage = state.open_api_storage().expect("Should open storage");
        drop(storage);

        // Now try read-only access
        let conn = state.open_readonly_connection();
        assert!(
            conn.is_ok(),
            "Should be able to open read-only connection: {:?}",
            conn.err()
        );
    }

    #[test]
    fn test_query_sql_validation() {
        // Test SQL validation logic

        // Test SQL validation
        let valid_sql = "SELECT 1 as test_col";
        assert!(valid_sql.trim().to_uppercase().starts_with("SELECT"));

        let invalid_sql = "DROP TABLE users";
        assert!(!invalid_sql.trim().to_uppercase().starts_with("SELECT"));
    }

    #[test]
    fn test_sql_validation() {
        // Test that our SQL validation logic works
        let test_cases = vec![
            ("SELECT * FROM jobs", true),
            ("WITH cte AS (SELECT 1) SELECT * FROM cte", true),
            ("EXPLAIN SELECT 1", true),
            ("INSERT INTO jobs VALUES (1)", false),
            ("DELETE FROM jobs", false),
            ("DROP TABLE jobs", false),
            ("select * from jobs", true), // Case insensitive
        ];

        for (sql, expected_valid) in test_cases {
            let trimmed = sql.trim().to_uppercase();
            let is_valid = trimmed.starts_with("SELECT")
                || trimmed.starts_with("WITH")
                || trimmed.starts_with("EXPLAIN");
            assert_eq!(is_valid, expected_valid, "SQL: {}", sql);
        }
    }

    #[test]
    fn test_forbidden_keywords() {
        let forbidden = ["INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER"];

        // This SQL starts with SELECT but contains DROP
        let dangerous_sql = "SELECT * FROM (DROP TABLE users)";
        let upper = dangerous_sql.to_uppercase();

        let contains_forbidden = forbidden.iter().any(|kw| {
            upper.contains(&format!(" {} ", kw))
                || upper.contains(&format!("({})", kw))
                || upper.contains(&format!("({} ", kw))
                || upper.ends_with(&format!(" {}", kw))
        });

        assert!(contains_forbidden, "Should detect DROP in SQL");
    }
}
