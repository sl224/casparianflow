use anyhow::{anyhow, bail, Result};
use arrow::array::{Array, ArrayRef, LargeStringArray, StringArray};
use arrow::datatypes::DataType as ArrowDataType;
use arrow::record_batch::RecordBatch;
use casparian_protocol::DataType as SchemaDataType;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone)]
struct SchemaDef {
    columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone)]
struct ColumnDef {
    name: String,
    data_type: SchemaDataType,
    nullable: bool,
    #[allow(dead_code)]
    // Reserved for future per-value format validation.
    format: Option<String>,
}

pub fn enforce_schema_on_batches(
    batches: &[RecordBatch],
    schema_def_json: &str,
    output_name: &str,
) -> Result<Vec<RecordBatch>> {
    let schema_def = parse_schema_def(schema_def_json, output_name)?;
    let mut validated = Vec::with_capacity(batches.len());
    for batch in batches {
        validated.push(validate_record_batch(batch, &schema_def)?);
    }
    Ok(validated)
}

fn parse_schema_def(schema_def_json: &str, output_name: &str) -> Result<SchemaDef> {
    let raw = schema_def_json.trim();
    if raw.is_empty() || raw == "null" {
        bail!("schema_def is empty");
    }

    let value: Value = serde_json::from_str(raw)
        .map_err(|e| anyhow!("schema_def is not valid JSON: {}", e))?;

    if let Some(schemas_value) = value.get("schemas") {
        let schemas = schemas_value
            .as_array()
            .ok_or_else(|| anyhow!("schema_def.schemas must be an array"))?;
        if schemas.is_empty() {
            bail!("schema_def.schemas is empty");
        }
        let selected = schemas
            .iter()
            .find(|schema| schema.get("name").and_then(|v| v.as_str()) == Some(output_name))
            .unwrap_or(&schemas[0]);
        return schema_from_value(selected);
    }

    if let Some(columns_value) = value.get("columns") {
        let columns = columns_from_value(columns_value)?;
        return Ok(SchemaDef { columns });
    }

    if value.is_array() {
        let columns = columns_from_value(&value)?;
        return Ok(SchemaDef { columns });
    }

    Err(anyhow!(
        "schema_def must be a contract, schema object with columns, or array of columns"
    ))
}

fn schema_from_value(value: &Value) -> Result<SchemaDef> {
    let columns_value = value
        .get("columns")
        .ok_or_else(|| anyhow!("schema missing columns"))?;
    let columns = columns_from_value(columns_value)?;
    Ok(SchemaDef { columns })
}

fn columns_from_value(value: &Value) -> Result<Vec<ColumnDef>> {
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow!("columns must be an array"))?;
    let mut columns = Vec::with_capacity(arr.len());
    for col_value in arr {
        columns.push(column_from_value(col_value)?);
    }
    Ok(columns)
}

fn column_from_value(value: &Value) -> Result<ColumnDef> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("column must be an object"))?;
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("column.name is required"))?
        .to_string();
    let nullable = obj
        .get("nullable")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let format = obj
        .get("format")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let data_type = if let Some(dt_value) = obj.get("data_type").or_else(|| obj.get("type")) {
        serde_json::from_value::<SchemaDataType>(dt_value.clone())
            .map_err(|e| anyhow!("column.data_type invalid: {}", e))?
    } else if let Some(dtype_value) = obj.get("dtype") {
        let dtype = dtype_value
            .as_str()
            .ok_or_else(|| anyhow!("column.dtype must be a string"))?;
        parse_dtype_string(dtype)?
    } else {
        bail!("column.data_type or column.dtype is required");
    };

    Ok(ColumnDef {
        name,
        data_type,
        nullable,
        format,
    })
}

fn parse_dtype_string(raw: &str) -> Result<SchemaDataType> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("dtype is empty");
    }

    if let Ok(dt) = SchemaDataType::from_str(trimmed) {
        return Ok(dt);
    }

    let lower = trimmed.to_lowercase();
    if let Some(dt) = parse_polars_datetime(&lower) {
        return Ok(dt);
    }
    if let Some(dt) = parse_polars_decimal(&lower) {
        return Ok(dt);
    }
    if let Some(dt) = parse_pandas_datetime(&lower) {
        return Ok(dt);
    }

    let dt = match lower.as_str() {
        "int" | "integer" | "int8" | "int16" | "int32" | "int64" => SchemaDataType::Int64,
        "uint8" | "uint16" | "uint32" | "uint64" => SchemaDataType::Int64,
        "float" | "float32" | "float64" | "double" => SchemaDataType::Float64,
        "bool" | "boolean" => SchemaDataType::Boolean,
        "str" | "string" | "utf8" | "text" | "object" | "category" => SchemaDataType::String,
        "binary" | "bytes" => SchemaDataType::Binary,
        "date" | "date32" | "date64" => SchemaDataType::Date,
        "time" | "time32" | "time64" => SchemaDataType::Time,
        "timestamp" | "datetime" => SchemaDataType::Timestamp,
        _ => {
            bail!("unsupported dtype string '{}'", trimmed);
        }
    };

    Ok(dt)
}

