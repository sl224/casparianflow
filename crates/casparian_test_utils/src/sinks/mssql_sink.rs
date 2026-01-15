//! MSSQL sink writer for Arrow data.
//!
//! Requires the `mssql` feature to be enabled.

use crate::pools::mssql::TestMssqlPool;
use anyhow::{bail, Context, Result};
use arrow::array::*;
use arrow::datatypes::{DataType as ArrowDataType, Schema};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use tracing::debug;
use url::Url;

/// MSSQL sink for writing Arrow data to a table.
pub struct MssqlSink {
    pool: TestMssqlPool,
    schema_name: String,
    table_name: String,
}

/// Configuration parsed from a MSSQL sink URI.
#[derive(Debug, Clone)]
pub struct MssqlSinkConfig {
    /// Host
    pub host: String,
    /// Port
    pub port: u16,
    /// Database name
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Schema name (defaults to "dbo")
    pub schema_name: String,
    /// Table name
    pub table_name: String,
}

impl MssqlSinkConfig {
    /// Parse a MSSQL sink URI.
    ///
    /// Format: `mssql://user:pass@host:port/db?schema=dbo&table=output`
    pub fn from_uri(uri: &str) -> Result<Self> {
        let url = Url::parse(uri).context("Invalid MSSQL URI")?;

        if url.scheme() != "mssql" && url.scheme() != "sqlserver" {
            bail!("Expected mssql:// or sqlserver:// URI");
        }

        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Missing host in URI"))?
            .to_string();

        let port = url.port().unwrap_or(1433);

        let database = url.path().trim_start_matches('/').to_string();

        if database.is_empty() {
            bail!("Missing database in URI path");
        }

        let username = url.username().to_string();
        if username.is_empty() {
            bail!("Missing username in URI");
        }

        let password = url
            .password()
            .ok_or_else(|| anyhow::anyhow!("Missing password in URI"))?
            .to_string();

        let schema_name = url
            .query_pairs()
            .find(|(k, _)| k == "schema")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "dbo".to_string());

        let table_name = url
            .query_pairs()
            .find(|(k, _)| k == "table")
            .map(|(_, v)| v.to_string())
            .ok_or_else(|| anyhow::anyhow!("Missing 'table' parameter in URI"))?;

        Ok(Self {
            host,
            port,
            database,
            username,
            password,
            schema_name,
            table_name,
        })
    }
}

impl MssqlSink {
    /// Create a new MSSQL sink from a test pool.
    pub fn new(pool: TestMssqlPool, schema_name: &str, table_name: &str) -> Self {
        Self {
            pool,
            schema_name: schema_name.to_string(),
            table_name: table_name.to_string(),
        }
    }

    /// Create the target table based on Arrow schema.
    ///
    /// Drops the table if it exists.
    pub async fn create_table(&self, schema: &Schema) -> Result<()> {
        let full_name = format!("[{}].[{}]", self.schema_name, self.table_name);

        let mut client = self.pool.get_connection().await?;

        // Drop existing table
        let drop_sql = format!(
            "IF OBJECT_ID('{}', 'U') IS NOT NULL DROP TABLE {}",
            full_name, full_name
        );
        client.execute(&drop_sql, &[]).await?;

        // Build CREATE TABLE statement
        let columns: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| {
                let mssql_type = arrow_to_mssql_type(f.data_type());
                let nullable = if f.is_nullable() { "NULL" } else { "NOT NULL" };
                format!("[{}] {} {}", f.name(), mssql_type, nullable)
            })
            .collect();

        let create_sql = format!("CREATE TABLE {} ({})", full_name, columns.join(", "));
        debug!("Creating table: {}", create_sql);
        client.execute(&create_sql, &[]).await?;

        Ok(())
    }

    /// Write a record batch to the table.
    ///
    /// Uses simple string-based INSERT for compatibility across all types.
    pub async fn write_batch(&self, batch: &RecordBatch) -> Result<()> {
        if batch.num_rows() == 0 {
            return Ok(());
        }

        let full_name = format!("[{}].[{}]", self.schema_name, self.table_name);
        let schema = batch.schema();
        let column_names: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| format!("[{}]", f.name()))
            .collect();

        let mut client = self.pool.get_connection().await?;

        // Insert row by row with inline values (simple approach)
        for row_idx in 0..batch.num_rows() {
            let values: Vec<String> = (0..batch.num_columns())
                .map(|col_idx| format_value(batch.column(col_idx), row_idx))
                .collect::<Result<Vec<_>>>()?;

            let insert_sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                full_name,
                column_names.join(", "),
                values.join(", ")
            );

            client.execute(&insert_sql, &[]).await?;
        }

        debug!("Wrote {} rows to {}", batch.num_rows(), full_name);

        Ok(())
    }

    /// Get the full table name ([schema].[table]).
    pub fn full_table_name(&self) -> String {
        format!("[{}].[{}]", self.schema_name, self.table_name)
    }
}

