use crate::types::{SchemaDefinition, SinkMode};
use blake3::Hasher;

const SEP: u8 = 0x1f;

fn hash_parts(parts: &[&str]) -> String {
    let mut hasher = Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(&[SEP]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Hash a schema definition for idempotency targeting.
pub fn schema_hash(schema: Option<&SchemaDefinition>) -> Option<String> {
    let schema = schema?;
    let json = serde_json::to_string(schema).ok()?;
    Some(hash_parts(&[json.as_str()]))
}

/// Deterministic table name for schema-scoped outputs.
pub fn table_name_with_schema(base: &str, schema_hash: Option<&str>) -> String {
    if let Some(hash) = schema_hash {
        let suffix = &hash[..hash.len().min(8)];
        format!("{}__schema_{}", base, suffix)
    } else {
        base.to_string()
    }
}

/// Stable hash for an output target.
///
/// Components:
/// - output_name
/// - sink_uri
/// - sink_mode
/// - table_name (optional)
/// - schema_hash (optional)
pub fn output_target_key(
    output_name: &str,
    sink_uri: &str,
    sink_mode: SinkMode,
    table_name: Option<&str>,
    schema_hash: Option<&str>,
) -> String {
    let table = table_name.unwrap_or("");
    let schema = schema_hash.unwrap_or("");
    hash_parts(&[
        output_name,
        sink_uri,
        sink_mode.as_str(),
        table,
        schema,
    ])
}

/// Stable key for a file/output materialization.
///
/// Components:
/// - file_id
/// - file_mtime
/// - file_size
/// - parser_fingerprint (artifact hash or version)
/// - output_target_key
pub fn materialization_key(
    file_id: i64,
    file_mtime: i64,
    file_size: i64,
    parser_fingerprint: &str,
    output_target_key: &str,
) -> String {
    hash_parts(&[
        &file_id.to_string(),
        &file_mtime.to_string(),
        &file_size.to_string(),
        parser_fingerprint,
        output_target_key,
    ])
}
