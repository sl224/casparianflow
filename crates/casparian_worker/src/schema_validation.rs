use anyhow::anyhow;
use arrow::array::{
    Array, ArrayRef, Date32Array, Decimal128Array, LargeStringArray, StringArray,
    Time64MicrosecondArray, TimestampMicrosecondArray,
};
use arrow::datatypes::DataType as ArrowDataType;
use arrow::record_batch::RecordBatch;
use casparian_protocol::types::{
    ColumnOrderMismatch, ObservedColumn, ObservedDataType, SchemaColumnSpec, SchemaDefinition,
    SchemaMismatch, TypeMismatch,
};
use casparian_protocol::DataType as SchemaDataType;
use chrono::Timelike;
use thiserror::Error;

type SchemaResult<T> = std::result::Result<T, SchemaValidationError>;
type AnyhowResult<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Error)]
pub enum SchemaValidationError {
    #[error("{message}")]
    InvalidSchemaDef { message: String },
    #[error("schema mismatch for '{output_name}'")]
    SchemaMismatch {
        output_name: String,
        mismatch: SchemaMismatch,
    },
}

pub fn summarize_schema_mismatch(mismatch: &SchemaMismatch) -> String {
    let mut parts = Vec::new();

    let expected_len = mismatch.expected_columns.len();
    let actual_len = mismatch.actual_columns.len();
    if expected_len != actual_len {
        parts.push(format!(
            "expected {} columns, got {}",
            expected_len, actual_len
        ));
    }
    if !mismatch.missing_columns.is_empty() {
        parts.push(format!("missing: {}", mismatch.missing_columns.join(", ")));
    }
    if !mismatch.extra_columns.is_empty() {
        parts.push(format!("extra: {}", mismatch.extra_columns.join(", ")));
    }
    if !mismatch.order_mismatches.is_empty() {
        let details = mismatch
            .order_mismatches
            .iter()
            .take(3)
            .map(|m| format!("#{} {} != {}", m.index, m.expected, m.actual))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("order: {}", details));
    }
    if !mismatch.type_mismatches.is_empty() {
        let details = mismatch
            .type_mismatches
            .iter()
            .take(3)
            .map(|m| {
                format!(
                    "{}: {} != {}",
                    m.name,
                    m.expected,
                    observed_type_label(&m.actual)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("types: {}", details));
    }

    if parts.is_empty() {
        format!("schema mismatch for '{}'", mismatch.output_name)
    } else {
        format!(
            "schema mismatch for '{}': {}",
            mismatch.output_name,
            parts.join("; ")
        )
    }
}

impl From<anyhow::Error> for SchemaValidationError {
    fn from(err: anyhow::Error) -> Self {
        SchemaValidationError::InvalidSchemaDef {
            message: err.to_string(),
        }
    }
}

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
    schema_def: &SchemaDefinition,
    output_name: &str,
) -> SchemaResult<Vec<RecordBatch>> {
    let schema_def = schema_def_from_definition(schema_def)?;
    let mut validated = Vec::with_capacity(batches.len());
    for batch in batches {
        validated.push(validate_record_batch(batch, &schema_def, output_name)?);
    }
    Ok(validated)
}

fn schema_def_from_definition(schema_def: &SchemaDefinition) -> SchemaResult<SchemaDef> {
    if schema_def.columns.is_empty() {
        return Err(SchemaValidationError::InvalidSchemaDef {
            message: "schema_def.columns is empty".to_string(),
        });
    }

    let columns = schema_def
        .columns
        .iter()
        .map(|col| ColumnDef {
            name: col.name.clone(),
            data_type: col.data_type.clone(),
            nullable: col.nullable,
            format: col.format.clone(),
        })
        .collect();

    Ok(SchemaDef { columns })
}

