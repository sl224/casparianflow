use anyhow::{Context, Result};
use arrow::array::{
    BinaryArray, BooleanArray, Int64Array, StringArray, TimestampMicrosecondArray,
    TimestampMicrosecondBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use blake3::Hasher;
use evtx::{EvtxParser, ParserSettings};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const PARSER_ID: &str = "evtx_native";
const PARSER_VERSION: &str = "0.1.0";
const PROTOCOL_VERSION: &str = "0.1";

const OUTPUT_EVENTS: &str = "evtx_events";
const OUTPUT_EVENTDATA: &str = "evtx_eventdata_kv";
const OUTPUT_USERDATA: &str = "evtx_userdata_kv";
const OUTPUT_ANNOTATIONS: &str = "evtx_record_annotations";
const OUTPUT_FILES: &str = "evtx_files";

const NOTE_TYPE_FIELD_VARIANT: &str = "field_shape_variant";
const NOTE_TYPE_CONVERSION_FAILED: &str = "conversion_failed";
const NOTE_TYPE_PARSE_WARNING: &str = "parse_warning";

const SEVERITY_INFO: i64 = 1;
const SEVERITY_WARN: i64 = 3;
const SEVERITY_ERROR: i64 = 4;

fn main() -> Result<()> {
    let input_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Usage: evtx_native <path-to-evtx>"))?;

    let schema_dir = resolve_schema_dir()?;
    let schema_hashes = load_schema_hashes(&schema_dir)?;

    let file_meta = FileMetadata::from_path(&input_path)?;

    emit_hello()?;

    let mut stream_index = 0usize;

    let event_schema = schema_evtx_events();
    let event_hash = lookup_schema_hash(&schema_hashes, OUTPUT_EVENTS)?;
    emit_output_begin(OUTPUT_EVENTS, event_hash, stream_index)?;
    let event_rows = write_evtx_events(&input_path, &event_schema)?;
    emit_output_end(OUTPUT_EVENTS, event_rows, stream_index)?;
    stream_index += 1;

    let eventdata_schema = schema_evtx_kv();
    let eventdata_hash = lookup_schema_hash(&schema_hashes, OUTPUT_EVENTDATA)?;
    emit_output_begin(OUTPUT_EVENTDATA, eventdata_hash, stream_index)?;
    let eventdata_rows = write_evtx_eventdata(&input_path, &eventdata_schema)?;
    emit_output_end(OUTPUT_EVENTDATA, eventdata_rows, stream_index)?;
    stream_index += 1;

    let userdata_schema = schema_evtx_kv();
    let userdata_hash = lookup_schema_hash(&schema_hashes, OUTPUT_USERDATA)?;
    emit_output_begin(OUTPUT_USERDATA, userdata_hash, stream_index)?;
    let userdata_rows = write_evtx_userdata(&input_path, &userdata_schema)?;
    emit_output_end(OUTPUT_USERDATA, userdata_rows, stream_index)?;
    stream_index += 1;

    let annotations_schema = schema_evtx_annotations();
    let annotations_hash = lookup_schema_hash(&schema_hashes, OUTPUT_ANNOTATIONS)?;
    emit_output_begin(OUTPUT_ANNOTATIONS, annotations_hash, stream_index)?;
    let annotations_rows = write_evtx_annotations(&input_path, &annotations_schema)?;
    emit_output_end(OUTPUT_ANNOTATIONS, annotations_rows, stream_index)?;
    stream_index += 1;

    let files_schema = schema_evtx_files();
    let files_hash = lookup_schema_hash(&schema_hashes, OUTPUT_FILES)?;
    emit_output_begin(OUTPUT_FILES, files_hash, stream_index)?;
    let files_rows = write_evtx_files(&files_schema, &file_meta, event_rows)?;
    emit_output_end(OUTPUT_FILES, files_rows, stream_index)?;

    Ok(())
}

fn resolve_schema_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("EVTX_SCHEMA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let exe = std::env::current_exe().context("Failed to resolve executable path")?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Executable has no parent directory"))?;
    let schema_dir = exe_dir.parent().unwrap_or(exe_dir).join("schemas");
    Ok(schema_dir)
}

fn lookup_schema_hash<'a>(schemas: &'a BTreeMap<String, String>, output: &str) -> Result<&'a str> {
    schemas
        .get(output)
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing schema hash for output '{}'", output))
}

fn emit_hello() -> Result<()> {
    let frame = serde_json::json!({
        "type": "hello",
        "protocol": PROTOCOL_VERSION,
        "parser_id": PARSER_ID,
        "parser_version": PARSER_VERSION,
        "capabilities": { "multi_output": true }
    });
    emit_frame(&frame)
}

fn emit_output_begin(output: &str, schema_hash: &str, stream_index: usize) -> Result<()> {
    let frame = serde_json::json!({
        "type": "output_begin",
        "output": output,
        "schema_hash": schema_hash,
        "stream_index": stream_index
    });
    emit_frame(&frame)
}

fn emit_output_end(output: &str, rows_emitted: u64, stream_index: usize) -> Result<()> {
    let frame = serde_json::json!({
        "type": "output_end",
        "output": output,
        "rows_emitted": rows_emitted,
        "stream_index": stream_index
    });
    emit_frame(&frame)
}

fn emit_frame(frame: &serde_json::Value) -> Result<()> {
    eprintln!("{}", serde_json::to_string(frame)?);
    Ok(())
}

fn parser_settings() -> ParserSettings {
    ParserSettings::default()
        .num_threads(1)
        .separate_json_attributes(true)
        .indent(false)
}

fn load_schema_hashes(schema_dir: &Path) -> Result<BTreeMap<String, String>> {
    if !schema_dir.exists() {
        anyhow::bail!("Schema directory not found: {}", schema_dir.display());
    }
    let mut hashes = BTreeMap::new();
    for entry in fs::read_dir(schema_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid schema filename"))?;
        if !file_name.ends_with(".schema.json") {
            continue;
        }
        let output_name = file_name.trim_end_matches(".schema.json");
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let value: Value = serde_json::from_str(&content)
            .with_context(|| format!("Invalid schema JSON in {}", path.display()))?;
        let canonical = serde_json::to_string(&value)?;
        let hash = schema_hash(&canonical);
        hashes.insert(output_name.to_string(), hash);
    }
    if hashes.is_empty() {
        anyhow::bail!("No schema files found in {}", schema_dir.display());
    }
    Ok(hashes)
}

