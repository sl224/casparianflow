//! Schema Contract Storage
//!
//! Database-backed persistence for schema contracts using casparian_db.

use crate::ids::{ContractId, DiscoveryId, SchemaTimestamp};
use crate::{LockedSchema, SchemaContract};
use casparian_db::{BackendError, DbConnection, DbValue, UnifiedDbRow};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur in schema storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] BackendError),

    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Contract not found: {0}")]
    NotFound(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::Serialization {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}

/// Database-backed storage for schema contracts.
pub struct SchemaStorage {
    conn: DbConnection,
}

impl SchemaStorage {
    /// Create a new SchemaStorage with the given connection.
    pub async fn new(conn: DbConnection) -> Result<Self, StorageError> {
        let storage = Self { conn };
        storage.init_tables().await?;
        Ok(storage)
    }

    /// Open a SchemaStorage from a file path (DuckDB).
    pub async fn open(path: &str) -> Result<Self, StorageError> {
        let conn = DbConnection::open_duckdb(Path::new(path)).await?;
        Self::new(conn).await
    }

    /// Create an in-memory SchemaStorage (for testing).
    pub async fn in_memory() -> Result<Self, StorageError> {
        let conn = DbConnection::open_duckdb_memory().await?;
        Self::new(conn).await
    }

    /// Initialize the database tables.
    async fn init_tables(&self) -> Result<(), StorageError> {
        let create_sql = r#"
            CREATE TABLE IF NOT EXISTS schema_contracts (
                contract_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                scope_description TEXT,
                logic_hash TEXT,
                approved_at TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                schemas_json TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(scope_id, version)
            );

            CREATE INDEX IF NOT EXISTS idx_schema_contracts_scope
                ON schema_contracts(scope_id);

            CREATE TABLE IF NOT EXISTS schema_discovery_results (
                discovery_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                discovered_at TEXT NOT NULL,
                source_file TEXT,
                proposed_schemas_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                CHECK(status IN ('pending', 'approved', 'rejected'))
            );

            CREATE INDEX IF NOT EXISTS idx_schema_discovery_scope
                ON schema_discovery_results(scope_id);
        "#;

        self.conn.execute_batch(create_sql).await?;

        self.conn
            .execute(
                "ALTER TABLE schema_contracts ADD COLUMN IF NOT EXISTS logic_hash TEXT",
                &[],
            )
            .await?;

        Ok(())
    }

    /// Save a schema contract to the database.
    pub async fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError> {
        let schemas_json = serde_json::to_string(&contract.schemas)?;

        self.conn
            .execute(
                r#"
                INSERT INTO schema_contracts
                    (contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(contract_id) DO UPDATE SET
                    scope_description = excluded.scope_description,
                    logic_hash = excluded.logic_hash,
                    approved_at = excluded.approved_at,
                    approved_by = excluded.approved_by,
                    version = excluded.version,
                    schemas_json = excluded.schemas_json
                "#,
                &[
                    DbValue::from(contract.contract_id.to_string()),
                    DbValue::from(contract.scope_id.as_str()),
                    DbValue::from(contract.scope_description.clone()),
                    DbValue::from(contract.logic_hash.clone()),
                    DbValue::from(contract.approved_at.as_str()),
                    DbValue::from(contract.approved_by.as_str()),
                    DbValue::from(contract.version as i64),
                    DbValue::from(schemas_json),
                ],
            )
            .await?;

        Ok(())
    }

    /// Get a contract by its ID.
    pub async fn get_contract(&self, contract_id: &ContractId) -> Result<Option<SchemaContract>, StorageError> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json
                FROM schema_contracts
                WHERE contract_id = ?
                "#,
                &[DbValue::from(contract_id.as_str())],
            )
            .await?;

        row.map(row_to_contract).transpose()
    }

    /// Get the latest contract for a scope.
    pub async fn get_contract_for_scope(&self, scope_id: &str) -> Result<Option<SchemaContract>, StorageError> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json
                FROM schema_contracts
                WHERE scope_id = ?
                ORDER BY version DESC
                LIMIT 1
                "#,
                &[DbValue::from(scope_id)],
            )
            .await?;

        row.map(row_to_contract).transpose()
    }

    /// Get all contract versions for a scope.
    pub async fn get_contract_history(&self, scope_id: &str) -> Result<Vec<SchemaContract>, StorageError> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json
                FROM schema_contracts
                WHERE scope_id = ?
                ORDER BY version DESC
                "#,
                &[DbValue::from(scope_id)],
            )
            .await?;

        rows.into_iter()
            .map(row_to_contract)
            .collect()
    }

    /// Delete a contract by its ID.
    pub async fn delete_contract(&self, contract_id: &ContractId) -> Result<bool, StorageError> {
        let result = self
            .conn
            .execute(
                "DELETE FROM schema_contracts WHERE contract_id = ?",
                &[DbValue::from(contract_id.as_str())],
            )
            .await?;
        Ok(result > 0)
    }

    /// List all contracts, optionally limited.
    pub async fn list_contracts(&self, limit: Option<usize>) -> Result<Vec<SchemaContract>, StorageError> {
        let rows = match limit {
            Some(n) => {
                self.conn
                    .query_all(
                        r#"
                        SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json
                        FROM schema_contracts
                        ORDER BY approved_at DESC
                        LIMIT ?
                        "#,
                        &[DbValue::from(n as i64)],
                    )
                    .await?
            }
            None => {
                self.conn
                    .query_all(
                        r#"
                        SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json
                        FROM schema_contracts
                        ORDER BY approved_at DESC
                        "#,
                        &[],
                    )
                    .await?
            }
        };

        rows.into_iter()
            .map(row_to_contract)
            .collect()
    }

    // === Discovery Results ===

    /// Save a schema discovery result (proposed schema before approval).
    pub async fn save_discovery_result(
        &self,
        scope_id: &str,
        source_file: Option<&str>,
        proposed_schemas: &[LockedSchema],
    ) -> Result<DiscoveryId, StorageError> {
        let discovery_id = DiscoveryId::new();
        let schemas_json = serde_json::to_string(proposed_schemas)?;
        let now = SchemaTimestamp::now();

        self.conn
            .execute(
                r#"
                INSERT INTO schema_discovery_results
                    (discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status)
                VALUES (?, ?, ?, ?, ?, 'pending')
                "#,
                &[
                    DbValue::from(discovery_id.as_str()),
                    DbValue::from(scope_id),
                    DbValue::from(now.as_str()),
                    DbValue::from(source_file),
                    DbValue::from(schemas_json),
                ],
            )
            .await?;

        Ok(discovery_id)
    }

    /// Get pending discovery results for a scope.
    pub async fn get_pending_discoveries(&self, scope_id: &str) -> Result<Vec<DiscoveryResult>, StorageError> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status
                FROM schema_discovery_results
                WHERE scope_id = ? AND status = 'pending'
                ORDER BY discovered_at DESC
                "#,
                &[DbValue::from(scope_id)],
            )
            .await?;

        rows.into_iter()
            .map(row_to_discovery)
            .collect()
    }

    /// Approve a discovery result and create a contract.
    #[deprecated(note = "Use approval::approve_schema with SchemaApprovalRequest to derive scope_id and enforce validation")]
    pub async fn approve_discovery(
        &self,
        discovery_id: &DiscoveryId,
        approved_by: &str,
    ) -> Result<SchemaContract, StorageError> {
        // Get the discovery result
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT scope_id, proposed_schemas_json
                FROM schema_discovery_results
                WHERE discovery_id = ? AND status = 'pending'
                "#,
                &[DbValue::from(discovery_id.as_str())],
            )
            .await?;

        let row = row.ok_or_else(|| StorageError::NotFound(discovery_id.to_string()))?;
        let scope_id: String = row.get_by_name("scope_id")?;
        let schemas_json: String = row.get_by_name("proposed_schemas_json")?;

        let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)?;

        // Get next version for this scope
        let version: i64 = self
            .conn
            .query_scalar(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM schema_contracts WHERE scope_id = ?",
                &[DbValue::from(scope_id.as_str())],
            )
            .await?;

        // Create the contract
        let mut contract = SchemaContract::with_schemas(scope_id, schemas, approved_by);
        contract.version = version as u32;

        // Save the contract
        self.save_contract(&contract).await?;

        // Mark discovery as approved
        self.conn
            .execute(
                "UPDATE schema_discovery_results SET status = 'approved' WHERE discovery_id = ?",
                &[DbValue::from(discovery_id.as_str())],
            )
            .await?;

        Ok(contract)
    }

    /// Reject a discovery result.
    pub async fn reject_discovery(&self, discovery_id: &DiscoveryId) -> Result<bool, StorageError> {
        let result = self
            .conn
            .execute(
                "UPDATE schema_discovery_results SET status = 'rejected' WHERE discovery_id = ?",
                &[DbValue::from(discovery_id.as_str())],
            )
            .await?;
        Ok(result > 0)
    }
}

