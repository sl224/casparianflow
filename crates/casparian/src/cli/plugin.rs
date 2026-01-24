//! Plugin command - Manage native bundles
//!
//! Commands:
//! - `plugin import <bundle_path>` - Verify and install a signed bundle
//! - `plugin list` - List installed bundles
//! - `plugin verify <name>@<version>` - Verify an installed bundle

use crate::cli::config;
use crate::cli::error::HelpfulError;
use crate::cli::output::print_table;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use casparian::trust::{
    load_default_trust_config, PublicKeyBase64, SignerId, TrustConfig, TrustMode,
};
use casparian_db::{DbConnection, DbTimestamp, DbValue};
use casparian_protocol::{PluginStatus, RuntimeKind, SchemaDefinition};
use casparian_schema::approval::derive_scope_id;
use casparian_schema::{LockedColumn, LockedSchema, SchemaContract, SchemaStorage};
use casparian_security::signing::sha256;
use clap::Subcommand;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug, Clone)]
pub enum PluginAction {
    /// Import a native bundle from disk
    Import {
        /// Path to the bundle root directory
        bundle: PathBuf,
    },
    /// List installed plugins
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Verify an installed bundle
    Verify {
        /// Plugin id in the format name@version
        target: String,
        /// Platform OS (required if multiple variants exist)
        #[arg(long)]
        os: Option<String>,
        /// Platform architecture (required if multiple variants exist)
        #[arg(long)]
        arch: Option<String>,
    },
}

pub fn run(action: PluginAction) -> Result<()> {
    match action {
        PluginAction::Import { bundle } => cmd_import(bundle),
        PluginAction::List { json } => cmd_list(json),
        PluginAction::Verify { target, os, arch } => cmd_verify(&target, os, arch),
    }
}

#[derive(Debug, Deserialize)]
struct BundleIndex {
    files: Vec<BundleFile>,
}