fn schema_hash(canonical_json: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(canonical_json.as_bytes());
    hasher.update(&[0x1f]);
    hasher.finalize().to_hex().to_string()
}

fn schema_evtx_events() -> Schema {
    Schema::new(vec![
        Field::new("event_record_id", DataType::Int64, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("provider_name", DataType::Utf8, true),
        Field::new("provider_guid", DataType::Utf8, true),
        Field::new("provider_event_source_name", DataType::Utf8, true),
        Field::new("channel", DataType::Utf8, true),
        Field::new("computer", DataType::Utf8, true),
        Field::new("event_id", DataType::Int64, false),
        Field::new("event_qualifiers", DataType::Int64, true),
        Field::new("version", DataType::Int64, true),
        Field::new("level", DataType::Int64, true),
        Field::new("task", DataType::Int64, true),
        Field::new("opcode", DataType::Int64, true),
        Field::new("keywords_hex", DataType::Utf8, true),
        Field::new("execution_process_id", DataType::Int64, true),
        Field::new("execution_thread_id", DataType::Int64, true),
        Field::new("security_user_id", DataType::Utf8, true),
        Field::new("correlation_activity_id", DataType::Utf8, true),
        Field::new("correlation_related_activity_id", DataType::Utf8, true),
        Field::new("raw_event_json", DataType::Utf8, true),
        Field::new("raw_event_xml", DataType::Utf8, true),
    ])
}

fn schema_evtx_kv() -> Schema {
    Schema::new(vec![
        Field::new("event_record_id", DataType::Int64, false),
        Field::new("section", DataType::Utf8, false),
        Field::new("key", DataType::Utf8, false),
        Field::new("idx", DataType::Int64, true),
        Field::new("value_raw", DataType::Utf8, true),
        Field::new("value_type", DataType::Utf8, true),
        Field::new("value_int64", DataType::Int64, true),
        Field::new("value_bool", DataType::Boolean, true),
        Field::new(
            "value_ts",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            true,
        ),
        Field::new("value_bytes", DataType::Binary, true),
    ])
}

fn schema_evtx_annotations() -> Schema {
    Schema::new(vec![
        Field::new("event_record_id", DataType::Int64, false),
        Field::new("note_type", DataType::Utf8, false),
        Field::new("note_key", DataType::Utf8, false),
        Field::new("note_value", DataType::Utf8, true),
        Field::new("severity", DataType::Int64, false),
    ])
}

fn schema_evtx_files() -> Schema {
    Schema::new(vec![
        Field::new("file_path", DataType::Utf8, true),
        Field::new("file_hash", DataType::Utf8, false),
        Field::new("file_size", DataType::Int64, false),
        Field::new(
            "file_mtime",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            true,
        ),
        Field::new("record_count", DataType::Int64, true),
        Field::new("parser_output_version", DataType::Utf8, false),
    ])
}

fn write_evtx_events(input_path: &Path, schema: &Schema) -> Result<u64> {
    let mut parser = EvtxParser::from_path(input_path)?.with_configuration(parser_settings());

    let mut builder = EventsBuilder::new(4096);
    let mut rows_emitted = 0u64;

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, schema)?;

    for record in parser.records_json_value() {
        let record = match record {
            Ok(record) => record,
            Err(_) => {
                continue;
            }
        };

        let record_id = match u64_to_i64(record.event_record_id) {
            Some(id) => id,
            None => continue,
        };
        let timestamp_us = record.timestamp.as_microsecond();
        let raw_json = serde_json::to_string(&record.data)?;

        let event_obj = match extract_event_object(&record.data) {
            Some(obj) => obj,
            None => {
                continue;
            }
        };

        let mut notes = Vec::new();
        let system = extract_system_fields(event_obj, record_id, &mut notes);

        builder.push(EventRow {
            event_record_id: record_id,
            timestamp_us,
            provider_name: system.provider_name,
            provider_guid: system.provider_guid,
            provider_event_source_name: system.provider_event_source_name,
            channel: system.channel,
            computer: system.computer,
            event_id: system.event_id,
            event_qualifiers: system.event_qualifiers,
            version: system.version,
            level: system.level,
            task: system.task,
            opcode: system.opcode,
            keywords_hex: system.keywords_hex,
            execution_process_id: system.execution_process_id,
            execution_thread_id: system.execution_thread_id,
            security_user_id: system.security_user_id,
            correlation_activity_id: system.correlation_activity_id,
            correlation_related_activity_id: system.correlation_related_activity_id,
            raw_event_json: Some(raw_json),
            raw_event_xml: None,
        });

        if builder.len() >= 4096 {
            let batch = builder.take_batch(schema)?;
            rows_emitted += batch.num_rows() as u64;
            writer.write(&batch)?;
        }
    }

    if !builder.is_empty() {
        let batch = builder.take_batch(schema)?;
        rows_emitted += batch.num_rows() as u64;
        writer.write(&batch)?;
    }

    writer.finish()?;
    handle.flush()?;
    Ok(rows_emitted)
}

fn write_evtx_eventdata(input_path: &Path, schema: &Schema) -> Result<u64> {
    let mut parser = EvtxParser::from_path(input_path)?.with_configuration(parser_settings());

    let mut builder = KvBuilder::new(4096);
    let mut rows_emitted = 0u64;

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, schema)?;

    for record in parser.records_json_value() {
        let record = match record {
            Ok(record) => record,
            Err(_) => {
                continue;
            }
        };
        let record_id = match u64_to_i64(record.event_record_id) {
            Some(id) => id,
            None => continue,
        };
        let event_obj = match extract_event_object(&record.data) {
            Some(obj) => obj,
            None => continue,
        };
        let mut notes = Vec::new();
        extract_eventdata_rows(event_obj, record_id, &mut builder, &mut notes);

        if builder.len() >= 4096 {
            let batch = builder.take_batch(schema)?;
            rows_emitted += batch.num_rows() as u64;
            writer.write(&batch)?;
        }
    }

    if !builder.is_empty() {
        let batch = builder.take_batch(schema)?;
        rows_emitted += batch.num_rows() as u64;
        writer.write(&batch)?;
    }

    writer.finish()?;
    handle.flush()?;
    Ok(rows_emitted)
}

