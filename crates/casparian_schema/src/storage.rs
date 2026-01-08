//! Schema Contract Storage
//!
//! SQLite-backed persistence for schema contracts using sqlx.

use crate::{LockedSchema, SchemaContract};
use chrono::{DateTime, Utc};
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur in schema storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Contract not found: {0}")]
    NotFound(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

/// SQLite-backed storage for schema contracts.
pub struct SchemaStorage {
    pool: Pool<Sqlite>,
}

impl SchemaStorage {
    /// Create a new SchemaStorage with the given pool.
    pub async fn new(pool: Pool<Sqlite>) -> Result<Self, StorageError> {
        let storage = Self { pool };
        storage.init_tables().await?;
        Ok(storage)
    }

    /// Open a SchemaStorage from a file path.
    pub async fn open(path: &str) -> Result<Self, StorageError> {
        let db_url = format!("sqlite:{}?mode=rwc", path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        Self::new(pool).await
    }

    /// Create an in-memory SchemaStorage (for testing).
    pub async fn in_memory() -> Result<Self, StorageError> {
        let pool = SqlitePoolOptions::new()
            .connect(":memory:")
            .await?;
        Self::new(pool).await
    }

    /// Initialize the database tables.
    async fn init_tables(&self) -> Result<(), StorageError> {
        // Schema contracts table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_contracts (
                contract_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                scope_description TEXT,
                approved_at TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                schemas_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(scope_id, version)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_schema_contracts_scope
                ON schema_contracts(scope_id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Schema discovery results (proposed schemas before approval)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_discovery_results (
                discovery_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                discovered_at TEXT NOT NULL,
                source_file TEXT,
                proposed_schemas_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                CHECK(status IN ('pending', 'approved', 'rejected'))
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_schema_discovery_scope
                ON schema_discovery_results(scope_id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Save a schema contract to the database.
    pub async fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError> {
        let schemas_json = serde_json::to_string(&contract.schemas)?;

        sqlx::query(
            r#"
            INSERT INTO schema_contracts
                (contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(contract_id) DO UPDATE SET
                scope_description = excluded.scope_description,
                approved_at = excluded.approved_at,
                approved_by = excluded.approved_by,
                version = excluded.version,
                schemas_json = excluded.schemas_json
            "#,
        )
        .bind(contract.contract_id.to_string())
        .bind(&contract.scope_id)
        .bind(&contract.scope_description)
        .bind(contract.approved_at.to_rfc3339())
        .bind(&contract.approved_by)
        .bind(contract.version as i64)
        .bind(&schemas_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a contract by its ID.
    pub async fn get_contract(&self, contract_id: &Uuid) -> Result<Option<SchemaContract>, StorageError> {
        let row: Option<ContractRow> = sqlx::query_as(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE contract_id = ?1
            "#,
        )
        .bind(contract_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_contract()).transpose()
    }

    /// Get the latest contract for a scope.
    pub async fn get_contract_for_scope(&self, scope_id: &str) -> Result<Option<SchemaContract>, StorageError> {
        let row: Option<ContractRow> = sqlx::query_as(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE scope_id = ?1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.into_contract()).transpose()
    }

    /// Get all contract versions for a scope.
    pub async fn get_contract_history(&self, scope_id: &str) -> Result<Vec<SchemaContract>, StorageError> {
        let rows: Vec<ContractRow> = sqlx::query_as(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE scope_id = ?1
            ORDER BY version DESC
            "#,
        )
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| r.into_contract())
            .collect()
    }

    /// Delete a contract by its ID.
    pub async fn delete_contract(&self, contract_id: &Uuid) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM schema_contracts WHERE contract_id = ?1")
            .bind(contract_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// List all contracts, optionally limited.
    pub async fn list_contracts(&self, limit: Option<usize>) -> Result<Vec<SchemaContract>, StorageError> {
        let rows: Vec<ContractRow> = match limit {
            Some(n) => {
                sqlx::query_as(
                    r#"
                    SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
                    FROM schema_contracts
                    ORDER BY approved_at DESC
                    LIMIT ?1
                    "#,
                )
                .bind(n as i64)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
                    FROM schema_contracts
                    ORDER BY approved_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter()
            .map(|r| r.into_contract())
            .collect()
    }

    // === Discovery Results ===

    /// Save a schema discovery result (proposed schema before approval).
    pub async fn save_discovery_result(
        &self,
        scope_id: &str,
        source_file: Option<&str>,
        proposed_schemas: &[LockedSchema],
    ) -> Result<String, StorageError> {
        let discovery_id = Uuid::new_v4().to_string();
        let schemas_json = serde_json::to_string(proposed_schemas)?;

        sqlx::query(
            r#"
            INSERT INTO schema_discovery_results
                (discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status)
            VALUES (?1, ?2, datetime('now'), ?3, ?4, 'pending')
            "#,
        )
        .bind(&discovery_id)
        .bind(scope_id)
        .bind(source_file)
        .bind(&schemas_json)
        .execute(&self.pool)
        .await?;

        Ok(discovery_id)
    }

    /// Get pending discovery results for a scope.
    pub async fn get_pending_discoveries(&self, scope_id: &str) -> Result<Vec<DiscoveryResult>, StorageError> {
        let rows: Vec<DiscoveryRow> = sqlx::query_as(
            r#"
            SELECT discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status
            FROM schema_discovery_results
            WHERE scope_id = ?1 AND status = 'pending'
            ORDER BY discovered_at DESC
            "#,
        )
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Approve a discovery result and create a contract.
    pub async fn approve_discovery(
        &self,
        discovery_id: &str,
        approved_by: &str,
    ) -> Result<SchemaContract, StorageError> {
        // Get the discovery result
        let (scope_id, schemas_json): (String, String) = sqlx::query_as(
            r#"
            SELECT scope_id, proposed_schemas_json
            FROM schema_discovery_results
            WHERE discovery_id = ?1 AND status = 'pending'
            "#,
        )
        .bind(discovery_id)
        .fetch_one(&self.pool)
        .await?;

        let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)?;

        // Get next version for this scope
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM schema_contracts WHERE scope_id = ?1",
        )
        .bind(&scope_id)
        .fetch_one(&self.pool)
        .await?;

        // Create the contract
        let mut contract = SchemaContract::with_schemas(scope_id, schemas, approved_by);
        contract.version = version as u32;

        // Save the contract
        self.save_contract(&contract).await?;

        // Mark discovery as approved
        sqlx::query(
            "UPDATE schema_discovery_results SET status = 'approved' WHERE discovery_id = ?1",
        )
        .bind(discovery_id)
        .execute(&self.pool)
        .await?;

        Ok(contract)
    }

    /// Reject a discovery result.
    pub async fn reject_discovery(&self, discovery_id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE schema_discovery_results SET status = 'rejected' WHERE discovery_id = ?1",
        )
        .bind(discovery_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct ContractRow {
    contract_id: String,
    scope_id: String,
    scope_description: Option<String>,
    approved_at: String,
    approved_by: String,
    version: i64,
    schemas_json: String,
}

impl ContractRow {
    fn into_contract(self) -> Result<SchemaContract, StorageError> {
        let contract_id = Uuid::parse_str(&self.contract_id)
            .map_err(|e| StorageError::Parse(format!("Invalid UUID: {}", e)))?;

        let approved_at = DateTime::parse_from_rfc3339(&self.approved_at)
            .map_err(|e| StorageError::Parse(format!("Invalid timestamp: {}", e)))?
            .with_timezone(&Utc);

        let schemas: Vec<LockedSchema> = serde_json::from_str(&self.schemas_json)?;

        Ok(SchemaContract {
            contract_id,
            scope_id: self.scope_id,
            scope_description: self.scope_description,
            approved_at,
            approved_by: self.approved_by,
            schemas,
            version: self.version as u32,
        })
    }
}

/// Internal row type for discovery results.
#[derive(sqlx::FromRow)]
struct DiscoveryRow {
    discovery_id: String,
    scope_id: String,
    discovered_at: String,
    source_file: Option<String>,
    proposed_schemas_json: String,
    status: String,
}

impl From<DiscoveryRow> for DiscoveryResult {
    fn from(row: DiscoveryRow) -> Self {
        DiscoveryResult {
            discovery_id: row.discovery_id,
            scope_id: row.scope_id,
            discovered_at: row.discovered_at,
            source_file: row.source_file,
            proposed_schemas_json: row.proposed_schemas_json,
            status: row.status,
        }
    }
}

/// A schema discovery result (proposed schema before approval).
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub discovery_id: String,
    pub scope_id: String,
    pub discovered_at: String,
    pub source_file: Option<String>,
    pub proposed_schemas_json: String,
    pub status: String,
}

impl DiscoveryResult {
    /// Parse the proposed schemas from JSON.
    pub fn proposed_schemas(&self) -> Result<Vec<LockedSchema>, serde_json::Error> {
        serde_json::from_str(&self.proposed_schemas_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DataType, LockedColumn};

    fn create_test_schema() -> LockedSchema {
        LockedSchema::new(
            "test_table",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::required("name", DataType::String),
                LockedColumn::optional("value", DataType::Float64),
            ],
        )
    }

    #[tokio::test]
    async fn test_save_and_get_contract() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("parser_abc", schema, "user_123");

        storage.save_contract(&contract).await.unwrap();

        let loaded = storage.get_contract(&contract.contract_id).await.unwrap().unwrap();
        assert_eq!(loaded.scope_id, "parser_abc");
        assert_eq!(loaded.approved_by, "user_123");
        assert_eq!(loaded.schemas.len(), 1);
        assert_eq!(loaded.schemas[0].name, "test_table");
    }

    #[tokio::test]
    async fn test_get_contract_for_scope() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("my_scope", schema, "admin");
        storage.save_contract(&contract).await.unwrap();

        let loaded = storage.get_contract_for_scope("my_scope").await.unwrap().unwrap();
        assert_eq!(loaded.contract_id, contract.contract_id);

        let not_found = storage.get_contract_for_scope("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_contract_versioning() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        // First version
        let schema1 = LockedSchema::new("v1", vec![LockedColumn::required("a", DataType::String)]);
        let mut contract1 = SchemaContract::new("versioned_scope", schema1, "user");
        contract1.version = 1;
        storage.save_contract(&contract1).await.unwrap();

        // Second version
        let schema2 = LockedSchema::new("v2", vec![
            LockedColumn::required("a", DataType::String),
            LockedColumn::required("b", DataType::Int64),
        ]);
        let mut contract2 = SchemaContract::new("versioned_scope", schema2, "user");
        contract2.version = 2;
        storage.save_contract(&contract2).await.unwrap();

        // Get latest should return v2
        let latest = storage.get_contract_for_scope("versioned_scope").await.unwrap().unwrap();
        assert_eq!(latest.version, 2);
        assert_eq!(latest.schemas[0].columns.len(), 2);

        // History should have both
        let history = storage.get_contract_history("versioned_scope").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 2); // Most recent first
        assert_eq!(history[1].version, 1);
    }

    #[tokio::test]
    async fn test_discovery_workflow() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        // Save a discovery result
        let proposed = vec![create_test_schema()];
        let discovery_id = storage
            .save_discovery_result("scope_xyz", Some("data.csv"), &proposed)
            .await
            .unwrap();

        // Get pending discoveries
        let pending = storage.get_pending_discoveries("scope_xyz").await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, "pending");

        // Approve it
        let contract = storage.approve_discovery(&discovery_id, "approver").await.unwrap();
        assert_eq!(contract.scope_id, "scope_xyz");
        assert_eq!(contract.approved_by, "approver");
        assert_eq!(contract.version, 1);

        // Pending should be empty now
        let pending = storage.get_pending_discoveries("scope_xyz").await.unwrap();
        assert_eq!(pending.len(), 0);

        // Contract should exist
        let loaded = storage.get_contract_for_scope("scope_xyz").await.unwrap().unwrap();
        assert_eq!(loaded.contract_id, contract.contract_id);
    }

    #[tokio::test]
    async fn test_delete_contract() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("to_delete", schema, "user");
        storage.save_contract(&contract).await.unwrap();

        assert!(storage.get_contract(&contract.contract_id).await.unwrap().is_some());

        let deleted = storage.delete_contract(&contract.contract_id).await.unwrap();
        assert!(deleted);

        assert!(storage.get_contract(&contract.contract_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_list_contracts() {
        let storage = SchemaStorage::in_memory().await.unwrap();

        for i in 0..5 {
            let schema = LockedSchema::new(format!("schema_{}", i), vec![]);
            let contract = SchemaContract::new(format!("scope_{}", i), schema, "user");
            storage.save_contract(&contract).await.unwrap();
        }

        let all = storage.list_contracts(None).await.unwrap();
        assert_eq!(all.len(), 5);

        let limited = storage.list_contracts(Some(3)).await.unwrap();
        assert_eq!(limited.len(), 3);
    }
}
