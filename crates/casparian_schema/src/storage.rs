//! Schema Contract Storage
//!
//! SQLite-backed persistence for schema contracts.

use crate::{LockedSchema, SchemaContract};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur in schema storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Contract not found: {0}")]
    NotFound(String),
}

/// SQLite-backed storage for schema contracts.
pub struct SchemaStorage {
    conn: Connection,
}

impl SchemaStorage {
    /// Create a new SchemaStorage with the given connection.
    pub fn new(conn: Connection) -> Result<Self, StorageError> {
        let storage = Self { conn };
        storage.init_tables()?;
        Ok(storage)
    }

    /// Open a SchemaStorage from a file path.
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Self::new(conn)
    }

    /// Create an in-memory SchemaStorage (for testing).
    pub fn in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Self::new(conn)
    }

    /// Initialize the database tables.
    fn init_tables(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            r#"
            -- Schema contracts table
            CREATE TABLE IF NOT EXISTS schema_contracts (
                contract_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                scope_description TEXT,
                approved_at TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                schemas_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),

                -- Index for scope lookups
                UNIQUE(scope_id, version)
            );

            CREATE INDEX IF NOT EXISTS idx_schema_contracts_scope
                ON schema_contracts(scope_id);

            -- Schema discovery results (proposed schemas before approval)
            CREATE TABLE IF NOT EXISTS schema_discovery_results (
                discovery_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                discovered_at TEXT NOT NULL,
                source_file TEXT,
                proposed_schemas_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',

                -- pending, approved, rejected
                CHECK(status IN ('pending', 'approved', 'rejected'))
            );

            CREATE INDEX IF NOT EXISTS idx_schema_discovery_scope
                ON schema_discovery_results(scope_id);
            "#,
        )?;
        Ok(())
    }

    /// Save a schema contract to the database.
    pub fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError> {
        let schemas_json = serde_json::to_string(&contract.schemas)?;

        self.conn.execute(
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
            params![
                contract.contract_id.to_string(),
                contract.scope_id,
                contract.scope_description,
                contract.approved_at.to_rfc3339(),
                contract.approved_by,
                contract.version,
                schemas_json,
            ],
        )?;

        Ok(())
    }

    /// Get a contract by its ID.
    pub fn get_contract(&self, contract_id: &Uuid) -> Result<Option<SchemaContract>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE contract_id = ?1
            "#,
        )?;

        let result = stmt.query_row(params![contract_id.to_string()], |row| {
            self.row_to_contract(row)
        });

        match result {
            Ok(contract) => Ok(Some(contract)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Database(e)),
        }
    }

    /// Get the latest contract for a scope.
    pub fn get_contract_for_scope(&self, scope_id: &str) -> Result<Option<SchemaContract>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE scope_id = ?1
            ORDER BY version DESC
            LIMIT 1
            "#,
        )?;

        let result = stmt.query_row(params![scope_id], |row| {
            self.row_to_contract(row)
        });

        match result {
            Ok(contract) => Ok(Some(contract)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Database(e)),
        }
    }

    /// Get all contract versions for a scope.
    pub fn get_contract_history(&self, scope_id: &str) -> Result<Vec<SchemaContract>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
            FROM schema_contracts
            WHERE scope_id = ?1
            ORDER BY version DESC
            "#,
        )?;

        let contracts = stmt
            .query_map(params![scope_id], |row| self.row_to_contract(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(contracts)
    }

    /// Delete a contract by its ID.
    pub fn delete_contract(&self, contract_id: &Uuid) -> Result<bool, StorageError> {
        let rows = self.conn.execute(
            "DELETE FROM schema_contracts WHERE contract_id = ?1",
            params![contract_id.to_string()],
        )?;
        Ok(rows > 0)
    }

    /// List all contracts, optionally limited.
    pub fn list_contracts(&self, limit: Option<usize>) -> Result<Vec<SchemaContract>, StorageError> {
        let sql = match limit {
            Some(n) => format!(
                r#"
                SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
                FROM schema_contracts
                ORDER BY approved_at DESC
                LIMIT {}
                "#,
                n
            ),
            None => r#"
                SELECT contract_id, scope_id, scope_description, approved_at, approved_by, version, schemas_json
                FROM schema_contracts
                ORDER BY approved_at DESC
            "#
            .to_string(),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let contracts = stmt
            .query_map([], |row| self.row_to_contract(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(contracts)
    }

    /// Convert a database row to a SchemaContract.
    fn row_to_contract(&self, row: &rusqlite::Row) -> SqlResult<SchemaContract> {
        let contract_id: String = row.get(0)?;
        let scope_id: String = row.get(1)?;
        let scope_description: Option<String> = row.get(2)?;
        let approved_at: String = row.get(3)?;
        let approved_by: String = row.get(4)?;
        let version: u32 = row.get(5)?;
        let schemas_json: String = row.get(6)?;

        let contract_id = Uuid::parse_str(&contract_id)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;

        let approved_at = DateTime::parse_from_rfc3339(&approved_at)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?
            .with_timezone(&Utc);

        let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e)))?;

        Ok(SchemaContract {
            contract_id,
            scope_id,
            scope_description,
            approved_at,
            approved_by,
            schemas,
            version,
        })
    }

    // === Discovery Results ===

    /// Save a schema discovery result (proposed schema before approval).
    pub fn save_discovery_result(
        &self,
        scope_id: &str,
        source_file: Option<&str>,
        proposed_schemas: &[LockedSchema],
    ) -> Result<String, StorageError> {
        let discovery_id = Uuid::new_v4().to_string();
        let schemas_json = serde_json::to_string(proposed_schemas)?;

        self.conn.execute(
            r#"
            INSERT INTO schema_discovery_results
                (discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status)
            VALUES (?1, ?2, datetime('now'), ?3, ?4, 'pending')
            "#,
            params![discovery_id, scope_id, source_file, schemas_json],
        )?;

        Ok(discovery_id)
    }

    /// Get pending discovery results for a scope.
    pub fn get_pending_discoveries(&self, scope_id: &str) -> Result<Vec<DiscoveryResult>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status
            FROM schema_discovery_results
            WHERE scope_id = ?1 AND status = 'pending'
            ORDER BY discovered_at DESC
            "#,
        )?;

        let results = stmt
            .query_map(params![scope_id], |row| {
                let discovery_id: String = row.get(0)?;
                let scope_id: String = row.get(1)?;
                let discovered_at: String = row.get(2)?;
                let source_file: Option<String> = row.get(3)?;
                let schemas_json: String = row.get(4)?;
                let status: String = row.get(5)?;

                Ok(DiscoveryResult {
                    discovery_id,
                    scope_id,
                    discovered_at,
                    source_file,
                    proposed_schemas_json: schemas_json,
                    status,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Approve a discovery result and create a contract.
    pub fn approve_discovery(
        &self,
        discovery_id: &str,
        approved_by: &str,
    ) -> Result<SchemaContract, StorageError> {
        // Get the discovery result
        let mut stmt = self.conn.prepare(
            r#"
            SELECT scope_id, proposed_schemas_json
            FROM schema_discovery_results
            WHERE discovery_id = ?1 AND status = 'pending'
            "#,
        )?;

        let (scope_id, schemas_json): (String, String) = stmt.query_row(params![discovery_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)?;

        // Get next version for this scope
        let version: u32 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM schema_contracts WHERE scope_id = ?1",
            params![scope_id],
            |row| row.get(0),
        )?;

        // Create the contract
        let mut contract = SchemaContract::with_schemas(scope_id, schemas, approved_by);
        contract.version = version;

        // Save the contract
        self.save_contract(&contract)?;

        // Mark discovery as approved
        self.conn.execute(
            "UPDATE schema_discovery_results SET status = 'approved' WHERE discovery_id = ?1",
            params![discovery_id],
        )?;

        Ok(contract)
    }

    /// Reject a discovery result.
    pub fn reject_discovery(&self, discovery_id: &str) -> Result<bool, StorageError> {
        let rows = self.conn.execute(
            "UPDATE schema_discovery_results SET status = 'rejected' WHERE discovery_id = ?1",
            params![discovery_id],
        )?;
        Ok(rows > 0)
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

    #[test]
    fn test_save_and_get_contract() {
        let storage = SchemaStorage::in_memory().unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("parser_abc", schema, "user_123");

        storage.save_contract(&contract).unwrap();

        let loaded = storage.get_contract(&contract.contract_id).unwrap().unwrap();
        assert_eq!(loaded.scope_id, "parser_abc");
        assert_eq!(loaded.approved_by, "user_123");
        assert_eq!(loaded.schemas.len(), 1);
        assert_eq!(loaded.schemas[0].name, "test_table");
    }

    #[test]
    fn test_get_contract_for_scope() {
        let storage = SchemaStorage::in_memory().unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("my_scope", schema, "admin");
        storage.save_contract(&contract).unwrap();

        let loaded = storage.get_contract_for_scope("my_scope").unwrap().unwrap();
        assert_eq!(loaded.contract_id, contract.contract_id);

        let not_found = storage.get_contract_for_scope("nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_contract_versioning() {
        let storage = SchemaStorage::in_memory().unwrap();

        // First version
        let schema1 = LockedSchema::new("v1", vec![LockedColumn::required("a", DataType::String)]);
        let mut contract1 = SchemaContract::new("versioned_scope", schema1, "user");
        contract1.version = 1;
        storage.save_contract(&contract1).unwrap();

        // Second version
        let schema2 = LockedSchema::new("v2", vec![
            LockedColumn::required("a", DataType::String),
            LockedColumn::required("b", DataType::Int64),
        ]);
        let mut contract2 = SchemaContract::new("versioned_scope", schema2, "user");
        contract2.version = 2;
        storage.save_contract(&contract2).unwrap();

        // Get latest should return v2
        let latest = storage.get_contract_for_scope("versioned_scope").unwrap().unwrap();
        assert_eq!(latest.version, 2);
        assert_eq!(latest.schemas[0].columns.len(), 2);

        // History should have both
        let history = storage.get_contract_history("versioned_scope").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 2); // Most recent first
        assert_eq!(history[1].version, 1);
    }

    #[test]
    fn test_discovery_workflow() {
        let storage = SchemaStorage::in_memory().unwrap();

        // Save a discovery result
        let proposed = vec![create_test_schema()];
        let discovery_id = storage
            .save_discovery_result("scope_xyz", Some("data.csv"), &proposed)
            .unwrap();

        // Get pending discoveries
        let pending = storage.get_pending_discoveries("scope_xyz").unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, "pending");

        // Approve it
        let contract = storage.approve_discovery(&discovery_id, "approver").unwrap();
        assert_eq!(contract.scope_id, "scope_xyz");
        assert_eq!(contract.approved_by, "approver");
        assert_eq!(contract.version, 1);

        // Pending should be empty now
        let pending = storage.get_pending_discoveries("scope_xyz").unwrap();
        assert_eq!(pending.len(), 0);

        // Contract should exist
        let loaded = storage.get_contract_for_scope("scope_xyz").unwrap().unwrap();
        assert_eq!(loaded.contract_id, contract.contract_id);
    }

    #[test]
    fn test_delete_contract() {
        let storage = SchemaStorage::in_memory().unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("to_delete", schema, "user");
        storage.save_contract(&contract).unwrap();

        assert!(storage.get_contract(&contract.contract_id).unwrap().is_some());

        let deleted = storage.delete_contract(&contract.contract_id).unwrap();
        assert!(deleted);

        assert!(storage.get_contract(&contract.contract_id).unwrap().is_none());
    }

    #[test]
    fn test_list_contracts() {
        let storage = SchemaStorage::in_memory().unwrap();

        for i in 0..5 {
            let schema = LockedSchema::new(format!("schema_{}", i), vec![]);
            let contract = SchemaContract::new(format!("scope_{}", i), schema, "user");
            storage.save_contract(&contract).unwrap();
        }

        let all = storage.list_contracts(None).unwrap();
        assert_eq!(all.len(), 5);

        let limited = storage.list_contracts(Some(3)).unwrap();
        assert_eq!(limited.len(), 3);
    }
}