#[derive(Debug, Deserialize)]
struct BundleFile {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BundleManifest {
    name: String,
    version: String,
    protocol_version: String,
    runtime_kind: RuntimeKind,
    entrypoint: String,
    #[serde(default)]
    platform_os: Option<String>,
    #[serde(default)]
    platform_arch: Option<String>,
}

#[derive(Debug, Serialize)]
struct PluginInfo {
    plugin_name: String,
    version: String,
    runtime_kind: String,
    platform_os: Option<String>,
    platform_arch: Option<String>,
    signature_verified: bool,
    signer_id: Option<String>,
    status: String,
    created_at: DbTimestamp,
}

fn cmd_import(bundle_root: PathBuf) -> Result<()> {
    let bundle_root = bundle_root
        .canonicalize()
        .context("Failed to resolve bundle path")?;
    if !bundle_root.is_dir() {
        anyhow::bail!("Bundle path must be a directory");
    }

    let (index, index_bytes) = load_bundle_index(&bundle_root)?;
    verify_bundle_files(&bundle_root, &index)?;

    let manifest_path = bundle_root.join("casparian.toml");
    let manifest = load_manifest(&manifest_path)?;
    if manifest.runtime_kind != RuntimeKind::NativeExec {
        anyhow::bail!("Only native_exec bundles are supported for import");
    }
    validate_manifest_platform(&manifest)?;

    let entrypoint_path = bundle_root.join(&manifest.entrypoint);
    if !entrypoint_path.exists() {
        anyhow::bail!(
            "Entrypoint '{}' does not exist in bundle",
            manifest.entrypoint
        );
    }

    let trust = load_default_trust_config().context("Failed to load trust configuration")?;
    let (signature_verified, signer_id) =
        verify_bundle_signature(&index_bytes, bundle_root.join("bundle.sig"), &trust)?;

    let schema_defs = load_schema_definitions(&bundle_root)?;
    let schema_artifacts_json =
        serde_json::to_string(&schema_defs).context("Failed to serialize schema artifacts")?;

    let binary_bytes = fs::read(&entrypoint_path)
        .with_context(|| format!("Failed to read binary: {}", entrypoint_path.display()))?;
    let binary_hash = sha256(&binary_bytes);
    let source_code = format!("binary_sha256:{}", binary_hash);
    let manifest_json =
        serde_json::to_string(&manifest).context("Failed to serialize manifest JSON")?;
    let artifact_hash =
        compute_artifact_hash(&source_code, "", &manifest_json, &schema_artifacts_json);

    let install_root = install_path(&manifest)?;
    if install_root.exists() {
        fs::remove_dir_all(&install_root).with_context(|| {
            format!(
                "Failed to remove existing bundle at {}",
                install_root.display()
            )
        })?;
    }
    copy_dir_all(&bundle_root, &install_root)?;

    let conn = connect_db()?;
    let schema_storage = SchemaStorage::new(conn.clone())
        .map_err(|e| anyhow::anyhow!("Failed to initialize schema storage: {}", e))?;

    ensure_plugin_not_installed(&conn, &manifest)?;

    insert_manifest(
        &conn,
        &manifest,
        &source_code,
        &binary_hash,
        &artifact_hash,
        &manifest_json,
        &schema_artifacts_json,
        signature_verified,
        signer_id.as_ref(),
    )?;

    register_schema_contracts(
        &schema_storage,
        &schema_defs,
        &manifest.name,
        &manifest.version,
        signer_id.as_ref(),
    )?;

    println!(
        "✓ Imported {} v{} ({})",
        manifest.name, manifest.version, manifest.entrypoint
    );
    Ok(())
}

fn cmd_list(json_output: bool) -> Result<()> {
    let conn = connect_db_readonly()?;
    let rows = conn.query_all(
        r#"
        SELECT plugin_name, version, runtime_kind, platform_os, platform_arch,
               signature_verified, signer_id, status, created_at
        FROM cf_plugin_manifest
        ORDER BY created_at DESC
        "#,
        &[],
    )?;

    let mut plugins = Vec::with_capacity(rows.len());
    for row in rows {
        let status: String = row.get_by_name("status")?;
        let created_at: DbTimestamp = row.get_by_name("created_at")?;
        plugins.push(PluginInfo {
            plugin_name: row.get_by_name("plugin_name")?,
            version: row.get_by_name("version")?,
            runtime_kind: row.get_by_name("runtime_kind")?,
            platform_os: row.get_by_name("platform_os")?,
            platform_arch: row.get_by_name("platform_arch")?,
            signature_verified: row.get_by_name("signature_verified")?,
            signer_id: row.get_by_name("signer_id")?,
            status,
            created_at,
        });
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&plugins)?);
        return Ok(());
    }

    let headers = [
        "Plugin", "Version", "Runtime", "Platform", "Signed", "Signer", "Status", "Created",
    ];
    let rows = plugins
        .into_iter()
        .map(|p| {
            let platform = match (p.platform_os.as_deref(), p.platform_arch.as_deref()) {
                (Some(os), Some(arch)) => format!("{}/{}", os, arch),
                _ => "-".to_string(),
            };
            vec![
                p.plugin_name,
                p.version,
                p.runtime_kind,
                platform,
                if p.signature_verified { "yes" } else { "no" }.to_string(),
                p.signer_id.unwrap_or_else(|| "-".to_string()),
                p.status,
                p.created_at.to_rfc3339(),
            ]
        })
        .collect::<Vec<_>>();

    print_table(&headers, rows);
    Ok(())
}