fn write_evtx_userdata(input_path: &Path, schema: &Schema) -> Result<u64> {
    let mut parser = EvtxParser::from_path(input_path)?.with_configuration(parser_settings());

    let mut builder = KvBuilder::new(4096);
    let mut rows_emitted = 0u64;

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, schema)?;

    for record in parser.records_json_value() {
        let record = match record {
            Ok(record) => record,
            Err(_) => {
                continue;
            }
        };
        let record_id = match u64_to_i64(record.event_record_id) {
            Some(id) => id,
            None => continue,
        };
        let event_obj = match extract_event_object(&record.data) {
            Some(obj) => obj,
            None => continue,
        };
        let mut notes = Vec::new();
        extract_userdata_rows(event_obj, record_id, &mut builder, &mut notes);

        if builder.len() >= 4096 {
            let batch = builder.take_batch(schema)?;
            rows_emitted += batch.num_rows() as u64;
            writer.write(&batch)?;
        }
    }

    if !builder.is_empty() {
        let batch = builder.take_batch(schema)?;
        rows_emitted += batch.num_rows() as u64;
        writer.write(&batch)?;
    }

    writer.finish()?;
    handle.flush()?;
    Ok(rows_emitted)
}

fn write_evtx_annotations(input_path: &Path, schema: &Schema) -> Result<u64> {
    let mut parser = EvtxParser::from_path(input_path)?.with_configuration(parser_settings());

    let mut builder = AnnotationBuilder::new(1024);
    let mut rows_emitted = 0u64;

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, schema)?;

    for record in parser.records_json_value() {
        match record {
            Ok(record) => {
                let record_id = match u64_to_i64(record.event_record_id) {
                    Some(id) => id,
                    None => continue,
                };
                let event_obj = match extract_event_object(&record.data) {
                    Some(obj) => obj,
                    None => {
                        builder.push(AnnotationRow {
                            event_record_id: record_id,
                            note_type: NOTE_TYPE_PARSE_WARNING.to_string(),
                            note_key: "Event".to_string(),
                            note_value: Some("Missing Event object".to_string()),
                            severity: SEVERITY_WARN,
                        });
                        continue;
                    }
                };
                let mut notes = Vec::new();
                let _ = extract_system_fields(event_obj, record_id, &mut notes);
                let mut sink = KvBuilder::new(0);
                extract_eventdata_rows(event_obj, record_id, &mut sink, &mut notes);
                extract_userdata_rows(event_obj, record_id, &mut sink, &mut notes);

                for note in notes {
                    builder.push(note);
                }
            }
            Err(err) => {
                if let Some(record_id) = extract_record_id_from_error(&err) {
                    builder.push(AnnotationRow {
                        event_record_id: record_id,
                        note_type: NOTE_TYPE_PARSE_WARNING.to_string(),
                        note_key: "record".to_string(),
                        note_value: Some(err.to_string()),
                        severity: SEVERITY_ERROR,
                    });
                }
            }
        }

        if builder.len() >= 1024 {
            let batch = builder.take_batch(schema)?;
            rows_emitted += batch.num_rows() as u64;
            writer.write(&batch)?;
        }
    }

    if !builder.is_empty() {
        let batch = builder.take_batch(schema)?;
        rows_emitted += batch.num_rows() as u64;
        writer.write(&batch)?;
    }

    writer.finish()?;
    handle.flush()?;
    Ok(rows_emitted)
}

fn write_evtx_files(schema: &Schema, meta: &FileMetadata, record_count: u64) -> Result<u64> {
    let mut builder = FilesBuilder::new(1);
    let include_path = std::env::var("EVTX_INCLUDE_PATH")
        .ok()
        .map(|value| value != "0")
        .unwrap_or(true);

    builder.push(FileRow {
        file_path: if include_path {
            Some(meta.path.clone())
        } else {
            None
        },
        file_hash: meta.hash.clone(),
        file_size: meta.size,
        file_mtime: meta.mtime_us,
        record_count: i64::try_from(record_count).ok(),
        parser_output_version: PARSER_VERSION.to_string(),
    });

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut writer = StreamWriter::try_new(&mut handle, schema)?;
    let batch = builder.take_batch(schema)?;
    writer.write(&batch)?;
    writer.finish()?;
    handle.flush()?;
    Ok(batch.num_rows() as u64)
}

fn extract_record_id_from_error(err: &evtx::err::EvtxError) -> Option<i64> {
    match err {
        evtx::err::EvtxError::FailedToParseRecord { record_id, .. } => u64_to_i64(*record_id),
        _ => None,
    }
}