fn parse_polars_datetime(lower: &str) -> Option<SchemaDataType> {
    if !lower.starts_with("datetime(") {
        return None;
    }
    let tz = extract_between(lower, "time_zone=", &[',', ')'])
        .map(|s| s.trim_matches('\'').trim_matches('"').to_string());
    match tz.as_deref() {
        Some("none") | Some("null") => Some(SchemaDataType::Timestamp),
        Some(tz) if !tz.is_empty() => Some(SchemaDataType::TimestampTz { tz: tz.to_string() }),
        _ => Some(SchemaDataType::Timestamp),
    }
}

fn parse_polars_decimal(lower: &str) -> Option<SchemaDataType> {
    if !lower.starts_with("decimal(") {
        return None;
    }
    let precision = extract_between(lower, "precision=", &[',', ')'])?.parse::<u8>().ok()?;
    let scale = extract_between(lower, "scale=", &[',', ')'])?.parse::<u8>().ok()?;
    Some(SchemaDataType::Decimal { precision, scale })
}

fn parse_pandas_datetime(lower: &str) -> Option<SchemaDataType> {
    if !lower.starts_with("datetime64") {
        return None;
    }
    if lower.contains("utc") || lower.contains("tz=") {
        return Some(SchemaDataType::TimestampTz {
            tz: "UTC".to_string(),
        });
    }
    Some(SchemaDataType::Timestamp)
}

fn extract_between<'a>(value: &'a str, key: &str, terminators: &[char]) -> Option<&'a str> {
    let start = value.find(key)? + key.len();
    let slice = &value[start..];
    let end = slice
        .find(|c| terminators.contains(&c))
        .unwrap_or(slice.len());
    Some(&slice[..end])
}

fn validate_record_batch(batch: &RecordBatch, schema_def: &SchemaDef) -> Result<RecordBatch> {
    let mut data_field_indices = Vec::with_capacity(batch.num_columns());
    for (idx, field) in batch.schema().fields().iter().enumerate() {
        if field.name() == "_cf_row_error" {
            continue;
        }
        data_field_indices.push(idx);
    }

    if schema_def.columns.len() != data_field_indices.len() {
        bail!(
            "schema mismatch: expected {} columns, got {}",
            schema_def.columns.len(),
            data_field_indices.len()
        );
    }

    let schema = batch.schema();
    for (pos, (expected, idx)) in schema_def
        .columns
        .iter()
        .zip(data_field_indices.iter())
        .enumerate()
    {
        let actual_name = schema.field(*idx).name();
        if expected.name != *actual_name {
            bail!(
                "schema mismatch at column {}: expected '{}', got '{}'",
                pos,
                expected.name,
                actual_name
            );
        }
    }

    let mut row_errors: Vec<String> = vec![String::new(); batch.num_rows()];
    let mut has_new_errors = false;

    for (expected, idx) in schema_def.columns.iter().zip(data_field_indices.iter()) {
        let array = batch.column(*idx);
        let type_check = type_check_mode(&expected.data_type, array.data_type(), expected)?;

        match type_check {
            TypeCheck::Compatible => {
                if !expected.nullable && array.null_count() > 0 {
                    for row in 0..array.len() {
                        if array.is_null(row) {
                            append_error(
                                &mut row_errors[row],
                                &format!("schema: null not allowed in '{}'", expected.name),
                            );
                            has_new_errors = true;
                        }
                    }
                }
                if validate_format_values(expected, array, &mut row_errors)? {
                    has_new_errors = true;
                }
            }
            TypeCheck::NullOnly => {
                if !expected.nullable && array.len() > 0 {
                    for row in 0..array.len() {
                        append_error(
                            &mut row_errors[row],
                            &format!("schema: null not allowed in '{}'", expected.name),
                        );
                        has_new_errors = true;
                    }
                }
            }
        }
    }

    if !has_new_errors && batch.schema().index_of("_cf_row_error").is_err() {
        return Ok(batch.clone());
    }

    let merged = merge_error_column(batch, &row_errors)?;
    Ok(merged)
}

enum TypeCheck {
    Compatible,
    NullOnly,
}

