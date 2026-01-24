//! Session Bundle storage for the Intent Pipeline.
//!
//! Session bundles persist the full decision history and artifacts
//! needed to reproduce outcomes.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

use super::state::IntentState;
use super::types::{
    ArtifactRef, DecisionRecord, FileSetId, ProposalId, SessionId, SessionManifest,
};

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(SessionId),

    #[error("session already exists: {0}")]
    AlreadyExists(SessionId),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid session path: {0}")]
    InvalidPath(String),
}

// ============================================================================
// Session Bundle
// ============================================================================

/// A session bundle containing all artifacts for a workflow instance.
///
/// Directory layout:
/// ```text
/// sessions/{session_id}/
///   manifest.json
///   corpora/
///     corpus_manifest.jsonl
///     filesets/
///       {file_set_id}.jsonl
///   proposals/
///     selection_{proposal_id}.json
///     tag_rules_{proposal_id}.json
///     path_fields_{proposal_id}.json
///     schema_intent_{proposal_id}.json
///     publish_plan_{proposal_id}.json
///   reports/
///     backtest_{job_id}.json
///     backtest_iters_{job_id}.jsonl
///   approvals.jsonl
///   patches/
///     schema_patch_{iteration_id}.json
///     parser_patch_{iteration_id}.patch
///     rule_patch_{iteration_id}.json
///   logs/
/// ```
#[derive(Debug)]
pub struct SessionBundle {
    pub session_id: SessionId,
    pub root: PathBuf,
}

impl SessionBundle {
    /// Create a new session bundle
    pub fn new(session_id: SessionId, root: PathBuf) -> Self {
        Self { session_id, root }
    }

    /// Get the session directory path
    pub fn session_dir(&self) -> PathBuf {
        self.root.join(self.session_id.to_string())
    }

    /// Initialize the directory structure
    pub fn init(&self) -> Result<(), SessionError> {
        let session_dir = self.session_dir();

        // Create directories
        fs::create_dir_all(session_dir.join("corpora/filesets"))?;
        fs::create_dir_all(session_dir.join("proposals"))?;
        fs::create_dir_all(session_dir.join("reports"))?;
        fs::create_dir_all(session_dir.join("patches"))?;
        fs::create_dir_all(session_dir.join("logs"))?;

        Ok(())
    }

    // ========================================================================
    // Manifest
    // ========================================================================

    /// Write the session manifest
    pub fn write_manifest(&self, manifest: &SessionManifest) -> Result<(), SessionError> {
        let path = self.session_dir().join("manifest.json");
        let json = serde_json::to_string_pretty(manifest)?;
        atomic_write(&path, json.as_bytes())?;
        Ok(())
    }