fn cmd_verify(target: &str, os: Option<String>, arch: Option<String>) -> Result<()> {
    let (plugin_name, version) = parse_target(target)?;
    let conn = connect_db_readonly()?;

    let mut rows = conn.query_all(
        r#"
        SELECT runtime_kind, platform_os, platform_arch
        FROM cf_plugin_manifest
        WHERE plugin_name = ? AND version = ?
        ORDER BY created_at DESC
        "#,
        &[
            DbValue::from(plugin_name.as_str()),
            DbValue::from(version.as_str()),
        ],
    )?;

    if rows.is_empty() {
        anyhow::bail!("Plugin {}@{} not found in registry", plugin_name, version);
    }

    let (runtime_kind, platform_os, platform_arch) = if rows.len() == 1 {
        let row = rows.pop().unwrap();
        (
            row.get_by_name::<String>("runtime_kind")?,
            row.get_by_name::<Option<String>>("platform_os")?,
            row.get_by_name::<Option<String>>("platform_arch")?,
        )
    } else {
        let os = os.ok_or_else(|| {
            anyhow::anyhow!("Multiple platform variants found; specify --os and --arch")
        })?;
        let arch = arch.ok_or_else(|| {
            anyhow::anyhow!("Multiple platform variants found; specify --os and --arch")
        })?;
        let mut matched = None;
        for row in rows {
            let row_os: Option<String> = row.get_by_name("platform_os")?;
            let row_arch: Option<String> = row.get_by_name("platform_arch")?;
            if row_os.as_deref() == Some(os.as_str()) && row_arch.as_deref() == Some(arch.as_str())
            {
                let runtime: String = row.get_by_name("runtime_kind")?;
                matched = Some((runtime, row_os, row_arch));
                break;
            }
        }
        matched.ok_or_else(|| anyhow::anyhow!("No matching platform variant found"))?
    };

    if runtime_kind != RuntimeKind::NativeExec.as_str() {
        anyhow::bail!("Only native_exec bundles can be verified with this command");
    }

    let manifest = BundleManifest {
        name: plugin_name.clone(),
        version: version.clone(),
        protocol_version: "".to_string(),
        runtime_kind: RuntimeKind::NativeExec,
        entrypoint: String::new(),
        platform_os: platform_os.clone(),
        platform_arch: platform_arch.clone(),
    };
    let bundle_root = install_path(&manifest)?;

    let (index, index_bytes) = load_bundle_index(&bundle_root)?;
    verify_bundle_files(&bundle_root, &index)?;

    let trust = load_default_trust_config().context("Failed to load trust configuration")?;
    let (signature_verified, signer_id) =
        verify_bundle_signature(&index_bytes, bundle_root.join("bundle.sig"), &trust)?;

    println!(
        "✓ Verified {}@{} ({})",
        plugin_name,
        version,
        if signature_verified {
            signer_id
                .map(|id| format!("signed by {}", id.as_str()))
                .unwrap_or_else(|| "signed".to_string())
        } else {
            "unsigned".to_string()
        }
    );
    Ok(())
}

fn load_bundle_index(bundle_root: &Path) -> Result<(BundleIndex, Vec<u8>)> {
    let index_path = bundle_root.join("bundle.index.json");
    if !index_path.exists() {
        anyhow::bail!("bundle.index.json missing from bundle");
    }
    let index_bytes = fs::read(&index_path)
        .with_context(|| format!("Failed to read {}", index_path.display()))?;
    let index: BundleIndex =
        serde_json::from_slice(&index_bytes).context("Failed to parse bundle.index.json")?;
    if index.files.is_empty() {
        anyhow::bail!("bundle.index.json must list at least one file");
    }
    Ok((index, index_bytes))
}

fn verify_bundle_files(bundle_root: &Path, index: &BundleIndex) -> Result<()> {
    let mut seen = HashSet::new();
    for file in &index.files {
        if file.path.trim().is_empty() {
            anyhow::bail!("bundle.index.json has an empty file path");
        }
        let rel = Path::new(&file.path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("bundle.index.json contains an unsafe path: {}", file.path);
        }
        let path = bundle_root.join(rel);
        if !path.exists() {
            anyhow::bail!("Bundle file missing: {}", path.display());
        }
        let bytes =
            fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let hash = sha256(&bytes);
        if hash != file.sha256 {
            anyhow::bail!(
                "Hash mismatch for {}: expected {}, got {}",
                file.path,
                file.sha256,
                hash
            );
        }
        seen.insert(file.path.clone());
    }

    let manifest_path = "casparian.toml";
    if !seen.contains(manifest_path) {
        anyhow::bail!("bundle.index.json must include casparian.toml");
    }
    Ok(())
}