fn validate_record_batch(
    batch: &RecordBatch,
    schema_def: &SchemaDef,
    output_name: &str,
) -> SchemaResult<RecordBatch> {
    let mut data_field_indices = Vec::with_capacity(batch.num_columns());
    for (idx, field) in batch.schema().fields().iter().enumerate() {
        if field.name() == "_cf_row_error" {
            continue;
        }
        data_field_indices.push(idx);
    }

    if schema_def.columns.len() != data_field_indices.len() {
        return Err(build_schema_mismatch(
            output_name,
            schema_def,
            batch,
            &data_field_indices,
        ));
    }

    let schema = batch.schema();
    for (expected, idx) in schema_def.columns.iter().zip(data_field_indices.iter()) {
        let actual_name = schema.field(*idx).name();
        if expected.name != *actual_name {
            return Err(build_schema_mismatch(
                output_name,
                schema_def,
                batch,
                &data_field_indices,
            ));
        }
    }

    let mut row_errors: Vec<String> = vec![String::new(); batch.num_rows()];
    let mut has_new_errors = false;
    let mut fields: Vec<_> = batch.schema().fields().iter().cloned().collect();
    let mut columns = batch.columns().to_vec();

    for (expected, idx) in schema_def.columns.iter().zip(data_field_indices.iter()) {
        let array = batch.column(*idx);
        let type_check = type_check_mode(&expected.data_type, array.data_type(), expected);
        let (next_array, cast_errors) = match type_check {
            TypeCheck::Compatible => {
                if validate_format_values(expected, array, &mut row_errors)? {
                    has_new_errors = true;
                }
                (array.clone(), false)
            }
            TypeCheck::NullOnly => (array.clone(), false),
            TypeCheck::Cast => cast_utf8_column(expected, array, &mut row_errors)?,
            TypeCheck::Mismatch => {
                return Err(build_schema_mismatch(
                    output_name,
                    schema_def,
                    batch,
                    &data_field_indices,
                ));
            }
        };

        if cast_errors {
            has_new_errors = true;
        }

        if !expected.nullable && next_array.null_count() > 0 {
            for row in 0..next_array.len() {
                if next_array.is_null(row) {
                    append_error(
                        &mut row_errors[row],
                        &format!("schema: null not allowed in '{}'", expected.name),
                    );
                    has_new_errors = true;
                }
            }
        }

        columns[*idx] = next_array.clone();
        let field_nullable = expected.nullable || next_array.null_count() > 0;
        fields[*idx] = std::sync::Arc::new(arrow::datatypes::Field::new(
            expected.name.as_str(),
            next_array.data_type().clone(),
            field_nullable,
        ));
    }

    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(fields));
    let rebuilt = RecordBatch::try_new(schema, columns).map_err(|e| anyhow::anyhow!(e))?;
    let merged = if has_new_errors {
        merge_error_column(&rebuilt, &row_errors)?
    } else {
        rebuilt
    };
    Ok(merged)
}

enum TypeCheck {
    Compatible,
    NullOnly,
    Cast,
    Mismatch,
}

fn type_check_mode(
    expected: &SchemaDataType,
    actual: &ArrowDataType,
    column: &ColumnDef,
) -> TypeCheck {
    use ArrowDataType as A;
    use SchemaDataType as S;

    if matches!(actual, A::Null) {
        return TypeCheck::NullOnly;
    }

    let castable_from_utf8 = matches!(actual, A::Utf8 | A::LargeUtf8)
        && (matches!(
            expected,
            S::Date | S::Timestamp | S::TimestampTz { .. } | S::Time
        ) && column.format.is_some()
            || matches!(expected, S::Decimal { .. }));

    if castable_from_utf8 {
        return TypeCheck::Cast;
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
        TypeCheck::Compatible
    } else {
        TypeCheck::Mismatch
    }
}

