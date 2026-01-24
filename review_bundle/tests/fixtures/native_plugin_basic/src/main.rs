use anyhow::Result;
use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use blake3::Hasher;
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;

#[derive(Debug, Serialize)]
struct SchemaColumnSpec {
    name: String,
    data_type: String,
    nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Debug, Serialize)]
struct SchemaDefinition {
    columns: Vec<SchemaColumnSpec>,
}

fn schema_hash(def: &SchemaDefinition) -> Result<String> {
    let json = serde_json::to_string(def)?;
    let mut hasher = Hasher::new();
    hasher.update(json.as_bytes());
    hasher.update(&[0x1f]);
    Ok(hasher.finalize().to_hex().to_string())
}

fn main() -> Result<()> {
    let schema_def = SchemaDefinition {
        columns: vec![
            SchemaColumnSpec {
                name: "value".to_string(),
                data_type: "int64".to_string(),
                nullable: false,
                format: None,
            },
            SchemaColumnSpec {
                name: "message".to_string(),
                data_type: "string".to_string(),
                nullable: false,
                format: None,
            },
        ],
    };
    let hash = schema_hash(&schema_def)?;

    eprintln!(
        r#"{{"type":"hello","protocol":"0.1","parser_id":"native_plugin_basic","parser_version":"0.1.0","capabilities":{{"multi_output":false}}}}"#
    );
    eprintln!(
        r#"{{"type":"output_begin","output":"events","schema_hash":"{}","stream_index":0}}"#,
        hash
    );

    let schema = Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new("message", DataType::Utf8, false),
    ]);
    let batch = RecordBatch::try_new(
        Arc::new(schema.clone()),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])),
            Arc::new(StringArray::from(vec!["one", "two", "three"])),
        ],
    )?;

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, &schema)?;
    writer.write(&batch)?;
    writer.finish()?;
    handle.flush()?;

    eprintln!(
        r#"{{"type":"output_end","output":"events","rows_emitted":3,"stream_index":0}}"#
    );
    Ok(())
}
