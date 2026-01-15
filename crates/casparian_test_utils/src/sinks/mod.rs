//! Database sink writers for Arrow data.

pub mod postgres_sink;

#[cfg(feature = "mssql")]
pub mod mssql_sink;

pub use postgres_sink::PostgresSink;

#[cfg(feature = "mssql")]
pub use mssql_sink::MssqlSink;