    /// Read the session manifest
    pub fn read_manifest(&self) -> Result<SessionManifest, SessionError> {
        let path = self.session_dir().join("manifest.json");
        let content = fs::read_to_string(&path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Update the manifest state
    pub fn update_state(&self, state: IntentState) -> Result<(), SessionError> {
        let mut manifest = self.read_manifest()?;
        manifest.state = state.as_str().to_string();
        self.write_manifest(&manifest)
    }

    /// Add an artifact reference to the manifest
    pub fn add_artifact(&self, kind: &str, reference: &str) -> Result<(), SessionError> {
        let mut manifest = self.read_manifest()?;
        manifest.artifacts.push(ArtifactRef {
            kind: kind.to_string(),
            reference: reference.to_string(),
        });
        self.write_manifest(&manifest)
    }

    // ========================================================================
    // Corpus Manifest
    // ========================================================================

    /// Path to the corpus manifest
    pub fn corpus_manifest_path(&self) -> PathBuf {
        self.session_dir().join("corpora/corpus_manifest.jsonl")
    }

    /// Append entries to the corpus manifest
    pub fn append_corpus_entries(&self, entries: &[CorpusEntry]) -> Result<(), SessionError> {
        let path = self.corpus_manifest_path();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        for entry in entries {
            let line = serde_json::to_string(entry)?;
            writeln!(file, "{}", line)?;
        }

        Ok(())
    }

    /// Read all corpus entries
    pub fn read_corpus_entries(&self) -> Result<Vec<CorpusEntry>, SessionError> {
        let path = self.corpus_manifest_path();
        if !path.exists() {
            return Ok(vec![]);
        }

        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                let entry: CorpusEntry = serde_json::from_str(&line)?;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    // ========================================================================
    // FileSet Storage
    // ========================================================================

    /// Path to a fileset manifest
    pub fn fileset_path(&self, file_set_id: FileSetId) -> PathBuf {
        self.session_dir()
            .join(format!("corpora/filesets/{}.jsonl", file_set_id))
    }

    /// Write fileset entries
    pub fn write_fileset(
        &self,
        file_set_id: FileSetId,
        entries: &[FileSetEntry],
    ) -> Result<(), SessionError> {
        let path = self.fileset_path(file_set_id);
        let mut file = fs::File::create(&path)?;

        for entry in entries {
            let line = serde_json::to_string(entry)?;
            writeln!(file, "{}", line)?;
        }

        Ok(())
    }

    /// Read fileset entries
    pub fn read_fileset(&self, file_set_id: FileSetId) -> Result<Vec<FileSetEntry>, SessionError> {
        let path = self.fileset_path(file_set_id);
        if !path.exists() {
            return Ok(vec![]);
        }

        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                let entry: FileSetEntry = serde_json::from_str(&line)?;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Read fileset with paging
    pub fn read_fileset_page(
        &self,
        file_set_id: FileSetId,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<FileSetEntry>, Option<usize>), SessionError> {
        let path = self.fileset_path(file_set_id);
        if !path.exists() {
            return Ok((vec![], None));
        }

        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut count = 0;
        let mut has_more = false;

        for line in reader.lines().skip(offset) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if count >= limit {
                has_more = true;
                break;
            }

            let entry: FileSetEntry = serde_json::from_str(&line)?;
            entries.push(entry);
            count += 1;
        }

        let next_cursor = if has_more { Some(offset + limit) } else { None };

        Ok((entries, next_cursor))
    }

    // ========================================================================
    // Proposals
    // ========================================================================

    /// Write a proposal artifact
    pub fn write_proposal<T: Serialize>(
        &self,
        kind: &str,
        proposal_id: ProposalId,
        proposal: &T,
    ) -> Result<String, SessionError> {
        let filename = format!("{}_{}.json", kind, proposal_id);
        let path = self.session_dir().join("proposals").join(&filename);
        let json = serde_json::to_string_pretty(proposal)?;
        atomic_write(&path, json.as_bytes())?;

        let ref_path = format!("proposals/{}", filename);
        self.add_artifact(kind, &ref_path)?;

        Ok(ref_path)
    }

    /// Read a proposal artifact
    pub fn read_proposal<T: for<'de> Deserialize<'de>>(
        &self,
        kind: &str,
        proposal_id: ProposalId,
    ) -> Result<T, SessionError> {
        let filename = format!("{}_{}.json", kind, proposal_id);
        let path = self.session_dir().join("proposals").join(&filename);
        let content = fs::read_to_string(&path)?;
        let proposal = serde_json::from_str(&content)?;
        Ok(proposal)
    }

    // ========================================================================
    // Reports
    // ========================================================================

    /// Write a report
    pub fn write_report<T: Serialize>(
        &self,
        kind: &str,
        job_id: &str,
        report: &T,
    ) -> Result<String, SessionError> {
        let filename = format!("{}_{}.json", kind, job_id);
        let path = self.session_dir().join("reports").join(&filename);
        let json = serde_json::to_string_pretty(report)?;
        atomic_write(&path, json.as_bytes())?;

        let ref_path = format!("reports/{}", filename);
        self.add_artifact(kind, &ref_path)?;

        Ok(ref_path)
    }

    /// Append to an iterations file
    pub fn append_iterations<T: Serialize>(
        &self,
        job_id: &str,
        iterations: &[T],
    ) -> Result<String, SessionError> {
        let filename = format!("backtest_iters_{}.jsonl", job_id);
        let path = self.session_dir().join("reports").join(&filename);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        for iter in iterations {
            let line = serde_json::to_string(iter)?;
            writeln!(file, "{}", line)?;
        }

        Ok(format!("reports/{}", filename))
    }

    // ========================================================================
    // Approvals
    // ========================================================================

    /// Path to approvals log
    pub fn approvals_path(&self) -> PathBuf {
        self.session_dir().join("approvals.jsonl")
    }

    /// Append a decision record
    pub fn append_decision(&self, record: &DecisionRecord) -> Result<(), SessionError> {
        let path = self.approvals_path();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let line = serde_json::to_string(record)?;
        writeln!(file, "{}", line)?;

        Ok(())
    }

    /// Read all decision records
    pub fn read_decisions(&self) -> Result<Vec<DecisionRecord>, SessionError> {
        let path = self.approvals_path();
        if !path.exists() {
            return Ok(vec![]);
        }

        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                let record: DecisionRecord = serde_json::from_str(&line)?;
                records.push(record);
            }
        }

        Ok(records)
    }

    // ========================================================================
    // Patches
    // ========================================================================

    /// Write a patch file
    pub fn write_patch(
        &self,
        kind: &str,
        iteration_id: &str,
        content: &[u8],
    ) -> Result<String, SessionError> {
        let ext = if kind == "parser_patch" {
            "patch"
        } else {
            "json"
        };
        let filename = format!("{}_{}.{}", kind, iteration_id, ext);
        let path = self.session_dir().join("patches").join(&filename);
        atomic_write(&path, content)?;

        Ok(format!("patches/{}", filename))
    }
}

// ============================================================================
// Session Store
// ============================================================================

/// Store for managing multiple sessions
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Create a new session store with the default path
    pub fn new() -> Self {
        Self::with_root(default_sessions_dir())
    }

