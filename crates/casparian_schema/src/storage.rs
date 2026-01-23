//! Schema Contract Storage
//!
//! Database-backed persistence for schema contracts using casparian_db.

use crate::ids::{ContractId, DiscoveryId, SchemaTimestamp};
use crate::{LockedSchema, QuarantineConfig, SchemaContract};
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

    #[error("Schema storage is outdated: {0}")]
    SchemaMismatch(String),
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
    pub fn new(conn: DbConnection) -> Result<Self, StorageError> {
        let storage = Self { conn };
        storage.init_tables()?;
        Ok(storage)
    }

    /// Open a SchemaStorage from a file path (DuckDB).
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let conn = DbConnection::open_duckdb(Path::new(path))?;
        Self::new(conn)
    }

    /// Create an in-memory SchemaStorage (for testing).
    pub fn in_memory() -> Result<Self, StorageError> {
        let conn = DbConnection::open_duckdb_memory()?;
        Self::new(conn)
    }

    /// Initialize the database tables.
    fn init_tables(&self) -> Result<(), StorageError> {
        let status_values = DiscoveryStatus::ALL
            .iter()
            .map(|status| format!("'{}'", status.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let create_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS schema_contracts (
                contract_id TEXT PRIMARY KEY,
                scope_id TEXT NOT NULL,
                scope_description TEXT,
                logic_hash TEXT,
                approved_at TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                schemas_json TEXT NOT NULL,
                quarantine_allow BOOLEAN,
                quarantine_max_pct DOUBLE,
                quarantine_max_count BIGINT,
                quarantine_dir TEXT,
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
                status TEXT NOT NULL DEFAULT '{}',
                CHECK(status IN ({}))
            );

            CREATE INDEX IF NOT EXISTS idx_schema_discovery_scope
                ON schema_discovery_results(scope_id);
        "#,
            DiscoveryStatus::Pending.as_str(),
            status_values
        );

        self.conn.execute_batch(&create_sql)?;
        self.require_columns(
            "schema_contracts",
            &[
                "logic_hash",
                "quarantine_allow",
                "quarantine_max_pct",
                "quarantine_max_count",
                "quarantine_dir",
            ],
        )?;

        Ok(())
    }

    fn require_columns(&self, table: &str, columns: &[&str]) -> Result<(), StorageError> {
        let mut missing = Vec::new();
        for column in columns {
            if !self.column_exists(table, column)? {
                missing.push(*column);
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        Err(StorageError::SchemaMismatch(format!(
            "table '{}' is missing columns: {}. Delete the database and recreate it.",
            table,
            missing.join(", ")
        )))
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool, StorageError> {
        let (query, params) = match self.conn.backend_name() {
            "DuckDB" => (
                "SELECT 1 FROM information_schema.columns WHERE table_name = ? AND column_name = ?"
                    .to_string(),
                vec![DbValue::from(table), DbValue::from(column)],
            ),
            "SQLite" => (
                format!(
                    "SELECT 1 FROM pragma_table_info('{}') WHERE name = ?",
                    table.replace('\'', "''")
                ),
                vec![DbValue::from(column)],
            ),
            _ => (
                "SELECT 1 FROM information_schema.columns WHERE table_name = ? AND column_name = ?"
                    .to_string(),
                vec![DbValue::from(table), DbValue::from(column)],
            ),
        };

        Ok(self.conn.query_optional(&query, &params)?.is_some())
    }

    /// Save a schema contract to the database.
    pub fn save_contract(&self, contract: &SchemaContract) -> Result<(), StorageError> {
        let schemas_json = serde_json::to_string(&contract.schemas)?;
        let (allow_quarantine, max_quarantine_pct, max_quarantine_count, quarantine_dir) =
            if let Some(config) = &contract.quarantine_config {
                let max_count = match config.max_quarantine_count {
                    Some(count) => {
                        Some(
                            i64::try_from(count).map_err(|_| StorageError::Serialization {
                                message: "quarantine max_quarantine_count out of range".to_string(),
                                source: None,
                            })?,
                        )
                    }
                    None => None,
                };
                (
                    Some(config.allow_quarantine),
                    Some(config.max_quarantine_pct),
                    max_count,
                    config.quarantine_dir.clone(),
                )
            } else {
                (None, None, None, None)
            };

        self.conn.execute(
            r#"
                INSERT INTO schema_contracts
                    (
                        contract_id,
                        scope_id,
                        scope_description,
                        logic_hash,
                        approved_at,
                        approved_by,
                        version,
                        schemas_json,
                        quarantine_allow,
                        quarantine_max_pct,
                        quarantine_max_count,
                        quarantine_dir
                    )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(contract_id) DO UPDATE SET
                    scope_description = excluded.scope_description,
                    logic_hash = excluded.logic_hash,
                    approved_at = excluded.approved_at,
                    approved_by = excluded.approved_by,
                    version = excluded.version,
                    schemas_json = excluded.schemas_json,
                    quarantine_allow = excluded.quarantine_allow,
                    quarantine_max_pct = excluded.quarantine_max_pct,
                    quarantine_max_count = excluded.quarantine_max_count,
                    quarantine_dir = excluded.quarantine_dir
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
                DbValue::from(allow_quarantine),
                DbValue::from(max_quarantine_pct),
                DbValue::from(max_quarantine_count),
                DbValue::from(quarantine_dir),
            ],
        )?;

        Ok(())
    }

    /// Get a contract by its ID.
    pub fn get_contract(
        &self,
        contract_id: &ContractId,
    ) -> Result<Option<SchemaContract>, StorageError> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json,
                       quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
                FROM schema_contracts
                WHERE contract_id = ?
                "#,
                &[DbValue::from(contract_id.as_str())],
            )
            ?;

        row.map(row_to_contract).transpose()
    }

    /// Get the latest contract for a scope.
    pub fn get_contract_for_scope(
        &self,
        scope_id: &str,
    ) -> Result<Option<SchemaContract>, StorageError> {
        let row = self
            .conn
            .query_optional(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json,
                       quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
                FROM schema_contracts
                WHERE scope_id = ?
                ORDER BY version DESC
                LIMIT 1
                "#,
                &[DbValue::from(scope_id)],
            )
            ?;

        row.map(row_to_contract).transpose()
    }

    /// Get all contract versions for a scope.
    pub fn get_contract_history(
        &self,
        scope_id: &str,
    ) -> Result<Vec<SchemaContract>, StorageError> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json,
                       quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
                FROM schema_contracts
                WHERE scope_id = ?
                ORDER BY version DESC
                "#,
                &[DbValue::from(scope_id)],
            )
            ?;

        rows.into_iter().map(row_to_contract).collect()
    }

    /// Delete a contract by its ID.
    pub fn delete_contract(&self, contract_id: &ContractId) -> Result<bool, StorageError> {
        let result = self.conn.execute(
            "DELETE FROM schema_contracts WHERE contract_id = ?",
            &[DbValue::from(contract_id.as_str())],
        )?;
        Ok(result > 0)
    }

    /// List all contracts, optionally limited.
    pub fn list_contracts(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<SchemaContract>, StorageError> {
        let rows = match limit {
            Some(n) => {
                self.conn
                    .query_all(
                        r#"
                        SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json,
                               quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
                        FROM schema_contracts
                        ORDER BY approved_at DESC
                        LIMIT ?
                        "#,
                        &[DbValue::from(n as i64)],
                    )
                    ?
            }
            None => {
                self.conn
                    .query_all(
                        r#"
                        SELECT contract_id, scope_id, scope_description, logic_hash, approved_at, approved_by, version, schemas_json,
                               quarantine_allow, quarantine_max_pct, quarantine_max_count, quarantine_dir
                        FROM schema_contracts
                        ORDER BY approved_at DESC
                        "#,
                        &[],
                    )
                    ?
            }
        };

        rows.into_iter().map(row_to_contract).collect()
    }

    // === Discovery Results ===

    /// Save a schema discovery result (proposed schema before approval).
    pub fn save_discovery_result(
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
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
                &[
                    DbValue::from(discovery_id.as_str()),
                    DbValue::from(scope_id),
                    DbValue::from(now.as_str()),
                    DbValue::from(source_file),
                    DbValue::from(schemas_json),
                    DbValue::from(DiscoveryStatus::Pending.as_str()),
                ],
            )
            ?;

        Ok(discovery_id)
    }

    /// Get pending discovery results for a scope.
    pub fn get_pending_discoveries(
        &self,
        scope_id: &str,
    ) -> Result<Vec<DiscoveryResult>, StorageError> {
        let rows = self
            .conn
            .query_all(
                r#"
                SELECT discovery_id, scope_id, discovered_at, source_file, proposed_schemas_json, status
                FROM schema_discovery_results
                WHERE scope_id = ? AND status = ?
                ORDER BY discovered_at DESC
                "#,
                &[
                    DbValue::from(scope_id),
                    DbValue::from(DiscoveryStatus::Pending.as_str()),
                ],
            )
            ?;

        rows.into_iter().map(row_to_discovery).collect()
    }

    /// Reject a discovery result.
    pub fn reject_discovery(&self, discovery_id: &DiscoveryId) -> Result<bool, StorageError> {
        let result = self.conn.execute(
            "UPDATE schema_discovery_results SET status = ? WHERE discovery_id = ?",
            &[
                DbValue::from(DiscoveryStatus::Rejected.as_str()),
                DbValue::from(discovery_id.as_str()),
            ],
        )?;
        Ok(result > 0)
    }
}

