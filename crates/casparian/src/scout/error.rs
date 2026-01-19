//! Error types for the Scout system

use std::io;
use thiserror::Error;

/// Scout error type
#[derive(Error, Debug)]
pub enum ScoutError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Database error: {0}")]
    Database(#[from] casparian_db::BackendError),

    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Source not found: {0}")]
    SourceNotFound(String),

    #[error("Route not found: {0}")]
    RouteNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Schema inference failed: {0}")]
    SchemaInference(String),

    #[error("Transform error: {0}")]
    Transform(String),

    #[error("Pattern error: {0}")]
    Pattern(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Extractor error: {0}")]
    Extractor(String),

    #[error("Source path '{new_path}' is inside existing source '{existing_name}' ({existing_path})")]
    SourceIsChildOfExisting {
        new_path: String,
        existing_name: String,
        existing_path: String,
    },

    #[error("Source path '{new_path}' encompasses existing source '{existing_name}' ({existing_path})")]
    SourceIsParentOfExisting {
        new_path: String,
        existing_name: String,
        existing_path: String,
    },
}

/// Result type alias
pub type Result<T> = std::result::Result<T, ScoutError>;