fn extract_event_object<'a>(value: &'a Value) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(map) => {
            if let Some(Value::Object(event)) = map.get("Event") {
                Some(event)
            } else if map.contains_key("System") {
                Some(map)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_system_fields(
    event_obj: &Map<String, Value>,
    record_id: i64,
    notes: &mut Vec<AnnotationRow>,
) -> SystemFields {
    let system = match event_obj.get("System") {
        Some(Value::Object(map)) => map,
        _ => return SystemFields::default(),
    };

    let provider_attr = get_attr_map(system, "Provider");
    let provider_name = provider_attr.and_then(|attrs| value_to_string(attrs.get("Name")));
    let provider_guid = provider_attr
        .and_then(|attrs| value_to_string(attrs.get("Guid")))
        .map(normalize_guid);
    let provider_event_source_name =
        provider_attr.and_then(|attrs| value_to_string(attrs.get("EventSourceName")));

    let channel = value_to_string(system.get("Channel"));
    let computer = value_to_string(system.get("Computer"));

    let event_id = parse_i64_value(system.get("EventID"), "System.EventID", record_id, notes);

    let qualifiers = system
        .get("EventID_attributes")
        .and_then(|val| match val {
            Value::Object(obj) => obj.get("Qualifiers"),
            _ => Some(val),
        })
        .or_else(|| {
            system
                .get("EventID")
                .and_then(|val| val.get("#attributes"))
                .and_then(|attrs| attrs.get("Qualifiers"))
        });
    let event_qualifiers =
        parse_i64_value(qualifiers, "System.EventID.Qualifiers", record_id, notes);

    let version = parse_i64_value(system.get("Version"), "System.Version", record_id, notes);
    let level = parse_i64_value(system.get("Level"), "System.Level", record_id, notes);
    let task = parse_i64_value(system.get("Task"), "System.Task", record_id, notes);
    let opcode = parse_i64_value(system.get("Opcode"), "System.Opcode", record_id, notes);

    let keywords_hex = value_to_string(system.get("Keywords"));

    let execution_attr = get_attr_map(system, "Execution");
    let execution_process_id = parse_i64_value(
        execution_attr.and_then(|attrs| attrs.get("ProcessID")),
        "System.Execution.ProcessID",
        record_id,
        notes,
    );
    let execution_thread_id = parse_i64_value(
        execution_attr.and_then(|attrs| attrs.get("ThreadID")),
        "System.Execution.ThreadID",
        record_id,
        notes,
    );

    let security_attr = get_attr_map(system, "Security");
    let security_user_id = security_attr.and_then(|attrs| value_to_string(attrs.get("UserID")));

    let correlation_attr = get_attr_map(system, "Correlation");
    let correlation_activity_id =
        correlation_attr.and_then(|attrs| value_to_string(attrs.get("ActivityID")));
    let correlation_related_activity_id =
        correlation_attr.and_then(|attrs| value_to_string(attrs.get("RelatedActivityID")));

    SystemFields {
        provider_name,
        provider_guid,
        provider_event_source_name,
        channel,
        computer,
        event_id,
        event_qualifiers,
        version,
        level,
        task,
        opcode,
        keywords_hex,
        execution_process_id,
        execution_thread_id,
        security_user_id,
        correlation_activity_id,
        correlation_related_activity_id,
    }
}

fn extract_eventdata_rows(
    event_obj: &Map<String, Value>,
    record_id: i64,
    builder: &mut KvBuilder,
    notes: &mut Vec<AnnotationRow>,
) {
    let eventdata = match event_obj.get("EventData") {
        Some(Value::Object(map)) => map,
        Some(Value::Null) | None => return,
        Some(other) => {
            notes.push(AnnotationRow {
                event_record_id: record_id,
                note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                note_key: "Event.EventData".to_string(),
                note_value: Some(other.to_string()),
                severity: SEVERITY_WARN,
            });
            return;
        }
    };

    if let Some(data_val) = eventdata.get("Data") {
        if let Some(list) = extract_text_list(data_val) {
            for (idx, item) in list.into_iter().enumerate() {
                let parsed = classify_value(&item);
                let idx_value = match i64::try_from(idx) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                builder.push(KvRow {
                    event_record_id: record_id,
                    section: "EventData".to_string(),
                    key: "Data".to_string(),
                    idx: Some(idx_value),
                    value_raw: Some(item),
                    value_type: parsed.value_type,
                    value_int64: parsed.value_int64,
                    value_bool: parsed.value_bool,
                    value_ts: parsed.value_ts,
                    value_bytes: None,
                });
            }
        } else {
            let raw = value_to_string(Some(data_val)).or_else(|| json_string(data_val));
            let parsed = raw.as_deref().map(classify_value).unwrap_or_default();
            builder.push(KvRow {
                event_record_id: record_id,
                section: "EventData".to_string(),
                key: "Data".to_string(),
                idx: None,
                value_raw: raw,
                value_type: parsed.value_type,
                value_int64: parsed.value_int64,
                value_bool: parsed.value_bool,
                value_ts: parsed.value_ts,
                value_bytes: None,
            });
        }
    }

    let mut keys = eventdata.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        if key == "Data" {
            continue;
        }
        let value = &eventdata[&key];
        match value {
            Value::Null => {
                builder.push(KvRow {
                    event_record_id: record_id,
                    section: "EventData".to_string(),
                    key: key.clone(),
                    idx: None,
                    value_raw: None,
                    value_type: Some("empty".to_string()),
                    value_int64: None,
                    value_bool: None,
                    value_ts: None,
                    value_bytes: None,
                });
            }
            Value::Array(_) => {
                let raw = json_string(value);
                builder.push(KvRow {
                    event_record_id: record_id,
                    section: "EventData".to_string(),
                    key: key.clone(),
                    idx: None,
                    value_raw: raw,
                    value_type: Some("json".to_string()),
                    value_int64: None,
                    value_bool: None,
                    value_ts: None,
                    value_bytes: None,
                });
                notes.push(AnnotationRow {
                    event_record_id: record_id,
                    note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                    note_key: format!("Event.EventData.{}", key),
                    note_value: Some("array_value".to_string()),
                    severity: SEVERITY_INFO,
                });
            }
            Value::Object(obj) => {
                if let Some(text) = obj.get("#text") {
                    let raw = value_to_string(Some(text)).or_else(|| json_string(text));
                    let parsed = raw.as_deref().map(classify_value).unwrap_or_default();
                    builder.push(KvRow {
                        event_record_id: record_id,
                        section: "EventData".to_string(),
                        key: key.clone(),
                        idx: None,
                        value_raw: raw,
                        value_type: parsed.value_type,
                        value_int64: parsed.value_int64,
                        value_bool: parsed.value_bool,
                        value_ts: parsed.value_ts,
                        value_bytes: None,
                    });
                    notes.push(AnnotationRow {
                        event_record_id: record_id,
                        note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                        note_key: format!("Event.EventData.{}", key),
                        note_value: Some("object_text".to_string()),
                        severity: SEVERITY_INFO,
                    });
                } else {
                    let raw = json_string(value);
                    builder.push(KvRow {
                        event_record_id: record_id,
                        section: "EventData".to_string(),
                        key: key.clone(),
                        idx: None,
                        value_raw: raw,
                        value_type: Some("json".to_string()),
                        value_int64: None,
                        value_bool: None,
                        value_ts: None,
                        value_bytes: None,
                    });
                    notes.push(AnnotationRow {
                        event_record_id: record_id,
                        note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                        note_key: format!("Event.EventData.{}", key),
                        note_value: Some("complex_value".to_string()),
                        severity: SEVERITY_INFO,
                    });
                }
            }
            _ => {
                let raw = value_to_string(Some(value));
                let parsed = raw.as_deref().map(classify_value).unwrap_or_default();
                builder.push(KvRow {
                    event_record_id: record_id,
                    section: "EventData".to_string(),
                    key: key.clone(),
                    idx: None,
                    value_raw: raw,
                    value_type: parsed.value_type,
                    value_int64: parsed.value_int64,
                    value_bool: parsed.value_bool,
                    value_ts: parsed.value_ts,
                    value_bytes: None,
                });
            }
        }
    }
}

fn extract_userdata_rows(
    event_obj: &Map<String, Value>,
    record_id: i64,
    builder: &mut KvBuilder,
    notes: &mut Vec<AnnotationRow>,
) {
    let userdata = match event_obj.get("UserData") {
        Some(Value::Object(map)) => map,
        Some(Value::Null) | None => return,
        Some(other) => {
            notes.push(AnnotationRow {
                event_record_id: record_id,
                note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                note_key: "Event.UserData".to_string(),
                note_value: Some(other.to_string()),
                severity: SEVERITY_WARN,
            });
            return;
        }
    };

    let mut keys = userdata.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        let value = &userdata[&key];
        let prefix = key.as_str();
        flatten_userdata_value(value, prefix, record_id, builder, notes, None);
    }
}