fn row_to_contract(row: UnifiedDbRow) -> Result<SchemaContract, StorageError> {
    let contract_id_raw: String = row.get_by_name("contract_id")?;
    let contract_id =
        ContractId::parse(&contract_id_raw).map_err(|e| StorageError::Parse(e.to_string()))?;

    let approved_at_raw: String = row.get_by_name("approved_at")?;
    let approved_at =
        SchemaTimestamp::parse(&approved_at_raw).map_err(|e| StorageError::Parse(e.to_string()))?;

    let schemas_json: String = row.get_by_name("schemas_json")?;
    let schemas: Vec<LockedSchema> = serde_json::from_str(&schemas_json)?;

    let allow_quarantine: Option<bool> = row.get_by_name("quarantine_allow")?;
    let max_quarantine_pct: Option<f64> = row.get_by_name("quarantine_max_pct")?;
    let max_quarantine_count: Option<i64> = row.get_by_name("quarantine_max_count")?;
    let quarantine_dir: Option<String> = row.get_by_name("quarantine_dir")?;

    let mut quarantine_config = QuarantineConfig::default();
    let mut has_quarantine_config = false;
    if let Some(value) = allow_quarantine {
        quarantine_config.allow_quarantine = value;
        has_quarantine_config = true;
    }
    if let Some(value) = max_quarantine_pct {
        quarantine_config.max_quarantine_pct = value;
        has_quarantine_config = true;
    }
    if let Some(value) = max_quarantine_count {
        let count = u64::try_from(value).map_err(|_| {
            StorageError::Parse("quarantine max_quarantine_count out of range".to_string())
        })?;
        quarantine_config.max_quarantine_count = Some(count);
        has_quarantine_config = true;
    }
    if let Some(value) = quarantine_dir {
        quarantine_config.quarantine_dir = Some(value);
        has_quarantine_config = true;
    }

    let version: i64 = row.get_by_name("version")?;

    Ok(SchemaContract {
        contract_id,
        scope_id: row.get_by_name("scope_id")?,
        scope_description: row.get_by_name("scope_description")?,
        logic_hash: row.get_by_name("logic_hash")?,
        approved_at,
        approved_by: row.get_by_name("approved_by")?,
        schemas,
        quarantine_config: if has_quarantine_config {
            Some(quarantine_config)
        } else {
            None
        },
        version: version as u32,
    })
}

