//! Integration tests for Docker test infrastructure.
//!
//! These tests require Docker to be running and the containers to be started.
//!
//! Run with:
//!   docker compose -f crates/casparian_test_utils/docker/docker-compose.yml up -d
//!   cargo test -p casparian_test_utils --features docker-tests

#![cfg(feature = "docker-tests")]

use arrow::array::{Int32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use casparian_test_utils::{DbVersion, PostgresTestGuard, TestPgPool};
use std::sync::Arc;

/// Test that we can connect to PostgreSQL 16 and run a simple query.
#[tokio::test]
async fn test_postgres16_connection() {
    let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();
    pool.execute("SELECT 1").await.unwrap();
}

/// Test that PostgresTestGuard creates an isolated schema.
#[tokio::test]
async fn test_postgres_test_guard_isolation() {
    let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();

    // Create two guards - they should have different schemas
    let guard1 = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();
    let guard2 = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();

    assert_ne!(guard1.schema_name(), guard2.schema_name());

    // Create tables in each schema
    guard1
        .execute("CREATE TABLE test_table (id INT)")
        .await
        .unwrap();
    guard2
        .execute("CREATE TABLE test_table (id INT)")
        .await
        .unwrap();

    // Insert different data
    guard1
        .execute("INSERT INTO test_table VALUES (1)")
        .await
        .unwrap();
    guard2
        .execute("INSERT INTO test_table VALUES (2)")
        .await
        .unwrap();

    // Verify isolation - each should only see its own data
    use sqlx::Row;
    let rows1 = guard1.fetch_all("SELECT id FROM test_table").await.unwrap();
    let rows2 = guard2.fetch_all("SELECT id FROM test_table").await.unwrap();

    assert_eq!(rows1.len(), 1);
    assert_eq!(rows2.len(), 1);
    assert_eq!(rows1[0].get::<i32, _>("id"), 1);
    assert_eq!(rows2[0].get::<i32, _>("id"), 2);
}

/// Test writing Arrow data to PostgreSQL via the sink.
#[tokio::test]
async fn test_postgres_sink_write() {
    use casparian_test_utils::sinks::postgres_sink::PostgresSink;

    let pool = TestPgPool::new(DbVersion::Postgres16).await.unwrap();
    let guard = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();

    // Create a sink
    let sink = PostgresSink::new(pool.pool.clone(), guard.schema_name(), "output");

    // Create Arrow schema and batch
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("name", DataType::Utf8, true),
    ]);

    sink.create_table(&schema).await.unwrap();

    // Create a record batch
    let id_array = Int32Array::from(vec![1, 2, 3]);
    let name_array = StringArray::from(vec![Some("Alice"), Some("Bob"), None]);

    let batch = RecordBatch::try_new(
        Arc::new(schema.clone()),
        vec![Arc::new(id_array), Arc::new(name_array)],
    )
    .unwrap();

    // Write the batch
    sink.write_batch(&batch).await.unwrap();

    // Verify the data
    use sqlx::Row;
    let rows = guard
        .fetch_all(&format!(
            "SELECT id, name FROM {}.output ORDER BY id",
            guard.schema_name()
        ))
        .await
        .unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<i32, _>("id"), 1);
    assert_eq!(rows[0].get::<String, _>("name"), "Alice");
    assert_eq!(rows[1].get::<i32, _>("id"), 2);
    assert_eq!(rows[1].get::<String, _>("name"), "Bob");
    assert_eq!(rows[2].get::<i32, _>("id"), 3);
    assert!(rows[2].try_get::<String, _>("name").is_err()); // NULL
}

/// Test PostgreSQL 14 compatibility.
#[tokio::test]
async fn test_postgres14_basic() {
    let pool = TestPgPool::new(DbVersion::Postgres14).await.unwrap();
    let guard = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();

    guard
        .execute("CREATE TABLE test (id SERIAL PRIMARY KEY, value TEXT)")
        .await
        .unwrap();

    guard
        .execute("INSERT INTO test (value) VALUES ('hello')")
        .await
        .unwrap();

    use sqlx::Row;
    let rows = guard.fetch_all("SELECT value FROM test").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("value"), "hello");
}

/// Test PostgreSQL 15 compatibility.
#[tokio::test]
async fn test_postgres15_basic() {
    let pool = TestPgPool::new(DbVersion::Postgres15).await.unwrap();
    let guard = PostgresTestGuard::new(pool.pool.clone()).await.unwrap();

    guard
        .execute("CREATE TABLE test (id SERIAL PRIMARY KEY, value TEXT)")
        .await
        .unwrap();

    guard
        .execute("INSERT INTO test (value) VALUES ('hello')")
        .await
        .unwrap();

    use sqlx::Row;
    let rows = guard.fetch_all("SELECT value FROM test").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("value"), "hello");
}

/// Test config parsing.
#[test]
fn test_config_basics() {
    use casparian_test_utils::config::TestDbConfig;

    let config = TestDbConfig::new(DbVersion::Postgres16);
    assert_eq!(config.username, "casparian");
    assert_eq!(config.password, "casparian_test");

    let config = TestDbConfig::new(DbVersion::Mssql2022);
    assert_eq!(config.username, "sa");
    assert_eq!(config.password, "Casparian_Test_123!");
}