fn flatten_userdata_value(
    value: &Value,
    prefix: &str,
    record_id: i64,
    builder: &mut KvBuilder,
    notes: &mut Vec<AnnotationRow>,
    idx_override: Option<i64>,
) {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                let child = &map[&key];
                if key == "#attributes" {
                    if let Value::Object(attrs) = child {
                        let mut attr_keys = attrs.keys().cloned().collect::<Vec<_>>();
                        attr_keys.sort();
                        for attr_key in attr_keys {
                            let attr_val = &attrs[&attr_key];
                            let path = format!("{}.#attributes.{}", prefix, attr_key);
                            push_userdata_leaf(
                                &path,
                                attr_val,
                                record_id,
                                builder,
                                notes,
                                idx_override,
                            );
                        }
                    } else {
                        notes.push(AnnotationRow {
                            event_record_id: record_id,
                            note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                            note_key: format!("{}.#attributes", prefix),
                            note_value: Some(child.to_string()),
                            severity: SEVERITY_INFO,
                        });
                    }
                } else if key == "#text" {
                    let path = format!("{}.#text", prefix);
                    push_userdata_leaf(&path, child, record_id, builder, notes, idx_override);
                } else {
                    let path = format!("{}.{}", prefix, key);
                    flatten_userdata_value(child, &path, record_id, builder, notes, idx_override);
                }
            }
        }
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                let idx_value = match i64::try_from(idx) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                flatten_userdata_value(item, prefix, record_id, builder, notes, Some(idx_value));
            }
        }
        _ => {
            push_userdata_leaf(prefix, value, record_id, builder, notes, idx_override);
        }
    }
}

fn push_userdata_leaf(
    path: &str,
    value: &Value,
    record_id: i64,
    builder: &mut KvBuilder,
    _notes: &mut Vec<AnnotationRow>,
    idx: Option<i64>,
) {
    let raw = match value {
        Value::Null => None,
        Value::Array(_) | Value::Object(_) => json_string(value),
        _ => value_to_string(Some(value)),
    };
    let parsed = raw.as_deref().map(classify_value).unwrap_or_default();

    builder.push(KvRow {
        event_record_id: record_id,
        section: "UserData".to_string(),
        key: path.to_string(),
        idx,
        value_raw: raw,
        value_type: parsed.value_type,
        value_int64: parsed.value_int64,
        value_bool: parsed.value_bool,
        value_ts: parsed.value_ts,
        value_bytes: None,
    });
}

fn get_attr_map<'a>(system: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    let separate_key = format!("{}_attributes", key);
    if let Some(Value::Object(attrs)) = system.get(&separate_key) {
        return Some(attrs);
    }
    if let Some(Value::Object(obj)) = system.get(key) {
        if let Some(Value::Object(attrs)) = obj.get("#attributes") {
            return Some(attrs);
        }
    }
    None
}

fn normalize_guid(value: String) -> String {
    value.trim_matches('{').trim_matches('}').to_string()
}

fn value_to_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn json_string(value: &Value) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn parse_i64_value(
    value: Option<&Value>,
    field: &str,
    record_id: i64,
    notes: &mut Vec<AnnotationRow>,
) -> Option<i64> {
    let value = value?;
    match value {
        Value::Number(num) => {
            if let Some(v) = num.as_i64() {
                Some(v)
            } else if let Some(v) = num.as_u64() {
                match i64::try_from(v) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        notes.push(AnnotationRow {
                            event_record_id: record_id,
                            note_type: NOTE_TYPE_CONVERSION_FAILED.to_string(),
                            note_key: field.to_string(),
                            note_value: Some(num.to_string()),
                            severity: SEVERITY_ERROR,
                        });
                        None
                    }
                }
            } else {
                notes.push(AnnotationRow {
                    event_record_id: record_id,
                    note_type: NOTE_TYPE_CONVERSION_FAILED.to_string(),
                    note_key: field.to_string(),
                    note_value: Some(num.to_string()),
                    severity: SEVERITY_ERROR,
                });
                None
            }
        }
        Value::String(s) => {
            let parsed = parse_int_string(s);
            if parsed.is_some() {
                notes.push(AnnotationRow {
                    event_record_id: record_id,
                    note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                    note_key: field.to_string(),
                    note_value: Some(s.clone()),
                    severity: SEVERITY_INFO,
                });
            } else {
                notes.push(AnnotationRow {
                    event_record_id: record_id,
                    note_type: NOTE_TYPE_CONVERSION_FAILED.to_string(),
                    note_key: field.to_string(),
                    note_value: Some(s.clone()),
                    severity: SEVERITY_ERROR,
                });
            }
            parsed
        }
        Value::Object(obj) => {
            if let Some(Value::String(text)) = obj.get("#text") {
                let parsed = parse_int_string(text);
                if parsed.is_some() {
                    notes.push(AnnotationRow {
                        event_record_id: record_id,
                        note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                        note_key: field.to_string(),
                        note_value: Some(text.clone()),
                        severity: SEVERITY_INFO,
                    });
                } else {
                    notes.push(AnnotationRow {
                        event_record_id: record_id,
                        note_type: NOTE_TYPE_CONVERSION_FAILED.to_string(),
                        note_key: field.to_string(),
                        note_value: Some(text.clone()),
                        severity: SEVERITY_ERROR,
                    });
                }
                parsed
            } else {
                notes.push(AnnotationRow {
                    event_record_id: record_id,
                    note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                    note_key: field.to_string(),
                    note_value: Some(value.to_string()),
                    severity: SEVERITY_WARN,
                });
                None
            }
        }
        _ => {
            notes.push(AnnotationRow {
                event_record_id: record_id,
                note_type: NOTE_TYPE_FIELD_VARIANT.to_string(),
                note_key: field.to_string(),
                note_value: Some(value.to_string()),
                severity: SEVERITY_WARN,
            });
            None
        }
    }
}

fn parse_int_string(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).ok();
    }
    trimmed.parse::<i64>().ok()
}

fn extract_text_list(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Object(map) => match map.get("#text") {
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .filter_map(|v| value_to_string(Some(v)))
                    .collect(),
            ),
            Some(Value::String(s)) => Some(vec![s.clone()]),
            Some(other) => value_to_string(Some(other)).map(|v| vec![v]),
            None => None,
        },
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|v| value_to_string(Some(v)))
                .collect(),
        ),
        Value::String(s) => Some(vec![s.clone()]),
        _ => None,
    }
}