fn row_to_discovery(row: UnifiedDbRow) -> Result<DiscoveryResult, StorageError> {
    let discovery_id_raw: String = row.get_by_name("discovery_id")?;
    let discovery_id =
        DiscoveryId::parse(&discovery_id_raw).map_err(|e| StorageError::Parse(e.to_string()))?;

    let discovered_at_raw: String = row.get_by_name("discovered_at")?;
    let discovered_at = SchemaTimestamp::parse(&discovered_at_raw)
        .map_err(|e| StorageError::Parse(e.to_string()))?;

    let status_str: String = row.get_by_name("status")?;
    let status = DiscoveryStatus::parse(&status_str)
        .ok_or_else(|| StorageError::Parse(format!("Invalid discovery status: {}", status_str)))?;

    Ok(DiscoveryResult {
        discovery_id,
        scope_id: row.get_by_name("scope_id")?,
        discovered_at,
        source_file: row.get_by_name("source_file")?,
        proposed_schemas_json: row.get_by_name("proposed_schemas_json")?,
        status,
    })
}

/// A schema discovery result (proposed schema before approval).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryStatus {
    Pending,
    Approved,
    Rejected,
}

impl DiscoveryStatus {
    pub const ALL: &'static [DiscoveryStatus] = &[
        DiscoveryStatus::Pending,
        DiscoveryStatus::Approved,
        DiscoveryStatus::Rejected,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DiscoveryStatus::Pending => "pending",
            DiscoveryStatus::Approved => "approved",
            DiscoveryStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(DiscoveryStatus::Pending),
            "approved" => Some(DiscoveryStatus::Approved),
            "rejected" => Some(DiscoveryStatus::Rejected),
            _ => None,
        }
    }
}

