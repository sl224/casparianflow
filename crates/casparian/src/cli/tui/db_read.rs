use super::*;
use casparian_db::{BackendError, DbConnection, DbValue, UnifiedDbRow};
use casparian::scout::{Workspace, WorkspaceId};
use crate::cli::config::DbBackend;
use chrono::Utc;

impl App {
    pub(super) fn open_db_readonly(&self) -> Result<Option<DbConnection>, BackendError> {
        let (backend, path) = self.resolve_db_target();
        Self::open_db_readonly_with(backend, &path)
    }

    pub(super) fn open_db_readonly_with(
        backend: DbBackend,
        path: &std::path::Path,
    ) -> Result<Option<DbConnection>, BackendError> {
        if !path.exists() {
            return Ok(None);
        }

        match backend {
            DbBackend::Sqlite => DbConnection::open_sqlite_readonly(path).map(Some),
            DbBackend::DuckDb => DbConnection::open_duckdb_readonly(path).map(Some),
        }
    }

    pub(super) fn table_exists(conn: &DbConnection, table: &str) -> Result<bool, BackendError> {
        conn.table_exists(table)
    }

    pub(super) fn column_exists(
        conn: &DbConnection,
        table: &str,
        column: &str,
    ) -> Result<bool, BackendError> {
        conn.column_exists(table, column)
    }

    pub(super) fn query_first_workspace(&self) -> Result<Option<Workspace>, String> {
        let workspaces = self.query_workspaces()?;
        Ok(workspaces.into_iter().next())
    }

    pub(super) fn query_workspace_by_id(&self, id: &WorkspaceId) -> Result<Option<Workspace>, String> {
        let conn = match self.open_db_readonly() {
            Ok(Some(conn)) => conn,
            Ok(None) => return Ok(None),
            Err(err) => {
                return Err(format!("Database open failed: {}", err));
            }
        };
        let has_table = App::table_exists(&conn, "cf_workspaces")
            .map_err(|err| format!("Workspace schema check failed: {}", err))?;
        if !has_table {
            return Ok(None);
        }
        let row = conn
            .query_optional(
                "SELECT id, name, created_at FROM cf_workspaces WHERE id = ?",
                &[DbValue::Text(id.to_string())],
            )
            .map_err(|err| format!("Workspace query failed: {}", err))?;
        match row {
            Some(row) => Ok(Some(Self::row_to_workspace(&row)?)),
            None => Ok(None),
        }
    }

    pub(super) fn query_workspaces(&self) -> Result<Vec<Workspace>, String> {
        let conn = match self.open_db_readonly() {
            Ok(Some(conn)) => conn,
            Ok(None) => return Err("Database not found".to_string()),
            Err(err) => {
                return Err(format!("Database open failed: {}", err));
            }
        };
        let has_table = App::table_exists(&conn, "cf_workspaces")
            .map_err(|err| format!("Workspace schema check failed: {}", err))?;
        if !has_table {
            return Err("Workspace registry not initialized".to_string());
        }
        let rows = conn
            .query_all(
                "SELECT id, name, created_at FROM cf_workspaces ORDER BY created_at ASC",
                &[],
            )
            .map_err(|err| format!("Workspace query failed: {}", err))?;
        let mut workspaces = Vec::with_capacity(rows.len());
        for row in rows {
            workspaces.push(Self::row_to_workspace(&row)?);
        }
        Ok(workspaces)
    }

    fn row_to_workspace(row: &UnifiedDbRow) -> Result<Workspace, String> {
        let id_raw: String = row.get(0).map_err(|err| err.to_string())?;
        let id = WorkspaceId::parse(&id_raw).map_err(|err| err.to_string())?;
        let name: String = row.get(1).map_err(|err| err.to_string())?;
        let created_at_ms: i64 = row.get(2).unwrap_or_default();
        let created_at =
            chrono::DateTime::from_timestamp_millis(created_at_ms).unwrap_or_else(Utc::now);
        Ok(Workspace {
            id,
            name,
            created_at,
        })
    }
}