fn type_check_mode(
    expected: &SchemaDataType,
    actual: &ArrowDataType,
    column: &ColumnDef,
) -> Result<TypeCheck> {
    use ArrowDataType as A;
    use SchemaDataType as S;

    if matches!(actual, A::Null) {
        return Ok(TypeCheck::NullOnly);
    }

    let compatible = match (expected, actual) {
        (S::Null, A::Null) => true,
        (S::Boolean, A::Boolean) => true,
        (S::Int64, A::Int8 | A::Int16 | A::Int32 | A::Int64) => true,
        (S::Float64, A::Float32 | A::Float64) => true,
        (S::Float64, A::Int8 | A::Int16 | A::Int32 | A::Int64) => true,
        (S::Date, A::Date32 | A::Date64) => true,
        (S::Timestamp, A::Timestamp(_, tz)) => tz.is_none(),
        (S::TimestampTz { tz }, A::Timestamp(_, tz_actual)) => {
            tz_actual.as_ref().map(|s| eq_tz(s, tz)).unwrap_or(false)
        }
        (S::Time, A::Time32(_) | A::Time64(_)) => true,
        (S::Date | S::Timestamp | S::TimestampTz { .. } | S::Time, A::Utf8 | A::LargeUtf8)
            if column.format.is_some() =>
        {
            true
        }
        (S::Duration, A::Duration(_)) => true,
        (S::String, A::Utf8 | A::LargeUtf8) => true,
        (S::Binary, A::Binary | A::LargeBinary) => true,
        (S::Decimal { precision, scale }, A::Decimal128(p, s)) => {
            *precision == *p && *scale as i8 == *s
        }
        (S::Decimal { precision, scale }, A::Decimal256(p, s)) => {
            *precision == *p && *scale as i8 == *s
        }
        (S::List { .. }, A::List(_) | A::LargeList(_)) => true,
        (S::Struct { .. }, A::Struct(_)) => true,
        _ => false,
    };

    if compatible {
        return Ok(TypeCheck::Compatible);
    }

    Err(anyhow!(
        "schema mismatch for '{}': expected {}, got {}",
        column.name,
        expected,
        actual
    ))
}

fn validate_format_values(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> Result<bool> {
    let Some(format) = expected.format.as_deref() else {
        return Ok(false);
    };

    let is_temporal = matches!(
        expected.data_type,
        SchemaDataType::Date
            | SchemaDataType::Time
            | SchemaDataType::Timestamp
            | SchemaDataType::TimestampTz { .. }
    );
    if !is_temporal {
        return Ok(false);
    }

    let message = format!(
        "schema: value does not match format '{}' for '{}'",
        format, expected.name
    );
    let mut has_errors = false;

    match array.data_type() {
        ArrowDataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("column '{}' is not Utf8", expected.name))?;
            for row in 0..arr.len() {
                if arr.is_null(row) {
                    continue;
                }
                let value = arr.value(row);
                if expected
                    .data_type
                    .validate_string_with_format(value, Some(format))
                {
                    continue;
                }
                append_error(&mut row_errors[row], &message);
                has_errors = true;
            }
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| anyhow!("column '{}' is not LargeUtf8", expected.name))?;
            for row in 0..arr.len() {
                if arr.is_null(row) {
                    continue;
                }
                let value = arr.value(row);
                if expected
                    .data_type
                    .validate_string_with_format(value, Some(format))
                {
                    continue;
                }
                append_error(&mut row_errors[row], &message);
                has_errors = true;
            }
        }
        _ => {
            // Format validation only applies to string arrays; typed arrays are accepted as-is.
        }
    }

    Ok(has_errors)
}