#[derive(Default)]
struct ParsedValue {
    value_type: Option<String>,
    value_int64: Option<i64>,
    value_bool: Option<bool>,
    value_ts: Option<i64>,
}

fn classify_value(raw: &str) -> ParsedValue {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedValue {
            value_type: Some("empty".to_string()),
            ..ParsedValue::default()
        };
    }

    if matches!(trimmed.to_ascii_lowercase().as_str(), "true" | "false") {
        let value_bool = trimmed.eq_ignore_ascii_case("true");
        return ParsedValue {
            value_type: Some("bool".to_string()),
            value_bool: Some(value_bool),
            ..ParsedValue::default()
        };
    }

    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if let Ok(value_int64) = i64::from_str_radix(hex, 16) {
            return ParsedValue {
                value_type: Some("hex".to_string()),
                value_int64: Some(value_int64),
                ..ParsedValue::default()
            };
        }
    }

    if let Ok(value_int64) = trimmed.parse::<i64>() {
        return ParsedValue {
            value_type: Some("int".to_string()),
            value_int64: Some(value_int64),
            ..ParsedValue::default()
        };
    }

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return ParsedValue {
            value_type: Some("timestamp".to_string()),
            value_ts: Some(dt.timestamp_micros()),
            ..ParsedValue::default()
        };
    }

    if trimmed.starts_with('P') {
        return ParsedValue {
            value_type: Some("duration".to_string()),
            ..ParsedValue::default()
        };
    }

    if trimmed.starts_with("S-1-") {
        return ParsedValue {
            value_type: Some("sid".to_string()),
            ..ParsedValue::default()
        };
    }

    if looks_like_guid(trimmed) {
        return ParsedValue {
            value_type: Some("guid".to_string()),
            ..ParsedValue::default()
        };
    }

    ParsedValue {
        value_type: Some("string".to_string()),
        ..ParsedValue::default()
    }
}