fn verify_bundle_signature(
    index_bytes: &[u8],
    sig_path: PathBuf,
    trust: &TrustConfig,
) -> Result<(bool, Option<SignerId>)> {
    let require_signature =
        matches!(trust.mode, TrustMode::VaultSignedOnly) && !trust.allow_unsigned_native;
    if !sig_path.exists() {
        if require_signature {
            anyhow::bail!("bundle.sig is required but missing");
        }
        return Ok((false, None));
    }

    let sig_raw = fs::read_to_string(&sig_path)
        .with_context(|| format!("Failed to read {}", sig_path.display()))?;
    let sig_trimmed = sig_raw.trim();
    if sig_trimmed.is_empty() {
        if require_signature {
            anyhow::bail!("bundle.sig is empty");
        }
        return Ok((false, None));
    }

    if trust.keys.is_empty() {
        if require_signature {
            anyhow::bail!("No trusted keys configured; cannot verify bundle signature");
        }
        return Ok((false, None));
    }

    let signature_bytes = general_purpose::STANDARD
        .decode(sig_trimmed)
        .context("bundle.sig is not valid base64")?;
    let signature_bytes: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("bundle.sig must be 64 bytes (base64-encoded)"))?;
    let signature = Signature::from_bytes(&signature_bytes);

    let mut hasher = Sha256::new();
    hasher.update(index_bytes);
    let index_digest = hasher.finalize();

    let candidates: Vec<(&SignerId, &PublicKeyBase64)> = if trust.allowed_signers.is_empty() {
        trust.keys.iter().collect()
    } else {
        trust
            .allowed_signers
            .iter()
            .filter_map(|signer| trust.keys.get_key_value(signer))
            .collect()
    };

    for (signer_id, key) in candidates {
        let key_bytes = general_purpose::STANDARD
            .decode(key.as_str())
            .with_context(|| format!("Invalid base64 key for signer '{}'", signer_id))?;
        let key_bytes: [u8; 32] = key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Signer key '{}' must be 32 bytes", signer_id))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid public key for signer '{}'", signer_id))?;
        if verifying_key
            .verify_strict(index_digest.as_slice(), &signature)
            .is_ok()
        {
            return Ok((true, Some(signer_id.clone())));
        }
    }

    if require_signature {
        anyhow::bail!("bundle.sig verification failed");
    }

    Ok((false, None))
}

fn load_manifest(path: &Path) -> Result<BundleManifest> {
    if !path.exists() {
        anyhow::bail!("casparian.toml missing from bundle");
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let manifest: BundleManifest =
        toml::from_str(&content).context("Failed to parse casparian.toml")?;

    if manifest.name.trim().is_empty() {
        anyhow::bail!("Manifest field 'name' must be non-empty");
    }
    if manifest.version.trim().is_empty() {
        anyhow::bail!("Manifest field 'version' must be non-empty");
    }
    if manifest.protocol_version.trim().is_empty() {
        anyhow::bail!("Manifest field 'protocol_version' must be non-empty");
    }
    if manifest.entrypoint.trim().is_empty() {
        anyhow::bail!("Manifest field 'entrypoint' must be non-empty");
    }

    Ok(manifest)
}

fn validate_manifest_platform(manifest: &BundleManifest) -> Result<()> {
    let platform_os = manifest
        .platform_os
        .as_ref()
        .map(|value| value.trim())
        .unwrap_or("");
    let platform_arch = manifest
        .platform_arch
        .as_ref()
        .map(|value| value.trim())
        .unwrap_or("");
    match manifest.runtime_kind {
        RuntimeKind::NativeExec => {
            if platform_os.is_empty() {
                anyhow::bail!("Manifest field 'platform_os' must be non-empty for native_exec");
            }
            if platform_arch.is_empty() {
                anyhow::bail!("Manifest field 'platform_arch' must be non-empty for native_exec");
            }
        }
        RuntimeKind::PythonShim => {
            if !platform_os.is_empty() || !platform_arch.is_empty() {
                anyhow::bail!(
                    "Manifest fields 'platform_os'/'platform_arch' are only valid for native_exec"
                );
            }
        }
    }
    Ok(())
}

fn load_schema_definitions(bundle_root: &Path) -> Result<BTreeMap<String, SchemaDefinition>> {
    let schemas_dir = bundle_root.join("schemas");
    if !schemas_dir.exists() {
        anyhow::bail!("schemas/ directory missing from bundle");
    }
    let mut schemas = BTreeMap::new();
    for entry in fs::read_dir(&schemas_dir)? {
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
        if output_name.trim().is_empty() {
            anyhow::bail!("Schema filename '{}' has empty output name", file_name);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let schema_def: SchemaDefinition = serde_json::from_str(&content)
            .with_context(|| format!("Invalid schema JSON in {}", path.display()))?;
        schemas.insert(output_name.to_string(), schema_def);
    }

    if schemas.is_empty() {
        anyhow::bail!("schemas/ must include at least one *.schema.json file");
    }

    Ok(schemas)
}

fn install_path(manifest: &BundleManifest) -> Result<PathBuf> {
    let home = config::casparian_home();
    let base = home
        .join("plugins")
        .join(&manifest.name)
        .join(&manifest.version);
    match manifest.runtime_kind {
        RuntimeKind::NativeExec => {
            let os = manifest
                .platform_os
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("platform_os missing from manifest"))?;
            let arch = manifest
                .platform_arch
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("platform_arch missing from manifest"))?;
            Ok(base.join(os).join(arch))
        }
        RuntimeKind::PythonShim => Ok(base.join("python_shim")),
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("Failed to create {}", dst.display()))?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("Failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

fn connect_db() -> Result<DbConnection> {
    let db_path = config::active_db_path();
    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestions([
                "TRY: Run 'casparian start' to initialize the database".to_string(),
                format!("TRY: Check if {} exists", db_path.display()),
            ])
            .into());
    }
    let url = format!("duckdb:{}", db_path.display());
    DbConnection::open_from_url(&url).map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(e.to_string())
            .with_suggestion("TRY: Ensure the database file is not corrupted")
            .into()
    })
}