    /// Create a session store with a custom root
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a new session
    pub fn create_session(
        &self,
        intent_text: &str,
        actor: Option<&str>,
        client: Option<&str>,
    ) -> Result<SessionBundle, SessionError> {
        let session_id = SessionId::new();
        let bundle = SessionBundle::new(session_id, self.root.clone());

        // Check if already exists
        if bundle.session_dir().exists() {
            return Err(SessionError::AlreadyExists(session_id));
        }

        // Initialize directory structure
        bundle.init()?;

        // Write initial manifest
        let manifest = SessionManifest {
            session_id,
            created_at: Utc::now(),
            intent_text: intent_text.to_string(),
            state: IntentState::InterpretIntent.as_str().to_string(),
            corpus_manifest_ref: None,
            artifacts: vec![],
            actor: actor.map(String::from),
            client: client.map(String::from),
        };
        bundle.write_manifest(&manifest)?;

        Ok(bundle)
    }

    /// Get an existing session
    pub fn get_session(&self, session_id: SessionId) -> Result<SessionBundle, SessionError> {
        let bundle = SessionBundle::new(session_id, self.root.clone());

        if !bundle.session_dir().exists() {
            return Err(SessionError::NotFound(session_id));
        }

        Ok(bundle)
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<SessionId>, SessionError> {
        if !self.root.exists() {
            return Ok(vec![]);
        }

        let mut sessions = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(session_id) = name.parse() {
                        sessions.push(session_id);
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Delete a session
    pub fn delete_session(&self, session_id: SessionId) -> Result<(), SessionError> {
        let bundle = SessionBundle::new(session_id, self.root.clone());
        let session_dir = bundle.session_dir();

        if !session_dir.exists() {
            return Err(SessionError::NotFound(session_id));
        }

        fs::remove_dir_all(session_dir)?;
        Ok(())
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Entry in the corpus manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusEntry {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Entry in a fileset manifest
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSetEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

// ============================================================================
// Helpers
// ============================================================================

/// Get the default sessions directory
pub fn default_sessions_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CASP_SESSION_DIR") {
        return PathBuf::from(dir);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("sessions")
}

/// Atomic write via temp file + rename
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let temp_path = parent.join(format!(".tmp_{}", uuid::Uuid::new_v4()));

    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_store_create_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());

        let bundle = store
            .create_session("process sales files", Some("user@example.com"), Some("cli"))
            .unwrap();

        let manifest = bundle.read_manifest().unwrap();
        assert_eq!(manifest.intent_text, "process sales files");
        assert_eq!(manifest.actor, Some("user@example.com".to_string()));

        // Get the same session
        let bundle2 = store.get_session(bundle.session_id).unwrap();
        assert_eq!(bundle.session_id, bundle2.session_id);
    }

    #[test]
    fn test_session_store_list() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());

        let bundle1 = store.create_session("intent 1", None, None).unwrap();
        let bundle2 = store.create_session("intent 2", None, None).unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&bundle1.session_id));
        assert!(sessions.contains(&bundle2.session_id));
    }

    #[test]
    fn test_session_bundle_fileset() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());

        let bundle = store.create_session("test", None, None).unwrap();
        let file_set_id = FileSetId::new();

        let entries = vec![
            FileSetEntry {
                path: "/data/file1.csv".to_string(),
                size: Some(1000),
                content_hash: Some("abc123".to_string()),
            },
            FileSetEntry {
                path: "/data/file2.csv".to_string(),
                size: Some(2000),
                content_hash: Some("def456".to_string()),
            },
        ];

        bundle.write_fileset(file_set_id, &entries).unwrap();

        let read_entries = bundle.read_fileset(file_set_id).unwrap();
        assert_eq!(read_entries.len(), 2);
        assert_eq!(read_entries[0].path, "/data/file1.csv");
        assert_eq!(read_entries[1].path, "/data/file2.csv");
    }

    #[test]
    fn test_session_bundle_fileset_paging() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());

        let bundle = store.create_session("test", None, None).unwrap();
        let file_set_id = FileSetId::new();

        let entries: Vec<_> = (0..10)
            .map(|i| FileSetEntry {
                path: format!("/data/file{}.csv", i),
                size: Some(1000),
                content_hash: None,
            })
            .collect();

        bundle.write_fileset(file_set_id, &entries).unwrap();

        // First page
        let (page1, cursor1) = bundle.read_fileset_page(file_set_id, 0, 3).unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(cursor1, Some(3));

        // Second page
        let (page2, cursor2) = bundle.read_fileset_page(file_set_id, 3, 3).unwrap();
        assert_eq!(page2.len(), 3);
        assert_eq!(cursor2, Some(6));

        // Last page
        let (page3, cursor3) = bundle.read_fileset_page(file_set_id, 9, 3).unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(cursor3, None);
    }

    #[test]
    fn test_session_bundle_decisions() {
        let temp_dir = TempDir::new().unwrap();
        let store = SessionStore::with_root(temp_dir.path().to_path_buf());

        let bundle = store.create_session("test", None, None).unwrap();

        let record = DecisionRecord {
            timestamp: Utc::now(),
            actor: "user@example.com".to_string(),
            decision: super::super::types::Decision::Approve,
            target: super::super::types::DecisionTarget {
                proposal_id: ProposalId::new(),
                approval_target_hash: "abc123".to_string(),
            },
            choice_payload: serde_json::json!({"selected_rule_id": "rule_1"}),
            notes: Some("Looks good".to_string()),
        };

        bundle.append_decision(&record).unwrap();

        let records = bundle.read_decisions().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].actor, "user@example.com");
    }
}
