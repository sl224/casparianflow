use crate::{LockedColumn, LockedSchema};
use casparian_protocol::SchemaDefinition;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaSpecError {
    #[error("Output name cannot be empty")]
    EmptyOutputName,
    #[error("Output name must start with a letter: '{0}'")]
    OutputNameMustStartWithLetter(String),
    #[error("Output name must be lowercase alphanumeric + underscore: '{0}'")]
    OutputNameInvalidChars(String),
    #[error("Schema for '{0}' must include at least one column")]
    NoColumns(String),
    #[error("Schema for '{0}' has a column with empty name")]
    EmptyColumnName(String),
    #[error("Schema for '{0}' has duplicate column '{1}'")]
    DuplicateColumnName(String, String),
    #[error("Failed to serialize outputs JSON: {0}")]
    Serialization(String),
}

#[derive(Debug, Serialize)]
struct OutputSpec {
    content_hash: String,
}

pub fn locked_schema_from_definition(
    output_name: &str,
    schema_def: &SchemaDefinition,
) -> Result<LockedSchema, SchemaSpecError> {
    if output_name.trim().is_empty() {
        return Err(SchemaSpecError::EmptyOutputName);
    }
    let mut chars = output_name.chars();
    let first = chars
        .next()
        .ok_or(SchemaSpecError::EmptyOutputName)?;
    if !first.is_ascii_alphabetic() {
        return Err(SchemaSpecError::OutputNameMustStartWithLetter(
            output_name.to_string(),
        ));
    }
    if !output_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(SchemaSpecError::OutputNameInvalidChars(
            output_name.to_string(),
        ));
    }
    if schema_def.columns.is_empty() {
        return Err(SchemaSpecError::NoColumns(output_name.to_string()));
    }

    let mut seen = HashSet::new();
    let mut columns = Vec::with_capacity(schema_def.columns.len());
    for col in &schema_def.columns {
        if col.name.trim().is_empty() {
            return Err(SchemaSpecError::EmptyColumnName(output_name.to_string()));
        }
        if !seen.insert(col.name.clone()) {
            return Err(SchemaSpecError::DuplicateColumnName(
                output_name.to_string(),
                col.name.clone(),
            ));
        }
        let mut locked = if col.nullable {
            LockedColumn::optional(&col.name, col.data_type.clone())
        } else {
            LockedColumn::required(&col.name, col.data_type.clone())
        };
        if let Some(format) = &col.format {
            locked = locked.with_format(format);
        }
        columns.push(locked);
    }

    Ok(LockedSchema::new(output_name, columns))
}

pub fn build_outputs_json(
    schema_defs: &BTreeMap<String, SchemaDefinition>,
) -> Result<String, SchemaSpecError> {
    let mut outputs = BTreeMap::new();
    for (output_name, schema_def) in schema_defs {
        let locked = locked_schema_from_definition(output_name, schema_def)?;
        outputs.insert(
            output_name.clone(),
            OutputSpec {
                content_hash: locked.content_hash,
            },
        );
    }
    serde_json::to_string(&outputs).map_err(|e| SchemaSpecError::Serialization(e.to_string()))
}