fn connect_db_readonly() -> Result<DbConnection> {
    let db_path = config::active_db_path();
    if !db_path.exists() {
        return Err(HelpfulError::new("Database not found")
            .with_context(format!("Expected database at: {}", db_path.display()))
            .with_suggestions([
                "TRY: Run 'casparian start' to initialize the database".to_string(),
                format!("TRY: Check if {} exists", db_path.display()),
            ])
            .into());
    }
    DbConnection::open_duckdb_readonly(&db_path).map_err(|e| {
        HelpfulError::new("Failed to connect to database")
            .with_context(e.to_string())
            .with_suggestion("TRY: Ensure the database file is not corrupted")
            .into()
    })
}

fn ensure_plugin_not_installed(conn: &DbConnection, manifest: &BundleManifest) -> Result<()> {
    let mut query = String::from(
        "SELECT 1 FROM cf_plugin_manifest WHERE plugin_name = ? AND version = ? AND runtime_kind = ?",
    );
    let mut params = vec![
        DbValue::from(manifest.name.as_str()),
        DbValue::from(manifest.version.as_str()),
        DbValue::from(manifest.runtime_kind.as_str()),
    ];

    match &manifest.platform_os {
        Some(value) => {
            query.push_str(" AND platform_os = ?");
            params.push(DbValue::from(value.as_str()));
        }
        None => {
            query.push_str(" AND platform_os IS NULL");
        }
    }
    match &manifest.platform_arch {
        Some(value) => {
            query.push_str(" AND platform_arch = ?");
            params.push(DbValue::from(value.as_str()));
        }
        None => {
            query.push_str(" AND platform_arch IS NULL");
        }
    }

    let row = conn.query_optional(&query, &params)?;
    if row.is_some() {
        anyhow::bail!(
            "Plugin {} v{} ({}) already installed. Remove it before importing again.",
            manifest.name,
            manifest.version,
            manifest.runtime_kind.as_str()
        );
    }
    Ok(())
}

fn insert_manifest(
    conn: &DbConnection,
    manifest: &BundleManifest,
    source_code: &str,
    source_hash: &str,
    artifact_hash: &str,
    manifest_json: &str,
    schema_artifacts_json: &str,
    signature_verified: bool,
    signer_id: Option<&SignerId>,
) -> Result<()> {
    let now = DbTimestamp::now();
    let signer_value = signer_id
        .map(|id| DbValue::from(id.as_str()))
        .unwrap_or(DbValue::Null);

    conn.execute(
        r#"
        INSERT INTO cf_plugin_manifest
        (plugin_name, version, runtime_kind, entrypoint, platform_os, platform_arch,
         source_code, source_hash, status, env_hash, artifact_hash,
         manifest_json, protocol_version, schema_artifacts_json, outputs_json,
         signature_verified, signer_id, created_at, deployed_at,
         publisher_name, publisher_email, azure_oid, system_requirements)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        &[
            DbValue::from(manifest.name.as_str()),
            DbValue::from(manifest.version.as_str()),
            DbValue::from(manifest.runtime_kind.as_str()),
            DbValue::from(manifest.entrypoint.as_str()),
            manifest
                .platform_os
                .as_deref()
                .map(DbValue::from)
                .unwrap_or(DbValue::Null),
            manifest
                .platform_arch
                .as_deref()
                .map(DbValue::from)
                .unwrap_or(DbValue::Null),
            DbValue::from(source_code),
            DbValue::from(source_hash),
            DbValue::from(PluginStatus::Active.as_str()),
            DbValue::from(""),
            DbValue::from(artifact_hash),
            DbValue::from(manifest_json),
            DbValue::from(manifest.protocol_version.as_str()),
            DbValue::from(schema_artifacts_json),
            DbValue::from(schema_artifacts_json),
            DbValue::from(signature_verified),
            signer_value,
            DbValue::from(now.clone()),
            DbValue::from(now.clone()),
            DbValue::from(
                signer_id
                    .map(|id| id.as_str().to_string())
                    .unwrap_or_else(|| "bundle_import".to_string()),
            ),
            DbValue::Null,
            DbValue::Null,
            DbValue::Null,
        ],
    )?;
    Ok(())
}