/// Convert Arrow DataType to MSSQL type.
fn arrow_to_mssql_type(dt: &ArrowDataType) -> &'static str {
    match dt {
        ArrowDataType::Boolean => "BIT",
        ArrowDataType::Int8 | ArrowDataType::UInt8 => "TINYINT",
        ArrowDataType::Int16 | ArrowDataType::UInt16 => "SMALLINT",
        ArrowDataType::Int32 | ArrowDataType::UInt32 => "INT",
        ArrowDataType::Int64 | ArrowDataType::UInt64 => "BIGINT",
        ArrowDataType::Float32 => "REAL",
        ArrowDataType::Float64 => "FLOAT",
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => "NVARCHAR(MAX)",
        ArrowDataType::Binary | ArrowDataType::LargeBinary => "VARBINARY(MAX)",
        ArrowDataType::Date32 | ArrowDataType::Date64 => "DATE",
        ArrowDataType::Timestamp(_, _) => "DATETIME2",
        ArrowDataType::Time32(_) | ArrowDataType::Time64(_) => "TIME",
        ArrowDataType::Decimal128(_, _) | ArrowDataType::Decimal256(_, _) => {
            // Can't return dynamic string, use max precision
            "DECIMAL(38, 10)"
        }
        _ => "NVARCHAR(MAX)", // Fallback
    }
}

/// Format a value from an Arrow array as a SQL literal.
fn format_value(array: &Arc<dyn Array>, row_idx: usize) -> Result<String> {
    if array.is_null(row_idx) {
        return Ok("NULL".to_string());
    }

    let value = match array.data_type() {
        ArrowDataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            if arr.value(row_idx) { "1" } else { "0" }.to_string()
        }
        ArrowDataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            arr.value(row_idx).to_string()
        }
        ArrowDataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            format_sql_string(arr.value(row_idx))
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            format_sql_string(arr.value(row_idx))
        }
        ArrowDataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            format_sql_binary(arr.value(row_idx))
        }
        ArrowDataType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            format_sql_binary(arr.value(row_idx))
        }
        _ => {
            // Fallback: convert to string representation
            let formatter = arrow::util::display::ArrayFormatter::try_new(
                array.as_ref(),
                &arrow::util::display::FormatOptions::default(),
            )?;
            format_sql_string(&formatter.value(row_idx).to_string())
        }
    };

    Ok(value)
}

/// Format a string for SQL, escaping single quotes.
fn format_sql_string(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

/// Format binary data as MSSQL hex literal.
fn format_sql_binary(data: &[u8]) -> String {
    let hex: String = data.iter().map(|b| format!("{:02X}", b)).collect();
    format!("0x{}", hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sink_uri() {
        let config = MssqlSinkConfig::from_uri(
            "mssql://sa:Pass123!@localhost:1433/testdb?schema=dbo&table=output",
        )
        .unwrap();

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 1433);
        assert_eq!(config.database, "testdb");
        assert_eq!(config.username, "sa");
        assert_eq!(config.password, "Pass123!");
        assert_eq!(config.schema_name, "dbo");
        assert_eq!(config.table_name, "output");
    }

    #[test]
    fn test_parse_uri_default_schema() {
        let config =
            MssqlSinkConfig::from_uri("mssql://sa:Pass123!@localhost:1433/testdb?table=output")
                .unwrap();

        assert_eq!(config.schema_name, "dbo");
    }

    #[test]
    fn test_arrow_to_mssql_type() {
        assert_eq!(arrow_to_mssql_type(&ArrowDataType::Int32), "INT");
        assert_eq!(arrow_to_mssql_type(&ArrowDataType::Int64), "BIGINT");
        assert_eq!(arrow_to_mssql_type(&ArrowDataType::Float64), "FLOAT");
        assert_eq!(arrow_to_mssql_type(&ArrowDataType::Utf8), "NVARCHAR(MAX)");
        assert_eq!(arrow_to_mssql_type(&ArrowDataType::Boolean), "BIT");
    }

    #[test]
    fn test_format_sql_string() {
        assert_eq!(format_sql_string("hello"), "N'hello'");
        assert_eq!(format_sql_string("it's"), "N'it''s'");
        assert_eq!(format_sql_string("O'Brien"), "N'O''Brien'");
    }

    #[test]
    fn test_format_sql_binary() {
        assert_eq!(format_sql_binary(&[0xDE, 0xAD, 0xBE, 0xEF]), "0xDEADBEEF");
    }
}