fn build_schema_mismatch(
    output_name: &str,
    schema_def: &SchemaDef,
    batch: &RecordBatch,
    data_field_indices: &[usize],
) -> SchemaValidationError {
    let expected_columns: Vec<SchemaColumnSpec> = schema_def
        .columns
        .iter()
        .map(|col| SchemaColumnSpec {
            name: col.name.clone(),
            data_type: col.data_type.clone(),
            nullable: col.nullable,
            format: col.format.clone(),
        })
        .collect();

    let schema = batch.schema();
    let actual_columns: Vec<ObservedColumn> = data_field_indices
        .iter()
        .map(|idx| {
            let field = schema.field(*idx);
            ObservedColumn {
                name: field.name().to_string(),
                data_type: observed_data_type(field.data_type()),
            }
        })
        .collect();

    let expected_names: Vec<String> = expected_columns
        .iter()
        .map(|col| col.name.clone())
        .collect();
    let actual_names: Vec<String> = actual_columns.iter().map(|col| col.name.clone()).collect();

    let expected_set: std::collections::HashSet<&str> =
        expected_names.iter().map(|name| name.as_str()).collect();
    let actual_set: std::collections::HashSet<&str> =
        actual_names.iter().map(|name| name.as_str()).collect();

    let missing_columns = expected_names
        .iter()
        .filter(|name| !actual_set.contains(name.as_str()))
        .cloned()
        .collect();
    let extra_columns = actual_names
        .iter()
        .filter(|name| !expected_set.contains(name.as_str()))
        .cloned()
        .collect();

    let mut order_mismatches = Vec::new();
    let min_len = std::cmp::min(expected_names.len(), actual_names.len());
    for idx in 0..min_len {
        let expected = &expected_names[idx];
        let actual = &actual_names[idx];
        if expected != actual {
            order_mismatches.push(ColumnOrderMismatch {
                index: idx,
                expected: expected.clone(),
                actual: actual.clone(),
            });
        }
    }

    let mut type_mismatches = Vec::new();
    for expected in &schema_def.columns {
        if let Some((idx, _)) = actual_names
            .iter()
            .enumerate()
            .find(|(_, name)| name.as_str() == expected.name.as_str())
        {
            let field = schema.field(data_field_indices[idx]);
            let actual_type = field.data_type();
            if !is_type_compatible(&expected.data_type, actual_type, expected) {
                type_mismatches.push(TypeMismatch {
                    name: expected.name.clone(),
                    expected: expected.data_type.clone(),
                    actual: observed_data_type(actual_type),
                });
            }
        }
    }

    SchemaValidationError::SchemaMismatch {
        output_name: output_name.to_string(),
        mismatch: SchemaMismatch {
            output_name: output_name.to_string(),
            expected_columns,
            actual_columns,
            missing_columns,
            extra_columns,
            order_mismatches,
            type_mismatches,
        },
    }
}

fn is_type_compatible(
    expected: &SchemaDataType,
    actual: &ArrowDataType,
    column: &ColumnDef,
) -> bool {
    use ArrowDataType as A;
    use SchemaDataType as S;

    if matches!(actual, A::Null) {
        return true;
    }

    match (expected, actual) {
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
        (S::Decimal { .. }, A::Utf8 | A::LargeUtf8) => true,
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
    }
}

fn observed_data_type(actual: &ArrowDataType) -> ObservedDataType {
    if let Some(data_type) = canonical_type_for_arrow(actual) {
        return ObservedDataType::Canonical { data_type };
    }
    ObservedDataType::Arrow {
        name: format!("{:?}", actual),
    }
}

fn observed_type_label(actual: &ObservedDataType) -> String {
    match actual {
        ObservedDataType::Canonical { data_type } => data_type.to_string(),
        ObservedDataType::Arrow { name } => name.clone(),
    }
}

fn canonical_type_for_arrow(actual: &ArrowDataType) -> Option<SchemaDataType> {
    use ArrowDataType as A;
    use SchemaDataType as S;

    match actual {
        A::Null => Some(S::Null),
        A::Boolean => Some(S::Boolean),
        A::Int8 | A::Int16 | A::Int32 | A::Int64 => Some(S::Int64),
        A::Float32 | A::Float64 => Some(S::Float64),
        A::Date32 | A::Date64 => Some(S::Date),
        A::Timestamp(_, tz) => match tz.as_ref() {
            Some(tz) => Some(S::TimestampTz { tz: tz.to_string() }),
            None => Some(S::Timestamp),
        },
        A::Time32(_) | A::Time64(_) => Some(S::Time),
        A::Duration(_) => Some(S::Duration),
        A::Utf8 | A::LargeUtf8 => Some(S::String),
        A::Binary | A::LargeBinary => Some(S::Binary),
        A::Decimal128(p, s) => Some(S::Decimal {
            precision: u8::try_from(*p).ok()?,
            scale: u8::try_from(*s).ok()?,
        }),
        A::Decimal256(p, s) => Some(S::Decimal {
            precision: u8::try_from(*p).ok()?,
            scale: u8::try_from(*s).ok()?,
        }),
        _ => None,
    }
}