fn register_schema_contracts(
    storage: &SchemaStorage,
    schema_defs: &BTreeMap<String, SchemaDefinition>,
    plugin_name: &str,
    version: &str,
    signer_id: Option<&SignerId>,
) -> Result<()> {
    let approved_by = signer_id.map(|id| id.as_str()).unwrap_or("bundle_import");

    for (output_name, schema_def) in schema_defs {
        let locked_schema = locked_schema_from_definition(output_name, schema_def)?;
        let scope_id = derive_scope_id(plugin_name, version, output_name);
        if let Some(existing) = storage
            .get_contract_for_scope(&scope_id)
            .map_err(|e| anyhow::anyhow!("Failed to load existing schema contract: {}", e))?
        {
            let existing_hash = existing
                .schemas
                .get(0)
                .map(|schema| schema.content_hash.as_str())
                .unwrap_or("");
            if existing_hash != locked_schema.content_hash {
                anyhow::bail!(
                    "Schema changed for output '{}' without version bump. Update version '{}' or delete the database.",
                    locked_schema.name,
                    version
                );
            }
            anyhow::bail!(
                "Schema contract already exists for output '{}' at version '{}'. Delete the database to re-import.",
                locked_schema.name,
                version
            );
        }

        let contract = SchemaContract::new(&scope_id, locked_schema, approved_by)
            .with_logic_hash(Some("native_bundle".to_string()));
        storage
            .save_contract(&contract)
            .map_err(|e| anyhow::anyhow!("Failed to save schema contract: {}", e))?;
    }
    Ok(())
}