fn merge_error_column(batch: &RecordBatch, new_errors: &[String]) -> Result<RecordBatch> {
    let error_idx = batch.schema().index_of("_cf_row_error").ok();
    let use_large = match error_idx {
        Some(idx) => matches!(batch.column(idx).data_type(), ArrowDataType::LargeUtf8),
        None => false,
    };

    let mut combined: Vec<Option<String>> = Vec::with_capacity(new_errors.len());
    for row in 0..new_errors.len() {
        let existing = match error_idx {
            Some(idx) => error_value(batch.column(idx), row)?,
            None => None,
        };
        let new_msg = if new_errors[row].is_empty() {
            None
        } else {
            Some(new_errors[row].clone())
        };

        let merged = match (existing, new_msg) {
            (None, None) => None,
            (Some(existing), None) => {
                if existing.is_empty() {
                    None
                } else {
                    Some(existing)
                }
            }
            (None, Some(new_msg)) => Some(new_msg),
            (Some(existing), Some(new_msg)) => {
                if existing.is_empty() {
                    Some(new_msg)
                } else {
                    Some(format!("{}; {}", existing, new_msg))
                }
            }
        };
        combined.push(merged);
    }

    let error_array: ArrayRef = if use_large {
        let mut builder = arrow::array::LargeStringBuilder::new();
        for value in combined {
            if let Some(v) = value {
                builder.append_value(v);
            } else {
                builder.append_null();
            }
        }
        std::sync::Arc::new(builder.finish())
    } else {
        let mut builder = arrow::array::StringBuilder::new();
        for value in combined {
            if let Some(v) = value {
                builder.append_value(v);
            } else {
                builder.append_null();
            }
        }
        std::sync::Arc::new(builder.finish())
    };

    let mut fields: Vec<_> = batch.schema().fields().iter().cloned().collect();
    let mut columns = batch.columns().to_vec();

    match error_idx {
        Some(idx) => {
            columns[idx] = error_array;
        }
        None => {
            fields.push(std::sync::Arc::new(arrow::datatypes::Field::new(
                "_cf_row_error",
                ArrowDataType::Utf8,
                true,
            )));
            columns.push(error_array);
        }
    }

    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(fields));
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn error_value(array: &ArrayRef, row: usize) -> Result<Option<String>> {
    match array.data_type() {
        ArrowDataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("_cf_row_error is not Utf8"))?;
            if arr.is_null(row) {
                Ok(None)
            } else {
                Ok(Some(arr.value(row).to_string()))
            }
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| anyhow!("_cf_row_error is not LargeUtf8"))?;
            if arr.is_null(row) {
                Ok(None)
            } else {
                Ok(Some(arr.value(row).to_string()))
            }
        }
        _ => Err(anyhow!("_cf_row_error must be Utf8 or LargeUtf8")),
    }
}

fn append_error(target: &mut String, message: &str) {
    if !target.is_empty() {
        target.push_str("; ");
    }
    target.push_str(message);
}

fn eq_tz(a: &str, b: &str) -> bool {
    normalize_tz(a) == normalize_tz(b)
}

fn normalize_tz(value: &str) -> String {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "utc" | "etc/utc" | "gmt" | "etc/gmt" => "utc".to_string(),
        _ => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{Field, Schema};
    use std::sync::Arc;

    #[test]
    fn test_parse_schema_def_array() {
        let json = r#"[{"name":"id","dtype":"int64"},{"name":"name","dtype":"utf8"}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        assert_eq!(schema.columns.len(), 2);
        assert_eq!(schema.columns[0].name, "id");
        assert_eq!(schema.columns[1].data_type, SchemaDataType::String);
    }

    #[test]
    fn test_schema_validation_string_hardfail() {
        let json = r#"[{"name":"id","dtype":"int64","nullable":false}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        let ids = StringArray::from(vec![Some("1"), Some("bad")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", ArrowDataType::Utf8, true)])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let err = validate_record_batch(&batch, &schema).unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_schema_validation_nullability() {
        let json = r#"[{"name":"id","dtype":"int64","nullable":false}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        let ids = Int64Array::from(vec![Some(1), None]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", ArrowDataType::Int64, true)])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema).unwrap();
        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.is_null(0));
        assert!(error_col.value(1).contains("null not allowed"));
    }

    #[test]
    fn test_schema_validation_type_mismatch() {
        let json = r#"[{"name":"id","dtype":"int64"}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        let ids = arrow::array::BooleanArray::from(vec![Some(true), Some(false)]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", ArrowDataType::Boolean, true)])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let err = validate_record_batch(&batch, &schema).unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_schema_validation_format_date_string() {
        let json = r#"[{"name":"date","dtype":"date","format":"%Y-%m-%d"}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        let dates = StringArray::from(vec![Some("2024-01-15"), Some("01/15/2024")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("date", ArrowDataType::Utf8, true)])),
            vec![Arc::new(dates) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema).unwrap();
        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.is_null(0));
        assert!(error_col.value(1).contains("format"));
    }

    #[test]
    fn test_schema_validation_format_timestamp_string_success() {
        let json = r#"[{"name":"created_at","dtype":"timestamp","format":"%Y-%m-%d %H:%M:%S"}]"#;
        let schema = parse_schema_def(json, "output").unwrap();
        let values =
            StringArray::from(vec![Some("2024-01-15 10:30:00"), Some("2024-01-16 09:00:00")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "created_at",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(values) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema).unwrap();
        assert!(validated.schema().index_of("_cf_row_error").is_err());
    }

    #[test]
    fn test_timezone_alias_matching() {
        assert!(eq_tz("UTC", "Etc/UTC"));
        assert!(eq_tz("gmt", "UTC"));
        assert!(eq_tz("America/New_York", "america/new_york"));
    }
}