fn validate_format_values(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<bool> {
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

fn cast_utf8_column(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    match expected.data_type {
        SchemaDataType::Date => cast_utf8_date(expected, array, row_errors),
        SchemaDataType::Time => cast_utf8_time(expected, array, row_errors),
        SchemaDataType::Timestamp => cast_utf8_timestamp(expected, array, row_errors),
        SchemaDataType::TimestampTz { .. } => cast_utf8_timestamp_tz(expected, array, row_errors),
        SchemaDataType::Decimal { .. } => cast_utf8_decimal(expected, array, row_errors),
        _ => Err(anyhow!(
            "column '{}' cannot be cast from Utf8",
            expected.name
        )),
    }
}

fn cast_utf8_date(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    let format = expected
        .format
        .as_deref()
        .ok_or_else(|| anyhow!("schema: missing format for '{}'", expected.name))?;
    let message = format!(
        "schema: value does not match format '{}' for '{}'",
        format, expected.name
    );
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
        .ok_or_else(|| anyhow!("schema: invalid epoch date"))?;
    let (values, has_errors) = cast_from_utf8_values(
        expected,
        array,
        row_errors,
        &message,
        |value| {
            chrono::NaiveDate::parse_from_str(value, format).ok().and_then(|date| {
                let days = date.signed_duration_since(epoch).num_days();
                i32::try_from(days).ok()
            })
        },
    )?;
    let array = Date32Array::from(values);
    Ok((std::sync::Arc::new(array), has_errors))
}

fn cast_utf8_time(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    let format = expected
        .format
        .as_deref()
        .ok_or_else(|| anyhow!("schema: missing format for '{}'", expected.name))?;
    let message = format!(
        "schema: value does not match format '{}' for '{}'",
        format, expected.name
    );
    let (values, has_errors) = cast_from_utf8_values(
        expected,
        array,
        row_errors,
        &message,
        |value| {
            chrono::NaiveTime::parse_from_str(value, format).ok().map(|time| {
                let secs = i64::from(time.num_seconds_from_midnight());
                let micros = i64::from(time.nanosecond() / 1_000);
                secs * 1_000_000 + micros
            })
        },
    )?;
    let array = Time64MicrosecondArray::from(values);
    Ok((std::sync::Arc::new(array), has_errors))
}

fn cast_utf8_timestamp(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    let format = expected
        .format
        .as_deref()
        .ok_or_else(|| anyhow!("schema: missing format for '{}'", expected.name))?;
    let message = format!(
        "schema: value does not match format '{}' for '{}'",
        format, expected.name
    );
    let (values, has_errors) = cast_from_utf8_values(
        expected,
        array,
        row_errors,
        &message,
        |value| {
            chrono::NaiveDateTime::parse_from_str(value, format)
                .ok()
                .map(|dt| dt.and_utc().timestamp_micros())
        },
    )?;
    let array = TimestampMicrosecondArray::from(values);
    Ok((std::sync::Arc::new(array), has_errors))
}

fn cast_utf8_timestamp_tz(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    let format = expected
        .format
        .as_deref()
        .ok_or_else(|| anyhow!("schema: missing format for '{}'", expected.name))?;
    let message = format!(
        "schema: value does not match format '{}' for '{}'",
        format, expected.name
    );
    let tz = match &expected.data_type {
        SchemaDataType::TimestampTz { tz } => tz.as_str(),
        _ => "",
    };
    let (values, has_errors) = cast_from_utf8_values(
        expected,
        array,
        row_errors,
        &message,
        |value| {
            if !expected
                .data_type
                .validate_string_with_format(value, Some(format))
            {
                return None;
            }
            chrono::DateTime::parse_from_str(value, format)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc).timestamp_micros())
        },
    )?;
    let array = TimestampMicrosecondArray::from(values).with_timezone(tz);
    Ok((std::sync::Arc::new(array), has_errors))
}

fn cast_utf8_decimal(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
) -> AnyhowResult<(ArrayRef, bool)> {
    let (precision, scale) = match expected.data_type {
        SchemaDataType::Decimal { precision, scale } => (precision, scale),
        _ => return Err(anyhow!("schema: invalid decimal type for '{}'", expected.name)),
    };
    let message = format!(
        "schema: value does not match decimal({}, {}) for '{}'",
        precision, scale, expected.name
    );
    let precision_usize = precision as usize;
    let scale_usize = scale as usize;
    let (values, has_errors) = cast_from_utf8_values(
        expected,
        array,
        row_errors,
        &message,
        |value| {
            let (raw, digits, value_scale) = parse_decimal_strict(value)?;
            if digits > precision_usize || value_scale > scale_usize {
                return None;
            }
            let scale_diff = scale_usize.saturating_sub(value_scale);
            let factor = pow10_i128(scale_diff)?;
            raw.checked_mul(factor)
        },
    )?;
    let array = Decimal128Array::from(values)
        .with_precision_and_scale(precision, scale as i8)?;
    Ok((std::sync::Arc::new(array), has_errors))
}