fn looks_like_guid(value: &str) -> bool {
    let trimmed = value.trim_matches('{').trim_matches('}');
    if trimmed.len() != 36 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let hyphen_positions = [8, 13, 18, 23];
    for (idx, byte) in bytes.iter().enumerate() {
        if hyphen_positions.contains(&idx) {
            if *byte != b'-' {
                return false;
            }
            continue;
        }
        if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

#[derive(Default)]
struct SystemFields {
    provider_name: Option<String>,
    provider_guid: Option<String>,
    provider_event_source_name: Option<String>,
    channel: Option<String>,
    computer: Option<String>,
    event_id: Option<i64>,
    event_qualifiers: Option<i64>,
    version: Option<i64>,
    level: Option<i64>,
    task: Option<i64>,
    opcode: Option<i64>,
    keywords_hex: Option<String>,
    execution_process_id: Option<i64>,
    execution_thread_id: Option<i64>,
    security_user_id: Option<String>,
    correlation_activity_id: Option<String>,
    correlation_related_activity_id: Option<String>,
}

struct EventRow {
    event_record_id: i64,
    timestamp_us: i64,
    provider_name: Option<String>,
    provider_guid: Option<String>,
    provider_event_source_name: Option<String>,
    channel: Option<String>,
    computer: Option<String>,
    event_id: Option<i64>,
    event_qualifiers: Option<i64>,
    version: Option<i64>,
    level: Option<i64>,
    task: Option<i64>,
    opcode: Option<i64>,
    keywords_hex: Option<String>,
    execution_process_id: Option<i64>,
    execution_thread_id: Option<i64>,
    security_user_id: Option<String>,
    correlation_activity_id: Option<String>,
    correlation_related_activity_id: Option<String>,
    raw_event_json: Option<String>,
    raw_event_xml: Option<String>,
}

struct EventsBuilder {
    event_record_id: Vec<i64>,
    timestamp_us: Vec<i64>,
    provider_name: Vec<Option<String>>,
    provider_guid: Vec<Option<String>>,
    provider_event_source_name: Vec<Option<String>>,
    channel: Vec<Option<String>>,
    computer: Vec<Option<String>>,
    event_id: Vec<Option<i64>>,
    event_qualifiers: Vec<Option<i64>>,
    version: Vec<Option<i64>>,
    level: Vec<Option<i64>>,
    task: Vec<Option<i64>>,
    opcode: Vec<Option<i64>>,
    keywords_hex: Vec<Option<String>>,
    execution_process_id: Vec<Option<i64>>,
    execution_thread_id: Vec<Option<i64>>,
    security_user_id: Vec<Option<String>>,
    correlation_activity_id: Vec<Option<String>>,
    correlation_related_activity_id: Vec<Option<String>>,
    raw_event_json: Vec<Option<String>>,
    raw_event_xml: Vec<Option<String>>,
}

impl EventsBuilder {
    fn new(capacity: usize) -> Self {
        Self {
            event_record_id: Vec::with_capacity(capacity),
            timestamp_us: Vec::with_capacity(capacity),
            provider_name: Vec::with_capacity(capacity),
            provider_guid: Vec::with_capacity(capacity),
            provider_event_source_name: Vec::with_capacity(capacity),
            channel: Vec::with_capacity(capacity),
            computer: Vec::with_capacity(capacity),
            event_id: Vec::with_capacity(capacity),
            event_qualifiers: Vec::with_capacity(capacity),
            version: Vec::with_capacity(capacity),
            level: Vec::with_capacity(capacity),
            task: Vec::with_capacity(capacity),
            opcode: Vec::with_capacity(capacity),
            keywords_hex: Vec::with_capacity(capacity),
            execution_process_id: Vec::with_capacity(capacity),
            execution_thread_id: Vec::with_capacity(capacity),
            security_user_id: Vec::with_capacity(capacity),
            correlation_activity_id: Vec::with_capacity(capacity),
            correlation_related_activity_id: Vec::with_capacity(capacity),
            raw_event_json: Vec::with_capacity(capacity),
            raw_event_xml: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, row: EventRow) {
        self.event_record_id.push(row.event_record_id);
        self.timestamp_us.push(row.timestamp_us);
        self.provider_name.push(row.provider_name);
        self.provider_guid.push(row.provider_guid);
        self.provider_event_source_name
            .push(row.provider_event_source_name);
        self.channel.push(row.channel);
        self.computer.push(row.computer);
        self.event_id.push(row.event_id);
        self.event_qualifiers.push(row.event_qualifiers);
        self.version.push(row.version);
        self.level.push(row.level);
        self.task.push(row.task);
        self.opcode.push(row.opcode);
        self.keywords_hex.push(row.keywords_hex);
        self.execution_process_id.push(row.execution_process_id);
        self.execution_thread_id.push(row.execution_thread_id);
        self.security_user_id.push(row.security_user_id);
        self.correlation_activity_id
            .push(row.correlation_activity_id);
        self.correlation_related_activity_id
            .push(row.correlation_related_activity_id);
        self.raw_event_json.push(row.raw_event_json);
        self.raw_event_xml.push(row.raw_event_xml);
    }

    fn len(&self) -> usize {
        self.event_record_id.len()
    }

    fn is_empty(&self) -> bool {
        self.event_record_id.is_empty()
    }

    fn take_batch(&mut self, schema: &Schema) -> Result<RecordBatch> {
        let event_record_id = Int64Array::from(std::mem::take(&mut self.event_record_id));
        let timestamp = build_timestamp_array_required(std::mem::take(&mut self.timestamp_us));
        let provider_name = StringArray::from(std::mem::take(&mut self.provider_name));
        let provider_guid = StringArray::from(std::mem::take(&mut self.provider_guid));
        let provider_event_source_name =
            StringArray::from(std::mem::take(&mut self.provider_event_source_name));
        let channel = StringArray::from(std::mem::take(&mut self.channel));
        let computer = StringArray::from(std::mem::take(&mut self.computer));
        let event_id = Int64Array::from(std::mem::take(&mut self.event_id));
        let event_qualifiers = Int64Array::from(std::mem::take(&mut self.event_qualifiers));
        let version = Int64Array::from(std::mem::take(&mut self.version));
        let level = Int64Array::from(std::mem::take(&mut self.level));
        let task = Int64Array::from(std::mem::take(&mut self.task));
        let opcode = Int64Array::from(std::mem::take(&mut self.opcode));
        let keywords_hex = StringArray::from(std::mem::take(&mut self.keywords_hex));
        let execution_process_id = Int64Array::from(std::mem::take(&mut self.execution_process_id));
        let execution_thread_id = Int64Array::from(std::mem::take(&mut self.execution_thread_id));
        let security_user_id = StringArray::from(std::mem::take(&mut self.security_user_id));
        let correlation_activity_id =
            StringArray::from(std::mem::take(&mut self.correlation_activity_id));
        let correlation_related_activity_id =
            StringArray::from(std::mem::take(&mut self.correlation_related_activity_id));
        let raw_event_json = StringArray::from(std::mem::take(&mut self.raw_event_json));
        let raw_event_xml = StringArray::from(std::mem::take(&mut self.raw_event_xml));

        RecordBatch::try_new(
            std::sync::Arc::new(schema.clone()),
            vec![
                std::sync::Arc::new(event_record_id),
                std::sync::Arc::new(timestamp),
                std::sync::Arc::new(provider_name),
                std::sync::Arc::new(provider_guid),
                std::sync::Arc::new(provider_event_source_name),
                std::sync::Arc::new(channel),
                std::sync::Arc::new(computer),
                std::sync::Arc::new(event_id),
                std::sync::Arc::new(event_qualifiers),
                std::sync::Arc::new(version),
                std::sync::Arc::new(level),
                std::sync::Arc::new(task),
                std::sync::Arc::new(opcode),
                std::sync::Arc::new(keywords_hex),
                std::sync::Arc::new(execution_process_id),
                std::sync::Arc::new(execution_thread_id),
                std::sync::Arc::new(security_user_id),
                std::sync::Arc::new(correlation_activity_id),
                std::sync::Arc::new(correlation_related_activity_id),
                std::sync::Arc::new(raw_event_json),
                std::sync::Arc::new(raw_event_xml),
            ],
        )
        .context("Failed to build evtx_events batch")
    }
}

struct KvRow {
    event_record_id: i64,
    section: String,
    key: String,
    idx: Option<i64>,
    value_raw: Option<String>,
    value_type: Option<String>,
    value_int64: Option<i64>,
    value_bool: Option<bool>,
    value_ts: Option<i64>,
    value_bytes: Option<Vec<u8>>,
}

struct KvBuilder {
    event_record_id: Vec<i64>,
    section: Vec<String>,
    key: Vec<String>,
    idx: Vec<Option<i64>>,
    value_raw: Vec<Option<String>>,
    value_type: Vec<Option<String>>,
    value_int64: Vec<Option<i64>>,
    value_bool: Vec<Option<bool>>,
    value_ts: Vec<Option<i64>>,
    value_bytes: Vec<Option<Vec<u8>>>,
}

impl KvBuilder {
    fn new(capacity: usize) -> Self {
        Self {
            event_record_id: Vec::with_capacity(capacity),
            section: Vec::with_capacity(capacity),
            key: Vec::with_capacity(capacity),
            idx: Vec::with_capacity(capacity),
            value_raw: Vec::with_capacity(capacity),
            value_type: Vec::with_capacity(capacity),
            value_int64: Vec::with_capacity(capacity),
            value_bool: Vec::with_capacity(capacity),
            value_ts: Vec::with_capacity(capacity),
            value_bytes: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, row: KvRow) {
        self.event_record_id.push(row.event_record_id);
        self.section.push(row.section);
        self.key.push(row.key);
        self.idx.push(row.idx);
        self.value_raw.push(row.value_raw);
        self.value_type.push(row.value_type);
        self.value_int64.push(row.value_int64);
        self.value_bool.push(row.value_bool);
        self.value_ts.push(row.value_ts);
        self.value_bytes.push(row.value_bytes);
    }

    fn len(&self) -> usize {
        self.event_record_id.len()
    }

    fn is_empty(&self) -> bool {
        self.event_record_id.is_empty()
    }

    fn take_batch(&mut self, schema: &Schema) -> Result<RecordBatch> {
        let event_record_id = Int64Array::from(std::mem::take(&mut self.event_record_id));
        let section = StringArray::from(std::mem::take(&mut self.section));
        let key = StringArray::from(std::mem::take(&mut self.key));
        let idx = Int64Array::from(std::mem::take(&mut self.idx));
        let value_raw = StringArray::from(std::mem::take(&mut self.value_raw));
        let value_type = StringArray::from(std::mem::take(&mut self.value_type));
        let value_int64 = Int64Array::from(std::mem::take(&mut self.value_int64));
        let value_bool = BooleanArray::from(std::mem::take(&mut self.value_bool));
        let value_ts = build_timestamp_array(std::mem::take(&mut self.value_ts));
        let value_bytes_vec = std::mem::take(&mut self.value_bytes);
        let value_bytes_refs = value_bytes_vec
            .iter()
            .map(|opt| opt.as_deref())
            .collect::<Vec<_>>();
        let value_bytes = BinaryArray::from(value_bytes_refs);

        RecordBatch::try_new(
            std::sync::Arc::new(schema.clone()),
            vec![
                std::sync::Arc::new(event_record_id),
                std::sync::Arc::new(section),
                std::sync::Arc::new(key),
                std::sync::Arc::new(idx),
                std::sync::Arc::new(value_raw),
                std::sync::Arc::new(value_type),
                std::sync::Arc::new(value_int64),
                std::sync::Arc::new(value_bool),
                std::sync::Arc::new(value_ts),
                std::sync::Arc::new(value_bytes),
            ],
        )
        .context("Failed to build kv batch")
    }
}

struct AnnotationRow {
    event_record_id: i64,
    note_type: String,
    note_key: String,
    note_value: Option<String>,
    severity: i64,
}

struct AnnotationBuilder {
    event_record_id: Vec<i64>,
    note_type: Vec<String>,
    note_key: Vec<String>,
    note_value: Vec<Option<String>>,
    severity: Vec<i64>,
}

impl AnnotationBuilder {
    fn new(capacity: usize) -> Self {
        Self {
            event_record_id: Vec::with_capacity(capacity),
            note_type: Vec::with_capacity(capacity),
            note_key: Vec::with_capacity(capacity),
            note_value: Vec::with_capacity(capacity),
            severity: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, row: AnnotationRow) {
        self.event_record_id.push(row.event_record_id);
        self.note_type.push(row.note_type);
        self.note_key.push(row.note_key);
        self.note_value.push(row.note_value);
        self.severity.push(row.severity);
    }

    fn len(&self) -> usize {
        self.event_record_id.len()
    }

    fn is_empty(&self) -> bool {
        self.event_record_id.is_empty()
    }

    fn take_batch(&mut self, schema: &Schema) -> Result<RecordBatch> {
        let event_record_id = Int64Array::from(std::mem::take(&mut self.event_record_id));
        let note_type = StringArray::from(std::mem::take(&mut self.note_type));
        let note_key = StringArray::from(std::mem::take(&mut self.note_key));
        let note_value = StringArray::from(std::mem::take(&mut self.note_value));
        let severity = Int64Array::from(std::mem::take(&mut self.severity));

        RecordBatch::try_new(
            std::sync::Arc::new(schema.clone()),
            vec![
                std::sync::Arc::new(event_record_id),
                std::sync::Arc::new(note_type),
                std::sync::Arc::new(note_key),
                std::sync::Arc::new(note_value),
                std::sync::Arc::new(severity),
            ],
        )
        .context("Failed to build annotations batch")
    }
}

struct FileRow {
    file_path: Option<String>,
    file_hash: String,
    file_size: i64,
    file_mtime: Option<i64>,
    record_count: Option<i64>,
    parser_output_version: String,
}

struct FilesBuilder {
    file_path: Vec<Option<String>>,
    file_hash: Vec<String>,
    file_size: Vec<i64>,
    file_mtime: Vec<Option<i64>>,
    record_count: Vec<Option<i64>>,
    parser_output_version: Vec<String>,
}

impl FilesBuilder {
    fn new(capacity: usize) -> Self {
        Self {
            file_path: Vec::with_capacity(capacity),
            file_hash: Vec::with_capacity(capacity),
            file_size: Vec::with_capacity(capacity),
            file_mtime: Vec::with_capacity(capacity),
            record_count: Vec::with_capacity(capacity),
            parser_output_version: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, row: FileRow) {
        self.file_path.push(row.file_path);
        self.file_hash.push(row.file_hash);
        self.file_size.push(row.file_size);
        self.file_mtime.push(row.file_mtime);
        self.record_count.push(row.record_count);
        self.parser_output_version.push(row.parser_output_version);
    }

    fn take_batch(&mut self, schema: &Schema) -> Result<RecordBatch> {
        let file_path = StringArray::from(std::mem::take(&mut self.file_path));
        let file_hash = StringArray::from(std::mem::take(&mut self.file_hash));
        let file_size = Int64Array::from(std::mem::take(&mut self.file_size));
        let file_mtime = build_timestamp_array(std::mem::take(&mut self.file_mtime));
        let record_count = Int64Array::from(std::mem::take(&mut self.record_count));
        let parser_output_version =
            StringArray::from(std::mem::take(&mut self.parser_output_version));

        RecordBatch::try_new(
            std::sync::Arc::new(schema.clone()),
            vec![
                std::sync::Arc::new(file_path),
                std::sync::Arc::new(file_hash),
                std::sync::Arc::new(file_size),
                std::sync::Arc::new(file_mtime),
                std::sync::Arc::new(record_count),
                std::sync::Arc::new(parser_output_version),
            ],
        )
        .context("Failed to build files batch")
    }
}

struct FileMetadata {
    path: String,
    hash: String,
    size: i64,
    mtime_us: Option<i64>,
}

impl FileMetadata {
    fn from_path(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        let size =
            i64::try_from(metadata.len()).map_err(|_| anyhow::anyhow!("File size overflow"))?;
        let mtime_us = metadata.modified().ok().and_then(system_time_to_micros);
        let hash = blake3_hash_file(path)?;
        Ok(Self {
            path: path.display().to_string(),
            hash,
            size,
            mtime_us,
        })
    }
}

fn system_time_to_micros(time: std::time::SystemTime) -> Option<i64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_micros()).ok()
}

fn u64_to_i64(value: u64) -> Option<i64> {
    i64::try_from(value).ok()
}

fn blake3_hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn build_timestamp_array(values: Vec<Option<i64>>) -> TimestampMicrosecondArray {
    let mut builder = TimestampMicrosecondBuilder::new().with_data_type(DataType::Timestamp(
        TimeUnit::Microsecond,
        Some("UTC".into()),
    ));
    for value in values {
        builder.append_option(value);
    }
    builder.finish()
}

fn build_timestamp_array_required(values: Vec<i64>) -> TimestampMicrosecondArray {
    let mut builder = TimestampMicrosecondBuilder::new().with_data_type(DataType::Timestamp(
        TimeUnit::Microsecond,
        Some("UTC".into()),
    ));
    for value in values {
        builder.append_value(value);
    }
    builder.finish()
}