fn row_to_contract(row: UnifiedDbRow) -> Result<SchemaContract, StorageError> {
    let contract_id_raw: String = row.get_by_name("contract_id")?;
    let contract_id = ContractId::parse(&contract_id_raw)
        .map_err(|e| StorageError::Parse(e.to_string()))?;

    let approved_at_raw: String = row.get_by_name("approved_at")?;
    let approved_at = SchemaTimestamp::parse(&approved_at_raw)
        .map_err(|e| StorageError::Parse(e.to_string()))?;

    let schemas_json: String = row.get_by_name("schemas_json")?;
    let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)?;

    let version: i64 = row.get_by_name("version")?;

    Ok(SchemaContract {
        contract_id,
        scope_id: row.get_by_name("scope_id")?,
        scope_description: row.get_by_name("scope_description")?,
        logic_hash: row.get_by_name("logic_hash")?,
        approved_at,
        approved_by: row.get_by_name("approved_by")?,
        schemas,
        version: version as u32,
    })
}

fn row_to_discovery(row: UnifiedDbRow) -> Result<DiscoveryResult, StorageError> {
    let discovery_id_raw: String = row.get_by_name("discovery_id")?;
    let discovery_id = DiscoveryId::parse(&discovery_id_raw)
        .map_err(|e| StorageError::Parse(e.to_string()))?;

    let discovered_at_raw: String = row.get_by_name("discovered_at")?;
    let discovered_at = SchemaTimestamp::parse(&discovered_at_raw)
        .map_err(|e| StorageError::Parse(e.to_string()))?;

    Ok(DiscoveryResult {
        discovery_id,
        scope_id: row.get_by_name("scope_id")?,
        discovered_at,
        source_file: row.get_by_name("source_file")?,
        proposed_schemas_json: row.get_by_name("proposed_schemas_json")?,
        status: row.get_by_name("status")?,
    })
}

/// A schema discovery result (proposed schema before approval).
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub discovery_id: DiscoveryId,
    pub scope_id: String,
    pub discovered_at: SchemaTimestamp,
    pub source_file: Option<String>,
    pub proposed_schemas_json: String,
    pub status: String,
}

impl DiscoveryResult {
    /// Parse the proposed schemas from JSON.
    pub fn proposed_schemas(&self) -> Result<Vec<LockedSchema>, StorageError> {
        Ok(serde_json::from_str(&self.proposed_schemas_json)?)
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
        let contract = SchemaContract::new("parser_abc", schema, "user_123")
            .with_logic_hash(Some("logic-1".to_string()));

        storage.save_contract(&contract).await.unwrap();

        let loaded = storage.get_contract(&contract.contract_id).await.unwrap().unwrap();
        assert_eq!(loaded.scope_id, "parser_abc");
        assert_eq!(loaded.logic_hash.as_deref(), Some("logic-1"));
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

    #[allow(deprecated)]
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