fn cast_from_utf8_values<T, F>(
    expected: &ColumnDef,
    array: &ArrayRef,
    row_errors: &mut [String],
    message: &str,
    mut parse: F,
) -> AnyhowResult<(Vec<Option<T>>, bool)>
where
    F: FnMut(&str) -> Option<T>,
{
    let mut values = Vec::with_capacity(array.len());
    let mut has_errors = false;

    match array.data_type() {
        ArrowDataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("column '{}' is not Utf8", expected.name))?;
            for row in 0..arr.len() {
                if arr.is_null(row) {
                    values.push(None);
                    continue;
                }
                let value = arr.value(row);
                if value.is_empty() {
                    values.push(None);
                    continue;
                }
                if let Some(parsed) = parse(value) {
                    values.push(Some(parsed));
                } else {
                    values.push(None);
                    append_error(&mut row_errors[row], message);
                    has_errors = true;
                }
            }
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .ok_or_else(|| anyhow!("column '{}' is not LargeUtf8", expected.name))?;
            for row in 0..arr.len() {
                if arr.is_null(row) {
                    values.push(None);
                    continue;
                }
                let value = arr.value(row);
                if value.is_empty() {
                    values.push(None);
                    continue;
                }
                if let Some(parsed) = parse(value) {
                    values.push(Some(parsed));
                } else {
                    values.push(None);
                    append_error(&mut row_errors[row], message);
                    has_errors = true;
                }
            }
        }
        _ => {
            return Err(anyhow!(
                "column '{}' is not Utf8 or LargeUtf8",
                expected.name
            ))
        }
    }

    Ok((values, has_errors))
}

fn parse_decimal_strict(value: &str) -> Option<(i128, usize, usize)> {
    let mut sign = 1i128;
    let mut total_digits = 0usize;
    let mut scale = 0usize;
    let mut saw_dot = false;
    let mut saw_digit = false;
    let mut result: i128 = 0;

    for (idx, ch) in value.chars().enumerate() {
        if ch == '+' || ch == '-' {
            if idx != 0 {
                return None;
            }
            if ch == '-' {
                sign = -1;
            }
            continue;
        }
        if ch == '.' {
            if saw_dot {
                return None;
            }
            saw_dot = true;
            continue;
        }
        if ch.is_ascii_digit() {
            saw_digit = true;
            total_digits += 1;
            if saw_dot {
                scale += 1;
            }
            let digit = i128::from(ch.to_digit(10)? as u8);
            result = result.checked_mul(10)?.checked_add(digit)?;
            continue;
        }
        return None;
    }

    if !saw_digit {
        None
    } else {
        Some((result * sign, total_digits, scale))
    }
}

fn pow10_i128(exp: usize) -> Option<i128> {
    let mut value: i128 = 1;
    for _ in 0..exp {
        value = value.checked_mul(10)?;
    }
    Some(value)
}

