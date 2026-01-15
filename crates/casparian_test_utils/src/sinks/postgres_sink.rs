//! PostgreSQL sink writer for Arrow data.

use anyhow::{bail, Context, Result};
use arrow::array::*;
use arrow::datatypes::{DataType as ArrowDataType, Schema};
use arrow::record_batch::RecordBatch;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::debug;
use url::Url;

/// PostgreSQL sink for writing Arrow data to a table.
pub struct PostgresSink {
    pool: PgPool,
    schema_name: String,
    table_name: String,
}

/// Configuration parsed from a PostgreSQL sink URI.
#[derive(Debug, Clone)]
pub struct PostgresSinkConfig {
    /// Connection string (without query params)
    pub connection_string: String,
    /// Schema name (defaults to "public")
    pub schema_name: String,
    /// Table name
    pub table_name: String,
}

impl PostgresSinkConfig {
    /// Parse a PostgreSQL sink URI.
    ///
    /// Format: `postgres://user:pass@host:port/db?schema=test&table=output`
    pub fn from_uri(uri: &str) -> Result<Self> {
        let url = Url::parse(uri).context("Invalid PostgreSQL URI")?;

        if url.scheme() != "postgres" && url.scheme() != "postgresql" {
            bail!("Expected postgres:// or postgresql:// URI");
        }

        // Get query parameters
        let schema_name = url
            .query_pairs()
            .find(|(k, _)| k == "schema")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "public".to_string());

        let table_name = url
            .query_pairs()
            .find(|(k, _)| k == "table")
            .map(|(_, v)| v.to_string())
            .ok_or_else(|| anyhow::anyhow!("Missing 'table' parameter in URI"))?;

        // Build connection string without query params
        let mut conn_url = url.clone();
        conn_url.set_query(None);

        Ok(Self {
            connection_string: conn_url.to_string(),
            schema_name,
            table_name,
        })
    }
}

impl PostgresSink {
    /// Create a new PostgreSQL sink from a pool.
    pub fn new(pool: PgPool, schema_name: &str, table_name: &str) -> Self {
        Self {
            pool,
            schema_name: schema_name.to_string(),
            table_name: table_name.to_string(),
        }
    }

    /// Create a sink from a URI configuration.
    pub async fn from_config(config: &PostgresSinkConfig) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.connection_string)
            .await?;

        Ok(Self::new(pool, &config.schema_name, &config.table_name))
    }

    /// Create the target table based on Arrow schema.
    ///
    /// Drops the table if it exists.
    pub async fn create_table(&self, schema: &Schema) -> Result<()> {
        let full_name = format!("{}.{}", self.schema_name, self.table_name);

        // Drop existing table
        let drop_sql = format!("DROP TABLE IF EXISTS {} CASCADE", full_name);
        sqlx::query(&drop_sql).execute(&self.pool).await?;

        // Build CREATE TABLE statement
        let columns: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| {
                let pg_type = arrow_to_pg_type(f.data_type());
                let nullable = if f.is_nullable() { "" } else { " NOT NULL" };
                format!("{} {}{}", f.name(), pg_type, nullable)
            })
            .collect();

        let create_sql = format!("CREATE TABLE {} ({})", full_name, columns.join(", "));
        debug!("Creating table: {}", create_sql);
        sqlx::query(&create_sql).execute(&self.pool).await?;

        Ok(())
    }

    /// Write a record batch to the table.
    pub async fn write_batch(&self, batch: &RecordBatch) -> Result<()> {
        if batch.num_rows() == 0 {
            return Ok(());
        }

        let full_name = format!("{}.{}", self.schema_name, self.table_name);
        let schema = batch.schema();
        let column_names: Vec<&str> = schema
            .fields()
            .iter()
            .map(|f| f.name().as_str())
            .collect();

        // Build INSERT statement with placeholders
        let placeholders: Vec<String> = (0..column_names.len())
            .map(|i| format!("${}", i + 1))
            .collect();

        let insert_sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            full_name,
            column_names.join(", "),
            placeholders.join(", ")
        );

        // Insert row by row (simple implementation)
        for row_idx in 0..batch.num_rows() {
            let mut query = sqlx::query(&insert_sql);

            for col_idx in 0..batch.num_columns() {
                query = bind_value(query, batch.column(col_idx), row_idx)?;
            }

            query.execute(&self.pool).await?;
        }

        debug!(
            "Wrote {} rows to {}",
            batch.num_rows(),
            full_name
        );

        Ok(())
    }

    /// Get the full table name (schema.table).
    pub fn full_table_name(&self) -> String {
        format!("{}.{}", self.schema_name, self.table_name)
    }
}