impl std::fmt::Display for DiscoveryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A schema discovery result (proposed schema before approval).
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub discovery_id: DiscoveryId,
    pub scope_id: String,
    pub discovered_at: SchemaTimestamp,
    pub source_file: Option<String>,
    pub proposed_schemas_json: String,
    pub status: DiscoveryStatus,
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

    #[test]
    fn test_save_and_get_contract() {
        let storage = SchemaStorage::in_memory().unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("parser_abc", schema, "user_123")
            .with_logic_hash(Some("logic-1".to_string()));

        storage.save_contract(&contract).unwrap();

        let loaded = storage
            .get_contract(&contract.contract_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.scope_id, "parser_abc");
        assert_eq!(loaded.logic_hash.as_deref(), Some("logic-1"));
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
        let schema2 = LockedSchema::new(
            "v2",
            vec![
                LockedColumn::required("a", DataType::String),
                LockedColumn::required("b", DataType::Int64),
            ],
        );
        let mut contract2 = SchemaContract::new("versioned_scope", schema2, "user");
        contract2.version = 2;
        storage.save_contract(&contract2).unwrap();

        // Get latest should return v2
        let latest = storage
            .get_contract_for_scope("versioned_scope")
            .unwrap()
            .unwrap();
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
        assert_eq!(pending[0].status, DiscoveryStatus::Pending);

        // Reject it
        let rejected = storage.reject_discovery(&discovery_id).unwrap();
        assert!(rejected);

        // Pending should be empty after rejection
        let pending = storage.get_pending_discoveries("scope_xyz").unwrap();
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_delete_contract() {
        let storage = SchemaStorage::in_memory().unwrap();

        let schema = create_test_schema();
        let contract = SchemaContract::new("to_delete", schema, "user");
        storage.save_contract(&contract).unwrap();

        assert!(storage
            .get_contract(&contract.contract_id)
            .unwrap()
            .is_some());

        let deleted = storage.delete_contract(&contract.contract_id).unwrap();
        assert!(deleted);

        assert!(storage
            .get_contract(&contract.contract_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_list_contracts() {
        let storage = SchemaStorage::in_memory().unwrap();

        for i in 0..5 {
            let schema_name = format!("schema_{}", i);
            let scope_id = format!("scope_{}", i);
            let schema = LockedSchema::new(&schema_name, vec![]);
            let contract = SchemaContract::new(&scope_id, schema, "user");
            storage.save_contract(&contract).unwrap();
        }

        let all = storage.list_contracts(None).unwrap();
        assert_eq!(all.len(), 5);

        let limited = storage.list_contracts(Some(3)).unwrap();
        assert_eq!(limited.len(), 3);
    }
}