fn merge_error_column(batch: &RecordBatch, new_errors: &[String]) -> AnyhowResult<RecordBatch> {
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

fn error_value(array: &ArrayRef, row: usize) -> AnyhowResult<Option<String>> {
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
    use arrow::array::{Date32Array, Decimal128Array, Int64Array, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{Field, Schema, TimeUnit};
    use std::sync::Arc;

    fn schema_def(columns: Vec<SchemaColumnSpec>) -> SchemaDef {
        schema_def_from_definition(&SchemaDefinition { columns }).unwrap()
    }

    #[test]
    fn test_schema_def_empty_columns_rejected() {
        let err = schema_def_from_definition(&SchemaDefinition { columns: vec![] }).unwrap_err();
        assert!(err.to_string().contains("schema_def.columns is empty"));
    }

    #[test]
    fn test_schema_validation_string_hardfail() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "id".to_string(),
            data_type: SchemaDataType::Int64,
            nullable: false,
            format: None,
        }]);
        let ids = StringArray::from(vec![Some("1"), Some("bad")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "id",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let err = validate_record_batch(&batch, &schema, "output").unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_schema_validation_nullability() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "id".to_string(),
            data_type: SchemaDataType::Int64,
            nullable: false,
            format: None,
        }]);
        let ids = Int64Array::from(vec![Some(1), None]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "id",
                ArrowDataType::Int64,
                true,
            )])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
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
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "id".to_string(),
            data_type: SchemaDataType::Int64,
            nullable: true,
            format: None,
        }]);
        let ids = arrow::array::BooleanArray::from(vec![Some(true), Some(false)]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "id",
                ArrowDataType::Boolean,
                true,
            )])),
            vec![Arc::new(ids) as ArrayRef],
        )
        .unwrap();

        let err = validate_record_batch(&batch, &schema, "output").unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_schema_validation_format_date_string() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "date".to_string(),
            data_type: SchemaDataType::Date,
            nullable: true,
            format: Some("%Y-%m-%d".to_string()),
        }]);
        let dates = StringArray::from(vec![Some("2024-01-15"), Some("01/15/2024")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "date",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(dates) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
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
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "created_at".to_string(),
            data_type: SchemaDataType::Timestamp,
            nullable: true,
            format: Some("%Y-%m-%d %H:%M:%S".to_string()),
        }]);
        let values = StringArray::from(vec![
            Some("2024-01-15 10:30:00"),
            Some("2024-01-16 09:00:00"),
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "created_at",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(values) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        assert!(validated.schema().index_of("_cf_row_error").is_err());
    }

    #[test]
    fn test_timezone_alias_matching() {
        assert!(eq_tz("UTC", "Etc/UTC"));
        assert!(eq_tz("gmt", "UTC"));
        assert!(eq_tz("America/New_York", "america/new_york"));
    }

    #[test]
    fn test_timestamp_tz_explicit_timezone_required() {
        // TimestampTz should only match Timestamp with explicit timezone
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "UTC".to_string(),
            },
            nullable: true,
            format: None,
        }]);
        assert!(matches!(
            schema.columns[0].data_type,
            SchemaDataType::TimestampTz { ref tz } if tz == "UTC"
        ));

        // Timestamp without TZ should fail
        let ts_no_tz =
            arrow::array::TimestampMicrosecondArray::from(vec![Some(1000000), Some(2000000)]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                true,
            )])),
            vec![Arc::new(ts_no_tz) as ArrayRef],
        )
        .unwrap();
        let err = validate_record_batch(&batch, &schema, "output").unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_timestamp_tz_with_matching_timezone() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "UTC".to_string(),
            },
            nullable: true,
            format: None,
        }]);

        // Timestamp WITH matching TZ should pass
        let ts_with_tz =
            arrow::array::TimestampMicrosecondArray::from(vec![Some(1000000), Some(2000000)])
                .with_timezone("UTC");
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Timestamp(
                    arrow::datatypes::TimeUnit::Microsecond,
                    Some("UTC".into()),
                ),
                true,
            )])),
            vec![Arc::new(ts_with_tz) as ArrayRef],
        )
        .unwrap();
        let result = validate_record_batch(&batch, &schema, "output");
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_timestamp_tz_mismatched_timezone() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "America/New_York".to_string(),
            },
            nullable: true,
            format: None,
        }]);

        // Timestamp with different TZ should fail
        let ts_utc =
            arrow::array::TimestampMicrosecondArray::from(vec![Some(1000000)]).with_timezone("UTC");
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Timestamp(
                    arrow::datatypes::TimeUnit::Microsecond,
                    Some("UTC".into()),
                ),
                true,
            )])),
            vec![Arc::new(ts_utc) as ArrayRef],
        )
        .unwrap();
        let err = validate_record_batch(&batch, &schema, "output").unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_decimal_precision_scale_validation() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "price".to_string(),
            data_type: SchemaDataType::Decimal {
                precision: 38,
                scale: 10,
            },
            nullable: true,
            format: None,
        }]);

        // Decimal with matching precision/scale should pass
        let decimal_array = arrow::array::Decimal128Array::from(vec![Some(123456789012345678i128)])
            .with_precision_and_scale(38, 10)
            .unwrap();
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "price",
                ArrowDataType::Decimal128(38, 10),
                true,
            )])),
            vec![Arc::new(decimal_array) as ArrayRef],
        )
        .unwrap();
        let result = validate_record_batch(&batch, &schema, "output");
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_decimal_precision_mismatch() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "price".to_string(),
            data_type: SchemaDataType::Decimal {
                precision: 38,
                scale: 10,
            },
            nullable: true,
            format: None,
        }]);

        // Decimal with different precision should fail
        let decimal_array = arrow::array::Decimal128Array::from(vec![Some(1234567890i128)])
            .with_precision_and_scale(10, 2)
            .unwrap();
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "price",
                ArrowDataType::Decimal128(10, 2),
                true,
            )])),
            vec![Arc::new(decimal_array) as ArrayRef],
        )
        .unwrap();
        let err = validate_record_batch(&batch, &schema, "output").unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn test_timestamp_tz_string_with_format() {
        // TimestampTz can be represented as string with format for validation
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "UTC".to_string(),
            },
            nullable: true,
            format: Some("%Y-%m-%dT%H:%M:%S%z".to_string()),
        }]);

        // Valid RFC3339 format should pass
        let ts_strings = StringArray::from(vec![
            Some("2024-01-15T10:30:00+00:00"),
            Some("2024-01-16T09:00:00+00:00"),
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(ts_strings) as ArrayRef],
        )
        .unwrap();
        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        // Should not have errors
        assert!(validated.schema().index_of("_cf_row_error").is_err());
    }

    #[test]
    fn test_timestamp_tz_string_format_invalid() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "UTC".to_string(),
            },
            nullable: true,
            format: Some("%Y-%m-%dT%H:%M:%S%z".to_string()),
        }]);

        // Invalid format should quarantine
        let ts_strings = StringArray::from(vec![
            Some("2024-01-15 10:30:00"),       // Missing timezone offset
            Some("2024-01-16T09:00:00+00:00"), // Valid
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(ts_strings) as ArrayRef],
        )
        .unwrap();
        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.value(0).contains("format"));
        assert!(error_col.is_null(1));
    }

    #[test]
    fn test_schema_cast_date_string_to_date32() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "date".to_string(),
            data_type: SchemaDataType::Date,
            nullable: true,
            format: Some("%Y-%m-%d".to_string()),
        }]);
        let dates = StringArray::from(vec![Some("2024-01-15"), Some("bad")]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "date",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(dates) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        assert_eq!(validated.schema().field(0).data_type(), &ArrowDataType::Date32);
        let date_array = validated
            .column(0)
            .as_any()
            .downcast_ref::<Date32Array>()
            .unwrap();
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2024, 1, 15)
            .unwrap()
            .signed_duration_since(epoch)
            .num_days() as i32;
        assert_eq!(date_array.value(0), expected);
        assert!(date_array.is_null(1));

        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.value(1).contains("format"));
    }

    #[test]
    fn test_schema_cast_decimal_string_quarantine() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "amount".to_string(),
            data_type: SchemaDataType::Decimal {
                precision: 10,
                scale: 2,
            },
            nullable: true,
            format: None,
        }]);
        let values = StringArray::from(vec![
            Some("12.30"),
            Some("12.345"),
            Some("bad"),
            Some("12"),
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "amount",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(values) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        assert_eq!(
            validated.schema().field(0).data_type(),
            &ArrowDataType::Decimal128(10, 2)
        );
        let decimal_array = validated
            .column(0)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap();
        assert_eq!(decimal_array.value(0), 1_230);
        assert!(decimal_array.is_null(1));
        assert!(decimal_array.is_null(2));
        assert_eq!(decimal_array.value(3), 1_200);

        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.value(1).contains("decimal"));
        assert!(error_col.value(2).contains("decimal"));
    }

    #[test]
    fn test_schema_cast_timestamp_tz_string_to_arrow_type() {
        let schema = schema_def(vec![SchemaColumnSpec {
            name: "ts".to_string(),
            data_type: SchemaDataType::TimestampTz {
                tz: "UTC".to_string(),
            },
            nullable: true,
            format: Some("%Y-%m-%dT%H:%M:%S%z".to_string()),
        }]);
        let values = StringArray::from(vec![
            Some("2024-01-15T10:30:00+00:00"),
            Some("2024-01-15T10:30:00+01:00"),
        ]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "ts",
                ArrowDataType::Utf8,
                true,
            )])),
            vec![Arc::new(values) as ArrayRef],
        )
        .unwrap();

        let validated = validate_record_batch(&batch, &schema, "output").unwrap();
        assert_eq!(
            validated.schema().field(0).data_type(),
            &ArrowDataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
        );
        let ts_array = validated
            .column(0)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert!(ts_array.is_null(1));

        let error_idx = validated.schema().index_of("_cf_row_error").unwrap();
        let error_col = validated
            .column(error_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert!(error_col.value(1).contains("format"));
    }
}