fn locked_schema_from_definition(
    output_name: &str,
    schema_def: &SchemaDefinition,
) -> Result<LockedSchema> {
    if output_name.trim().is_empty() {
        anyhow::bail!("Output name cannot be empty");
    }
    let mut chars = output_name.chars();
    let first = chars
        .next()
        .ok_or_else(|| anyhow::anyhow!("Output name cannot be empty"))?;
    if !first.is_ascii_alphabetic() {
        anyhow::bail!("Output name must start with a letter: '{}'", output_name);
    }
    if !output_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        anyhow::bail!(
            "Output name must be lowercase alphanumeric + underscore: '{}'",
            output_name
        );
    }
    if schema_def.columns.is_empty() {
        anyhow::bail!(
            "Schema for '{}' must include at least one column",
            output_name
        );
    }

    let mut seen = HashSet::new();
    let mut columns = Vec::with_capacity(schema_def.columns.len());
    for col in &schema_def.columns {
        if col.name.trim().is_empty() {
            anyhow::bail!("Schema for '{}' has a column with empty name", output_name);
        }
        if !seen.insert(col.name.clone()) {
            anyhow::bail!(
                "Schema for '{}' has duplicate column '{}'",
                output_name,
                col.name
            );
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

fn compute_artifact_hash(
    source_code: &str,
    lockfile_content: &str,
    manifest_json: &str,
    schema_artifacts_json: &str,
) -> String {
    const SEP: u8 = 0x1f;
    let mut hasher = Sha256::new();
    for part in [
        source_code,
        lockfile_content,
        manifest_json,
        schema_artifacts_json,
    ] {
        hasher.update(part.as_bytes());
        hasher.update(&[SEP]);
    }
    format!("{:x}", hasher.finalize())
}

fn parse_target(target: &str) -> Result<(String, String)> {
    let (name, version) = target.split_once('@').ok_or_else(|| {
        anyhow::anyhow!(
            "Target must be in the format name@version (got '{}')",
            target
        )
    })?;
    if name.trim().is_empty() || version.trim().is_empty() {
        anyhow::bail!(
            "Target must be in the format name@version (got '{}')",
            target
        );
    }
    Ok((name.to_string(), version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};
    use casparian_db::DbConnection;
    use casparian_sentinel::db::queue::JobQueue;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_unsigned_bundle_rejected_when_required() {
        let temp = TempDir::new().unwrap();
        let trust = TrustConfig {
            mode: TrustMode::VaultSignedOnly,
            allowed_signers: Vec::new(),
            keys: BTreeMap::new(),
            allow_unsigned_native: false,
            allow_unsigned_python: false,
        };
        let err = verify_bundle_signature(b"{}", temp.path().join("bundle.sig"), &trust)
            .expect_err("expected missing bundle.sig to be rejected");
        assert!(err.to_string().contains("bundle.sig"));
    }

    #[test]
    #[ignore]
    fn test_import_native_bundle_end_to_end() {
        let temp_home = TempDir::new().unwrap();
        std::env::set_var("CASPARIAN_HOME", temp_home.path());

        let db_path = temp_home.path().join("casparian_flow.duckdb");
        {
            let conn = DbConnection::open_duckdb(&db_path).unwrap();
            let queue = JobQueue::new(conn);
            queue.init_registry_schema().unwrap();
        }

        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/native_plugin_basic");
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(&fixture_dir)
            .status()
            .expect("failed to build fixture");
        assert!(status.success(), "fixture build failed");

        let binary_name = if cfg!(windows) {
            "native_plugin_basic.exe"
        } else {
            "native_plugin_basic"
        };
        let binary_path = fixture_dir.join("target").join("release").join(binary_name);
        assert!(binary_path.exists(), "fixture binary missing");

        let bundle_dir = TempDir::new().unwrap();
        let bin_dir = bundle_dir.path().join("bin");
        let schemas_dir = bundle_dir.path().join("schemas");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&schemas_dir).unwrap();
        fs::copy(&binary_path, bin_dir.join(binary_name)).unwrap();
        let schema_src = fixture_dir.join("schemas").join("events.schema.json");
        fs::copy(&schema_src, schemas_dir.join("events.schema.json")).unwrap();

        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let manifest = format!(
            r#"
name = "native_plugin_basic"
version = "0.1.0"
protocol_version = "0.1"
runtime_kind = "native_exec"
entrypoint = "bin/{binary}"
platform_os = "{os}"
platform_arch = "{arch}"
"#,
            binary = binary_name,
            os = os,
            arch = arch
        );
        fs::write(bundle_dir.path().join("casparian.toml"), manifest).unwrap();

        let mut files = vec![
            "casparian.toml".to_string(),
            format!("bin/{}", binary_name),
            "schemas/events.schema.json".to_string(),
        ];
        files.sort();
        let index_files = files
            .iter()
            .map(|path| {
                let bytes = fs::read(bundle_dir.path().join(path)).unwrap();
                serde_json::json!({
                    "path": path,
                    "sha256": sha256(&bytes),
                })
            })
            .collect::<Vec<_>>();
        let index_json = serde_json::json!({ "files": index_files });
        let index_bytes = serde_json::to_vec(&index_json).unwrap();
        fs::write(bundle_dir.path().join("bundle.index.json"), &index_bytes).unwrap();

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let pub_key_b64 = general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let config = format!(
            r#"
[trust]
mode = "vault_signed_only"
allowed_signers = ["test_signer"]

[trust.keys]
test_signer = "{pub_key}"
"#,
            pub_key = pub_key_b64
        );
        fs::write(temp_home.path().join("config.toml"), config).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(&index_bytes);
        let digest = hasher.finalize();
        let signature = signing_key.sign(digest.as_slice());
        let sig_b64 = general_purpose::STANDARD.encode(signature.to_bytes());
        fs::write(bundle_dir.path().join("bundle.sig"), sig_b64).unwrap();

        cmd_import(bundle_dir.path().to_path_buf()).unwrap();

        let conn = DbConnection::open_duckdb_readonly(&db_path).unwrap();
        let row = conn
            .query_optional(
                "SELECT 1 FROM cf_plugin_manifest WHERE plugin_name = ? AND version = ?",
                &[DbValue::from("native_plugin_basic"), DbValue::from("0.1.0")],
            )
            .unwrap();
        assert!(row.is_some(), "plugin manifest row missing");
    }
}