/// Convert Arrow DataType to PostgreSQL type.
fn arrow_to_pg_type(dt: &ArrowDataType) -> &'static str {
    match dt {
        ArrowDataType::Boolean => "BOOLEAN",
        ArrowDataType::Int8 => "SMALLINT",
        ArrowDataType::Int16 => "SMALLINT",
        ArrowDataType::Int32 => "INTEGER",
        ArrowDataType::Int64 => "BIGINT",
        ArrowDataType::UInt8 => "SMALLINT",
        ArrowDataType::UInt16 => "INTEGER",
        ArrowDataType::UInt32 => "BIGINT",
        ArrowDataType::UInt64 => "NUMERIC(20,0)",
        ArrowDataType::Float32 => "REAL",
        ArrowDataType::Float64 => "DOUBLE PRECISION",
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => "TEXT",
        ArrowDataType::Binary | ArrowDataType::LargeBinary => "BYTEA",
        ArrowDataType::Date32 | ArrowDataType::Date64 => "DATE",
        ArrowDataType::Timestamp(_, _) => "TIMESTAMP",
        ArrowDataType::Time32(_) | ArrowDataType::Time64(_) => "TIME",
        ArrowDataType::Decimal128(_, _) | ArrowDataType::Decimal256(_, _) => "NUMERIC",
        _ => "TEXT", // Fallback
    }
}

/// Bind an Arrow array value to a sqlx query.
fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    array: &Arc<dyn Array>,
    row_idx: usize,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>> {
    if array.is_null(row_idx) {
        return Ok(query.bind(None::<String>));
    }

    let result = match array.data_type() {
        ArrowDataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            query.bind(arr.value(row_idx) as i16)
        }
        ArrowDataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            query.bind(arr.value(row_idx) as i16)
        }
        ArrowDataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            query.bind(arr.value(row_idx) as i32)
        }
        ArrowDataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            query.bind(arr.value(row_idx) as i64)
        }
        ArrowDataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            // Convert to string for NUMERIC
            query.bind(arr.value(row_idx).to_string())
        }
        ArrowDataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            query.bind(arr.value(row_idx))
        }
        ArrowDataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            query.bind(arr.value(row_idx).to_string())
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            query.bind(arr.value(row_idx).to_string())
        }
        ArrowDataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            query.bind(arr.value(row_idx).to_vec())
        }
        ArrowDataType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            query.bind(arr.value(row_idx).to_vec())
        }
        _ => {
            // Fallback: convert to string
            let arr = arrow::util::display::ArrayFormatter::try_new(
                array.as_ref(),
                &arrow::util::display::FormatOptions::default(),
            )?;
            query.bind(arr.value(row_idx).to_string())
        }
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sink_uri() {
        let config = PostgresSinkConfig::from_uri(
            "postgres://user:pass@localhost:5432/db?schema=test&table=output",
        )
        .unwrap();

        assert_eq!(config.schema_name, "test");
        assert_eq!(config.table_name, "output");
        assert_eq!(
            config.connection_string,
            "postgres://user:pass@localhost:5432/db"
        );
    }

    #[test]
    fn test_parse_uri_default_schema() {
        let config = PostgresSinkConfig::from_uri(
            "postgres://user:pass@localhost:5432/db?table=output",
        )
        .unwrap();

        assert_eq!(config.schema_name, "public");
        assert_eq!(config.table_name, "output");
    }

    #[test]
    fn test_parse_uri_missing_table() {
        let result = PostgresSinkConfig::from_uri(
            "postgres://user:pass@localhost:5432/db?schema=test",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_arrow_to_pg_type() {
        assert_eq!(arrow_to_pg_type(&ArrowDataType::Int32), "INTEGER");
        assert_eq!(arrow_to_pg_type(&ArrowDataType::Int64), "BIGINT");
        assert_eq!(arrow_to_pg_type(&ArrowDataType::Float64), "DOUBLE PRECISION");
        assert_eq!(arrow_to_pg_type(&ArrowDataType::Utf8), "TEXT");
        assert_eq!(arrow_to_pg_type(&ArrowDataType::Boolean), "BOOLEAN");
    }
}
