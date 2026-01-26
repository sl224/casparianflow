use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbValue, UnifiedDbRow};
use casparian_protocol::http_types::{
    ApiJobId, Approval, ApprovalStatus, HttpJobStatus, HttpJobType, Job as ApiJob, JobResult,
};
use casparian_protocol::{
    ArtifactV1, JobId, PipelineRunStatus, PluginStatus, ProcessingStatus, RuntimeKind,
};
use casparian_schema::{SchemaContract, SchemaStorage};
use casparian_scout::{Database as ScoutDatabase, ScanConfig, Scanner as ScoutScanner};
use casparian_scout::types::{
    Source, SourceId, SourceType, TaggingRule, TaggingRuleId, Workspace, WorkspaceId,
};

use crate::api_storage::ApiStorage;
use crate::expected_outputs::{ExpectedOutputs, OutputSpec};
use crate::models::{
    DeadLetterReason, ParserHealth, ProcessingJob, TopicConfig, TOPIC_CONFIG_COLUMNS,
};
use crate::queue::{DispatchMetadata, Job, JobDetails, JobQueue, OutputMaterialization};
use crate::sessions::SessionStorage;

/// Parsed state store URL.
#[derive(Debug, Clone)]
pub enum StateStoreUrl {
    Sqlite(PathBuf),
    Postgres(String),
    SqlServer(String),
}

impl StateStoreUrl {
    pub fn parse(raw: &str) -> Result<Self> {
        if let Some(rest) = raw.strip_prefix("sqlite:") {
            let path = rest.trim();
            if path.is_empty() {
                anyhow::bail!("sqlite URL missing path: {raw}");
            }
            return Ok(Self::Sqlite(PathBuf::from(path)));
        }
        if raw.starts_with("postgres://") || raw.starts_with("postgresql://") {
            return Ok(Self::Postgres(raw.to_string()));
        }
        if raw.starts_with("sqlserver://") {
            return Ok(Self::SqlServer(raw.to_string()));
        }
        anyhow::bail!("Unsupported state store URL: {raw}")
    }
}

/// Semantic state store wrapper.
pub struct StateStore {
    inner: Box<dyn StateStoreBackend>,
}

impl StateStore {
    pub fn open(raw: &str) -> Result<Self> {
        let url = StateStoreUrl::parse(raw)?;
        Self::from_url(url)
    }

    pub fn from_url(url: StateStoreUrl) -> Result<Self> {
        match url {
            StateStoreUrl::Sqlite(path) => Ok(Self {
                inner: Box::new(SqliteStateStore::new(path)),
            }),
            StateStoreUrl::Postgres(_) => anyhow::bail!("Postgres state store not yet supported"),
            StateStoreUrl::SqlServer(_) => anyhow::bail!("SQL Server state store not yet supported"),
        }
    }

    pub fn init(&self) -> Result<()> {
        self.inner.init()
    }

    pub fn queue(&self) -> &dyn QueueStore {
        self.inner.queue()
    }

    pub fn api(&self) -> &dyn ApiStore {
        self.inner.api()
    }

    pub fn sessions(&self) -> &dyn SessionStore {
        self.inner.sessions()
    }

    pub fn routing(&self) -> &dyn RoutingStore {
        self.inner.routing()
    }

    pub fn scout(&self) -> &dyn ScoutStore {
        self.inner.scout()
    }

    pub fn artifacts(&self) -> &dyn ArtifactStore {
        self.inner.artifacts()
    }

    pub fn schema_storage(&self) -> Result<SchemaStorage> {
        self.inner.schema_storage()
    }
}

pub trait StateStoreBackend: Send + Sync {
    fn init(&self) -> Result<()>;

    fn queue(&self) -> &dyn QueueStore;
    fn api(&self) -> &dyn ApiStore;
    fn sessions(&self) -> &dyn SessionStore;
    fn routing(&self) -> &dyn RoutingStore;
    fn scout(&self) -> &dyn ScoutStore;
    fn artifacts(&self) -> &dyn ArtifactStore;
    fn schema_storage(&self) -> Result<SchemaStorage>;
}

// ============================================================================
// Queue Store
// ============================================================================

#[derive(Debug, Clone)]
pub struct DispatchData {
    pub rel_path: String,
    pub scan_root: String,
    pub exec_root: Option<String>,
    pub source_code: String,
    pub parser_version: String,
    pub env_hash: String,
    pub artifact_hash: String,
    pub runtime_kind: RuntimeKind,
    pub entrypoint: String,
    pub platform_os: Option<String>,
    pub platform_arch: Option<String>,
    pub signature_verified: bool,
    pub signer_id: Option<String>,
}

impl DispatchData {
    fn from_row(row: &UnifiedDbRow) -> Result<Self> {
        let runtime_str: String = row.get_by_name("runtime_kind")?;
        let runtime_kind = runtime_str
            .parse::<RuntimeKind>()
            .map_err(|err| anyhow::anyhow!(err))?;
        let exec_root: Option<String> = row.get_by_name("exec_root")?;
        let exec_root = exec_root.and_then(|value| {
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        });

        Ok(Self {
            rel_path: row.get_by_name("rel_path")?,
            scan_root: row.get_by_name("scan_root")?,
            exec_root,
            source_code: row.get_by_name("source_code")?,
            parser_version: row.get_by_name("parser_version")?,
            env_hash: row.get_by_name("env_hash")?,
            artifact_hash: row.get_by_name("artifact_hash")?,
            runtime_kind,
            entrypoint: row.get_by_name("entrypoint")?,
            platform_os: row.get_by_name("platform_os")?,
            platform_arch: row.get_by_name("platform_arch")?,
            signature_verified: row.get_by_name("signature_verified")?,
            signer_id: row.get_by_name("signer_id")?,
        })
    }
}

pub trait QueueStore: Send + Sync {
    fn init_queue_schema(&self) -> Result<()>;
    fn init_registry_schema(&self) -> Result<()>;
    fn init_error_handling_schema(&self) -> Result<()>;

    fn pop_job(&self) -> Result<Option<ProcessingJob>>;
    fn complete_job(
        &self,
        job_id: i64,
        completion_status: &str,
        summary: &str,
        quarantine_rows: Option<i64>,
    ) -> Result<()>;
    fn fail_job(&self, job_id: i64, completion_status: &str, error: &str) -> Result<()>;
    fn abort_job(&self, job_id: i64, error: &str) -> Result<()>;
    fn cancel_job(&self, job_id: JobId) -> Result<bool>;
    fn requeue_job(&self, job_id: i64) -> Result<()>;
    fn defer_job(&self, job_id: i64, scheduled_at: i64, reason: Option<&str>) -> Result<()>;
    fn schedule_retry(
        &self,
        job_id: i64,
        next_retry_count: i32,
        error: &str,
        scheduled_at: i64,
    ) -> Result<()>;

    fn list_jobs(
        &self,
        status: Option<ProcessingStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Job>>;
    fn get_job(&self, job_id: JobId) -> Result<Option<Job>>;
    fn count_jobs_by_status(&self) -> Result<HashMap<ProcessingStatus, i64>>;

    fn get_job_details(&self, job_id: i64) -> Result<Option<JobDetails>>;
    fn record_dispatch_metadata(
        &self,
        job_id: i64,
        parser_version: &str,
        parser_fingerprint: &str,
        sink_config_json: &str,
    ) -> Result<()>;
    fn get_dispatch_metadata(&self, job_id: i64) -> Result<Option<DispatchMetadata>>;
    fn insert_output_materialization(&self, record: &OutputMaterialization) -> Result<()>;

    fn record_schema_mismatch(
        &self,
        job_id: i64,
        mismatch: &casparian_protocol::types::SchemaMismatch,
    ) -> Result<()>;
    fn record_parser_success(&self, parser_name: &str) -> Result<()>;
    fn record_parser_failure(&self, parser_name: &str, reason: &str) -> Result<i32>;
    fn pause_parser(&self, parser_name: &str) -> Result<()>;
    fn get_parser_health(&self, parser_name: &str) -> Result<Option<ParserHealth>>;
    fn move_to_dead_letter(
        &self,
        job_id: i64,
        error: &str,
        reason: DeadLetterReason,
    ) -> Result<()>;

    fn load_dispatch_data(&self, plugin_name: &str, file_id: i64) -> Result<DispatchData>;
    fn load_file_generation(&self, file_id: i64) -> Result<Option<(i64, i64)>>;

    fn update_pipeline_run_status_for_job(&self, job_id: i64) -> Result<()>;
    fn set_pipeline_run_running(&self, run_id: &str) -> Result<()>;
    fn update_pipeline_run_status(&self, run_id: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
struct SqliteQueueStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteQueueStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn open_conn(&self) -> Result<DbConnection> {
        DbConnection::open_sqlite_with_busy_timeout(&self.path, self.busy_timeout_ms)
            .context("Failed to open sqlite state store")
    }

    fn with_queue<T>(&self, op: impl FnOnce(&JobQueue) -> Result<T>) -> Result<T> {
        let conn = self.open_conn()?;
        let queue = JobQueue::new(conn);
        op(&queue)
    }

    fn with_conn<T>(&self, op: impl FnOnce(&DbConnection) -> Result<T>) -> Result<T> {
        let conn = self.open_conn()?;
        op(&conn)
    }
}

impl QueueStore for SqliteQueueStore {
    fn init_queue_schema(&self) -> Result<()> {
        self.with_queue(|queue| queue.init_queue_schema())
    }

    fn init_registry_schema(&self) -> Result<()> {
        self.with_queue(|queue| queue.init_registry_schema())
    }

    fn init_error_handling_schema(&self) -> Result<()> {
        self.with_queue(|queue| queue.init_error_handling_schema())
    }

    fn pop_job(&self) -> Result<Option<ProcessingJob>> {
        self.with_queue(|queue| queue.pop_job())
    }

    fn complete_job(
        &self,
        job_id: i64,
        completion_status: &str,
        summary: &str,
        quarantine_rows: Option<i64>,
    ) -> Result<()> {
        self.with_queue(|queue| queue.complete_job(job_id, completion_status, summary, quarantine_rows))
    }

    fn fail_job(&self, job_id: i64, completion_status: &str, error: &str) -> Result<()> {
        self.with_queue(|queue| queue.fail_job(job_id, completion_status, error))
    }

    fn abort_job(&self, job_id: i64, error: &str) -> Result<()> {
        self.with_queue(|queue| queue.abort_job(job_id, error))
    }

    fn cancel_job(&self, job_id: JobId) -> Result<bool> {
        self.with_queue(|queue| queue.cancel_job(job_id))
    }

    fn requeue_job(&self, job_id: i64) -> Result<()> {
        self.with_queue(|queue| queue.requeue_job(job_id))
    }

    fn defer_job(&self, job_id: i64, scheduled_at: i64, reason: Option<&str>) -> Result<()> {
        self.with_queue(|queue| queue.defer_job(job_id, scheduled_at, reason))
    }

    fn schedule_retry(
        &self,
        job_id: i64,
        next_retry_count: i32,
        error: &str,
        scheduled_at: i64,
    ) -> Result<()> {
        self.with_queue(|queue| queue.schedule_retry(job_id, next_retry_count, error, scheduled_at))
    }

    fn list_jobs(
        &self,
        status: Option<ProcessingStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Job>> {
        let limit = usize::try_from(limit).context("limit must be non-negative")?;
        let offset = usize::try_from(offset).context("offset must be non-negative")?;
        self.with_queue(|queue| queue.list_jobs(status, limit, offset))
    }

    fn get_job(&self, job_id: JobId) -> Result<Option<Job>> {
        self.with_queue(|queue| queue.get_job(job_id))
    }

    fn count_jobs_by_status(&self) -> Result<HashMap<ProcessingStatus, i64>> {
        self.with_queue(|queue| queue.count_jobs_by_status())
    }

    fn get_job_details(&self, job_id: i64) -> Result<Option<JobDetails>> {
        self.with_queue(|queue| queue.get_job_details(job_id))
    }

    fn record_dispatch_metadata(
        &self,
        job_id: i64,
        parser_version: &str,
        parser_fingerprint: &str,
        sink_config_json: &str,
    ) -> Result<()> {
        self.with_queue(|queue| {
            queue.record_dispatch_metadata(job_id, parser_version, parser_fingerprint, sink_config_json)
        })
    }

    fn get_dispatch_metadata(&self, job_id: i64) -> Result<Option<DispatchMetadata>> {
        self.with_queue(|queue| queue.get_dispatch_metadata(job_id))
    }

    fn insert_output_materialization(&self, record: &OutputMaterialization) -> Result<()> {
        self.with_queue(|queue| queue.insert_output_materialization(record))
    }

    fn record_schema_mismatch(
        &self,
        job_id: i64,
        mismatch: &casparian_protocol::types::SchemaMismatch,
    ) -> Result<()> {
        self.with_queue(|queue| queue.record_schema_mismatch(job_id, mismatch))
    }

    fn record_parser_success(&self, parser_name: &str) -> Result<()> {
        self.with_queue(|queue| queue.record_parser_success(parser_name))
    }

    fn record_parser_failure(&self, parser_name: &str, reason: &str) -> Result<i32> {
        self.with_queue(|queue| queue.record_parser_failure(parser_name, reason))
    }

    fn pause_parser(&self, parser_name: &str) -> Result<()> {
        self.with_queue(|queue| queue.pause_parser(parser_name))
    }

    fn get_parser_health(&self, parser_name: &str) -> Result<Option<ParserHealth>> {
        self.with_queue(|queue| queue.get_parser_health(parser_name))
    }

    fn move_to_dead_letter(
        &self,
        job_id: i64,
        error: &str,
        reason: DeadLetterReason,
    ) -> Result<()> {
        self.with_queue(|queue| queue.move_to_dead_letter(job_id, error, reason))
    }

    fn load_dispatch_data(&self, plugin_name: &str, file_id: i64) -> Result<DispatchData> {
        self.with_conn(|conn| {
            let row = conn.query_optional(
                r#"
                SELECT
                    sf.rel_path as rel_path,
                    ss.path as scan_root,
                    ss.exec_path as exec_root,
                    pm.source_code,
                    pm.version as parser_version,
                    pm.env_hash,
                    pm.artifact_hash,
                    pm.runtime_kind,
                    pm.entrypoint,
                    pm.platform_os,
                    pm.platform_arch,
                    pm.signature_verified,
                    pm.signer_id
                FROM scout_files sf
                JOIN scout_sources ss ON ss.id = sf.source_id
                JOIN cf_plugin_manifest pm ON pm.plugin_name = ? AND pm.status IN (?, ?)
                WHERE sf.id = ?
                ORDER BY pm.created_at DESC
                LIMIT 1
                "#,
                &[
                    DbValue::from(plugin_name),
                    DbValue::from(PluginStatus::Active.as_str()),
                    DbValue::from(PluginStatus::Deployed.as_str()),
                    DbValue::from(file_id),
                ],
            )?;
            let row = row.ok_or_else(|| anyhow::anyhow!("Dispatch data missing"))?;
            DispatchData::from_row(&row)
        })
    }

    fn load_file_generation(&self, file_id: i64) -> Result<Option<(i64, i64)>> {
        self.with_conn(|conn| {
            let row = conn.query_optional(
                "SELECT mtime, size FROM scout_files WHERE id = ?",
                &[DbValue::from(file_id)],
            )?;
            let Some(row) = row else {
                return Ok(None);
            };
            let mtime: i64 = row.get_by_name("mtime")?;
            let size: i64 = row.get_by_name("size")?;
            Ok(Some((mtime, size)))
        })
    }

    fn update_pipeline_run_status_for_job(&self, job_id: i64) -> Result<()> {
        self.with_conn(|conn| {
            let run_id = conn
                .query_optional(
                    "SELECT pipeline_run_id FROM cf_processing_queue WHERE id = ?",
                    &[DbValue::from(job_id)],
                )?
                .and_then(|row| row.get_by_name::<String>("pipeline_run_id").ok());
            let Some(run_id) = run_id else {
                return Ok(());
            };
            update_pipeline_run_status(conn, &run_id)
        })
    }

    fn set_pipeline_run_running(&self, run_id: &str) -> Result<()> {
        self.with_conn(|conn| set_pipeline_run_running(conn, run_id))
    }

    fn update_pipeline_run_status(&self, run_id: &str) -> Result<()> {
        self.with_conn(|conn| update_pipeline_run_status(conn, run_id))
    }
}

fn table_exists(conn: &DbConnection, table: &str) -> Result<bool> {
    Ok(conn.table_exists(table)?)
}

fn set_pipeline_run_running(conn: &DbConnection, run_id: &str) -> Result<()> {
    if !table_exists(conn, "cf_pipeline_runs")? {
        return Ok(());
    }
    conn.execute(
        r#"
            UPDATE cf_pipeline_runs
            SET status = ?,
                started_at = COALESCE(started_at, ?)
            WHERE id = ?
            "#,
        &[
            DbValue::from(PipelineRunStatus::Running.as_str()),
            DbValue::from(now_millis()),
            DbValue::from(run_id),
        ],
    )?;
    Ok(())
}

fn update_pipeline_run_status(conn: &DbConnection, run_id: &str) -> Result<()> {
    if !table_exists(conn, "cf_pipeline_runs")? {
        return Ok(());
    }

    let row = conn.query_optional(
        &format!(
            r#"
            SELECT
                SUM(CASE WHEN status IN ('{failed}', '{aborted}') THEN 1 ELSE 0 END) AS failed,
                SUM(CASE WHEN status IN ('{queued}', '{running}') THEN 1 ELSE 0 END) AS active,
                SUM(CASE WHEN status = '{completed}' THEN 1 ELSE 0 END) AS completed
            FROM cf_processing_queue
            WHERE pipeline_run_id = ?
            "#,
            failed = ProcessingStatus::Failed.as_str(),
            aborted = ProcessingStatus::Aborted.as_str(),
            queued = ProcessingStatus::Queued.as_str(),
            running = ProcessingStatus::Running.as_str(),
            completed = ProcessingStatus::Completed.as_str(),
        ),
        &[DbValue::from(run_id)],
    )?;

    let Some(row) = row else {
        return Ok(());
    };

    let failed: i64 = row.get_by_name("failed").unwrap_or(0);
    let active: i64 = row.get_by_name("active").unwrap_or(0);
    let completed: i64 = row.get_by_name("completed").unwrap_or(0);

    if failed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = ?, completed_at = ? WHERE id = ?",
            &[
                DbValue::from(PipelineRunStatus::Failed.as_str()),
                DbValue::from(now_millis()),
                DbValue::from(run_id),
            ],
        )?;
        return Ok(());
    }

    if active > 0 {
        set_pipeline_run_running(conn, run_id)?;
        return Ok(());
    }

    if completed > 0 {
        conn.execute(
            "UPDATE cf_pipeline_runs SET status = ?, completed_at = ? WHERE id = ?",
            &[
                DbValue::from(PipelineRunStatus::Completed.as_str()),
                DbValue::from(now_millis()),
                DbValue::from(run_id),
            ],
        )?;
    }

    Ok(())
}

// ============================================================================
// API Store
// ============================================================================

pub trait ApiStore: Send + Sync {
    fn init_schema(&self) -> Result<()>;
    fn create_approval(
        &self,
        approval_id: &str,
        operation: &casparian_protocol::ApprovalOperation,
        summary: &str,
        expires_in: chrono::Duration,
    ) -> Result<()>;

    fn create_job(
        &self,
        job_type: HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output_sink: Option<&str>,
        approval_id: Option<&str>,
        job_spec_json: Option<&str>,
    ) -> Result<ApiJobId>;
    fn get_job(&self, job_id: ApiJobId) -> Result<Option<ApiJob>>;
    fn list_jobs(
        &self,
        status: Option<HttpJobStatus>,
        limit: usize,
    ) -> Result<Vec<ApiJob>>;
    fn update_job_status(
        &self,
        job_id: ApiJobId,
        status: HttpJobStatus,
    ) -> Result<()>;
    fn update_job_progress(
        &self,
        job_id: ApiJobId,
        phase: &str,
        items_done: u64,
        items_total: Option<u64>,
        message: Option<&str>,
    ) -> Result<()>;
    fn update_job_result(&self, job_id: ApiJobId, result: &JobResult) -> Result<()>;
    fn update_job_error(&self, job_id: ApiJobId, error: &str) -> Result<()>;
    fn cancel_job(&self, job_id: ApiJobId) -> Result<bool>;

    fn list_approvals(
        &self,
        status: Option<ApprovalStatus>,
    ) -> Result<Vec<Approval>>;
    fn get_approval(&self, approval_id: &str) -> Result<Option<Approval>>;
    fn approve(&self, approval_id: &str, decided_by: Option<&str>) -> Result<bool>;
    fn reject(
        &self,
        approval_id: &str,
        decided_by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<bool>;
    fn link_approval_to_job(&self, approval_id: &str, job_id: ApiJobId) -> Result<()>;
    fn expire_approvals(&self) -> Result<usize>;
}

#[derive(Debug, Clone)]
struct SqliteApiStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteApiStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn with_storage<T>(&self, op: impl FnOnce(&ApiStorage) -> Result<T>) -> Result<T> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(&self.path, self.busy_timeout_ms)?;
        let storage = ApiStorage::new(conn);
        op(&storage)
    }
}

impl ApiStore for SqliteApiStore {
    fn init_schema(&self) -> Result<()> {
        self.with_storage(|storage| storage.init_schema())
    }

    fn create_approval(
        &self,
        approval_id: &str,
        operation: &casparian_protocol::ApprovalOperation,
        summary: &str,
        expires_in: chrono::Duration,
    ) -> Result<()> {
        self.with_storage(|storage| storage.create_approval(approval_id, operation, summary, expires_in))
    }

    fn create_job(
        &self,
        job_type: HttpJobType,
        plugin_name: &str,
        plugin_version: Option<&str>,
        input_dir: &str,
        output_sink: Option<&str>,
        approval_id: Option<&str>,
        job_spec_json: Option<&str>,
    ) -> Result<ApiJobId> {
        self.with_storage(|storage| {
            storage.create_job(
                job_type,
                plugin_name,
                plugin_version,
                input_dir,
                output_sink,
                approval_id,
                job_spec_json,
            )
        })
    }

    fn get_job(&self, job_id: ApiJobId) -> Result<Option<ApiJob>> {
        self.with_storage(|storage| storage.get_job(job_id))
    }

    fn list_jobs(
        &self,
        status: Option<HttpJobStatus>,
        limit: usize,
    ) -> Result<Vec<ApiJob>> {
        self.with_storage(|storage| storage.list_jobs(status, limit))
    }

    fn update_job_status(
        &self,
        job_id: ApiJobId,
        status: HttpJobStatus,
    ) -> Result<()> {
        self.with_storage(|storage| storage.update_job_status(job_id, status))
    }

    fn update_job_progress(
        &self,
        job_id: ApiJobId,
        phase: &str,
        items_done: u64,
        items_total: Option<u64>,
        message: Option<&str>,
    ) -> Result<()> {
        self.with_storage(|storage| {
            storage.update_job_progress(job_id, phase, items_done, items_total, message)
        })
    }

    fn update_job_result(&self, job_id: ApiJobId, result: &JobResult) -> Result<()> {
        self.with_storage(|storage| storage.update_job_result(job_id, result))
    }

    fn update_job_error(&self, job_id: ApiJobId, error: &str) -> Result<()> {
        self.with_storage(|storage| storage.update_job_error(job_id, error))
    }

    fn cancel_job(&self, job_id: ApiJobId) -> Result<bool> {
        self.with_storage(|storage| storage.cancel_job(job_id))
    }

    fn list_approvals(
        &self,
        status: Option<ApprovalStatus>,
    ) -> Result<Vec<Approval>> {
        self.with_storage(|storage| storage.list_approvals(status))
    }

    fn get_approval(&self, approval_id: &str) -> Result<Option<Approval>> {
        self.with_storage(|storage| storage.get_approval(approval_id))
    }

    fn approve(&self, approval_id: &str, decided_by: Option<&str>) -> Result<bool> {
        self.with_storage(|storage| storage.approve(approval_id, decided_by))
    }

    fn reject(
        &self,
        approval_id: &str,
        decided_by: Option<&str>,
        reason: Option<&str>,
    ) -> Result<bool> {
        self.with_storage(|storage| storage.reject(approval_id, decided_by, reason))
    }

    fn link_approval_to_job(&self, approval_id: &str, job_id: ApiJobId) -> Result<()> {
        self.with_storage(|storage| storage.link_approval_to_job(approval_id, job_id))
    }

    fn expire_approvals(&self) -> Result<usize> {
        self.with_storage(|storage| storage.expire_approvals())
    }
}

// ============================================================================
// Session Store
// ============================================================================

pub trait SessionStore: Send + Sync {
    fn init_schema(&self) -> Result<()>;
    fn create_session(&self, intent_text: &str, input_dir: Option<&str>) -> Result<casparian_intent::SessionId>;
    fn get_session(
        &self,
        session_id: casparian_intent::SessionId,
    ) -> Result<Option<casparian_intent::Session>>;
    fn list_sessions(
        &self,
        state: Option<casparian_intent::IntentState>,
        limit: usize,
    ) -> Result<Vec<casparian_intent::Session>>;
    fn list_sessions_needing_input(&self, limit: usize) -> Result<Vec<casparian_intent::Session>>;
    fn update_session_state(
        &self,
        session_id: casparian_intent::SessionId,
        new_state: casparian_intent::IntentState,
    ) -> Result<bool>;
    fn cancel_session(&self, session_id: casparian_intent::SessionId) -> Result<bool>;
}

#[derive(Debug, Clone)]
struct SqliteSessionStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteSessionStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn with_storage<T>(&self, op: impl FnOnce(&SessionStorage) -> Result<T>) -> Result<T> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(&self.path, self.busy_timeout_ms)?;
        let storage = SessionStorage::new(conn);
        op(&storage)
    }
}

impl SessionStore for SqliteSessionStore {
    fn init_schema(&self) -> Result<()> {
        self.with_storage(|storage| storage.init_schema())
    }

    fn create_session(
        &self,
        intent_text: &str,
        input_dir: Option<&str>,
    ) -> Result<casparian_intent::SessionId> {
        self.with_storage(|storage| storage.create_session(intent_text, input_dir))
    }

    fn get_session(
        &self,
        session_id: casparian_intent::SessionId,
    ) -> Result<Option<casparian_intent::Session>> {
        self.with_storage(|storage| storage.get_session(session_id))
    }

    fn list_sessions(
        &self,
        state: Option<casparian_intent::IntentState>,
        limit: usize,
    ) -> Result<Vec<casparian_intent::Session>> {
        self.with_storage(|storage| storage.list_sessions(state, limit))
    }

    fn list_sessions_needing_input(&self, limit: usize) -> Result<Vec<casparian_intent::Session>> {
        self.with_storage(|storage| storage.list_sessions_needing_input(limit))
    }

    fn update_session_state(
        &self,
        session_id: casparian_intent::SessionId,
        new_state: casparian_intent::IntentState,
    ) -> Result<bool> {
        self.with_storage(|storage| storage.update_session_state(session_id, new_state))
    }

    fn cancel_session(&self, session_id: casparian_intent::SessionId) -> Result<bool> {
        self.with_storage(|storage| storage.cancel_session(session_id))
    }
}

// ============================================================================
// Routing Store
// ============================================================================

pub trait RoutingStore: Send + Sync {
    fn list_topic_configs(&self) -> Result<Vec<TopicConfig>>;
    fn expected_outputs_for_plugin(
        &self,
        plugin_name: &str,
        parser_version: Option<&str>,
    ) -> Result<Vec<OutputSpec>>;
    fn deploy_plugin(&self, request: PluginDeployRequest) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct PluginDeployRequest {
    pub plugin_name: String,
    pub version: String,
    pub runtime_kind: RuntimeKind,
    pub entrypoint: String,
    pub platform_os: Option<String>,
    pub platform_arch: Option<String>,
    pub source_code: String,
    pub source_hash: String,
    pub env_hash: String,
    pub artifact_hash: String,
    pub manifest_json: String,
    pub protocol_version: String,
    pub schema_artifacts_json: String,
    pub outputs_json: String,
    pub signature_verified: bool,
    pub signer_id: Option<String>,
    pub created_at: i64,
    pub deployed_at: i64,
    pub publisher_name: String,
    pub publisher_email: Option<String>,
    pub azure_oid: Option<String>,
    pub system_requirements_json: Option<String>,
    pub lockfile_content: Option<String>,
    pub contracts: Vec<(String, casparian_schema::LockedSchema)>,
}

#[derive(Debug, Clone)]
struct SqliteRoutingStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteRoutingStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn with_conn<T>(&self, op: impl FnOnce(&DbConnection) -> Result<T>) -> Result<T> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(&self.path, self.busy_timeout_ms)?;
        op(&conn)
    }
}

impl RoutingStore for SqliteRoutingStore {
    fn list_topic_configs(&self) -> Result<Vec<TopicConfig>> {
        self.with_conn(|conn| {
            let rows = conn.query_all(
                &format!(
                    "SELECT {} FROM cf_topic_config ORDER BY id ASC",
                    TOPIC_CONFIG_COLUMNS.join(", ")
                ),
                &[],
            )?;
            rows.into_iter()
                .map(|row| TopicConfig::from_row(&row).map_err(anyhow::Error::from))
                .collect()
        })
    }

    fn expected_outputs_for_plugin(
        &self,
        plugin_name: &str,
        parser_version: Option<&str>,
    ) -> Result<Vec<OutputSpec>> {
        self.with_conn(|conn| ExpectedOutputs::list_for_plugin(conn, plugin_name, parser_version))
    }

    fn deploy_plugin(&self, request: PluginDeployRequest) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("BEGIN TRANSACTION", &[])?;

            // Upsert plugin environment (lockfile)
            if let Some(lockfile) = request.lockfile_content.as_ref() {
                if !lockfile.is_empty() {
                    if let Err(e) = conn.execute(
                        r#"
                        INSERT INTO cf_plugin_environment (hash, lockfile_content, size_mb, last_used, created_at)
                        VALUES (?, ?, ?, ?, ?)
                        ON CONFLICT(hash) DO UPDATE SET last_used = ?
                        "#,
                        &[
                            DbValue::from(request.env_hash.as_str()),
                            DbValue::from(lockfile.as_str()),
                            DbValue::from(lockfile.len() as f64 / 1_000_000.0),
                            DbValue::from(request.created_at),
                            DbValue::from(request.created_at),
                            DbValue::from(request.created_at),
                        ],
                    ) {
                        let _ = conn.execute("ROLLBACK", &[]);
                        return Err(e.into());
                    }
                }
            }

            let publisher_email = request
                .publisher_email
                .as_deref()
                .map(DbValue::from)
                .unwrap_or(DbValue::Null);
            let azure_oid = request
                .azure_oid
                .as_deref()
                .map(DbValue::from)
                .unwrap_or(DbValue::Null);
            let system_requirements = request
                .system_requirements_json
                .as_deref()
                .map(DbValue::from)
                .unwrap_or(DbValue::Null);

            if let Err(e) = conn.execute(
                r#"
                    INSERT INTO cf_plugin_manifest
                    (plugin_name, version, runtime_kind, entrypoint, platform_os, platform_arch,
                     source_code, source_hash, status,
                     env_hash, artifact_hash, manifest_json, protocol_version, schema_artifacts_json,
                     outputs_json, signature_verified, signer_id,
                     created_at, deployed_at,
                     publisher_name, publisher_email, azure_oid, system_requirements)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                &[
                    DbValue::from(request.plugin_name.as_str()),
                    DbValue::from(request.version.as_str()),
                    DbValue::from(request.runtime_kind.as_str()),
                    DbValue::from(request.entrypoint.as_str()),
                    request
                        .platform_os
                        .as_deref()
                        .map(DbValue::from)
                        .unwrap_or(DbValue::Null),
                    request
                        .platform_arch
                        .as_deref()
                        .map(DbValue::from)
                        .unwrap_or(DbValue::Null),
                    DbValue::from(request.source_code.as_str()),
                    DbValue::from(request.source_hash.as_str()),
                    DbValue::from(PluginStatus::Active.as_str()),
                    DbValue::from(request.env_hash.as_str()),
                    DbValue::from(request.artifact_hash.as_str()),
                    DbValue::from(request.manifest_json.as_str()),
                    DbValue::from(request.protocol_version.as_str()),
                    DbValue::from(request.schema_artifacts_json.as_str()),
                    DbValue::from(request.outputs_json.as_str()),
                    DbValue::from(request.signature_verified),
                    request
                        .signer_id
                        .as_deref()
                        .map(DbValue::from)
                        .unwrap_or(DbValue::Null),
                    DbValue::from(request.created_at),
                    DbValue::from(request.deployed_at),
                    DbValue::from(request.publisher_name.as_str()),
                    publisher_email,
                    azure_oid,
                    system_requirements,
                ],
            ) {
                let _ = conn.execute("ROLLBACK", &[]);
                return Err(e.into());
            }

            // Insert schema contracts (fail if schema changed without version bump)
            let schema_storage = SchemaStorage::new(conn.clone()).map_err(|e| anyhow::anyhow!(e))?;
            for (scope_id, locked_schema) in &request.contracts {
                if let Some(existing) = schema_storage
                    .get_contract_for_scope(scope_id)
                    .context("Failed to load existing schema contract")?
                {
                    let existing_hash = existing
                        .schemas
                        .get(0)
                        .map(|schema| schema.content_hash.as_str())
                        .unwrap_or("");
                    if existing_hash != locked_schema.content_hash {
                        let _ = conn.execute("ROLLBACK", &[]);
                        anyhow::bail!(
                            "Schema changed for output '{}' without version bump. \
Update version '{}' or delete the database.",
                            locked_schema.name,
                            request.version
                        );
                    }
                    let _ = conn.execute("ROLLBACK", &[]);
                    anyhow::bail!(
                        "Schema contract already exists for output '{}' at version '{}'. \
Delete the database to republish.",
                        locked_schema.name,
                        request.version
                    );
                }

                let contract =
                    SchemaContract::new(scope_id, locked_schema.clone(), &request.publisher_name)
                        .with_logic_hash(Some(request.source_hash.clone()));
                if let Err(e) = schema_storage.save_contract(&contract) {
                    let _ = conn.execute("ROLLBACK", &[]);
                    return Err(anyhow::anyhow!(e));
                }
            }

            // Deactivate previous versions
            if let Err(e) = conn.execute(
                r#"
                    UPDATE cf_plugin_manifest
                    SET status = ?
                    WHERE plugin_name = ? AND version != ? AND status = ?
                    "#,
                &[
                    DbValue::from(PluginStatus::Superseded.as_str()),
                    DbValue::from(request.plugin_name.as_str()),
                    DbValue::from(request.version.as_str()),
                    DbValue::from(PluginStatus::Active.as_str()),
                ],
            ) {
                let _ = conn.execute("ROLLBACK", &[]);
                return Err(e.into());
            }

            if let Err(e) = conn.execute("COMMIT", &[]) {
                let _ = conn.execute("ROLLBACK", &[]);
                return Err(e.into());
            }

            Ok(())
        })
    }
}

// ============================================================================
// Scout Store
// ============================================================================

#[derive(Debug, Clone)]
pub struct ScoutTagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub struct ScoutTagStats {
    pub total_files: i64,
    pub untagged_files: i64,
    pub tags: Vec<ScoutTagCount>,
}

#[derive(Debug, Clone)]
pub struct ScoutSourceRecord {
    pub source: Source,
    pub file_count: i64,
}

pub trait ScoutStore: Send + Sync {
    fn init_schema(&self) -> Result<()>;

    fn list_sources(&self, workspace_id: WorkspaceId) -> Result<Vec<Source>>;
    fn list_sources_with_counts(&self, workspace_id: WorkspaceId) -> Result<Vec<ScoutSourceRecord>>;
    fn get_source(&self, id: &SourceId) -> Result<Option<Source>>;
    fn get_source_by_name(&self, workspace_id: &WorkspaceId, name: &str) -> Result<Option<Source>>;
    fn get_source_by_path(&self, workspace_id: &WorkspaceId, path: &str) -> Result<Option<Source>>;
    fn upsert_source(&self, source: &Source) -> Result<()>;
    fn delete_source(&self, id: &SourceId) -> Result<bool>;
    fn touch_source(&self, id: &SourceId) -> Result<()>;

    fn list_tagging_rules(&self, workspace_id: &WorkspaceId) -> Result<Vec<TaggingRule>>;
    fn get_tagging_rule(&self, id: &TaggingRuleId) -> Result<Option<TaggingRule>>;
    fn upsert_tagging_rule(&self, rule: &TaggingRule) -> Result<()>;
    fn delete_tagging_rule(&self, id: &TaggingRuleId) -> Result<bool>;

    fn tag_file(&self, file_id: i64, tag: &str) -> Result<()>;
    fn tag_file_by_rule(&self, file_id: i64, tag: &str, rule_id: &TaggingRuleId)
        -> Result<()>;

    fn tag_stats(&self, workspace_id: WorkspaceId, source_id: SourceId) -> Result<ScoutTagStats>;

    fn ensure_default_workspace(&self) -> Result<Workspace>;
    fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>>;

    fn check_source_overlap(&self, workspace_id: &WorkspaceId, new_path: &Path) -> Result<()>;

    fn scanner(&self, config: ScanConfig) -> Result<ScoutScanner>;
}

#[derive(Debug, Clone)]
struct SqliteScoutStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteScoutStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn open_db(&self) -> Result<ScoutDatabase> {
        ScoutDatabase::open_with_busy_timeout(&self.path, self.busy_timeout_ms)
            .context("Failed to open scout state store")
    }
}

impl ScoutStore for SqliteScoutStore {
    fn init_schema(&self) -> Result<()> {
        let _ = self.open_db()?;
        Ok(())
    }

    fn list_sources(&self, workspace_id: WorkspaceId) -> Result<Vec<Source>> {
        let db = self.open_db()?;
        Ok(db.list_sources_by_mru(&workspace_id)?)
    }

    fn list_sources_with_counts(&self, workspace_id: WorkspaceId) -> Result<Vec<ScoutSourceRecord>> {
        let db = self.open_db()?;
        let rows = db.conn().query_all(
            "SELECT id, name, source_type, path, exec_path, poll_interval_secs, enabled, file_count \
             FROM scout_sources WHERE workspace_id = ? AND enabled = 1 ORDER BY updated_at DESC",
            &[DbValue::from(workspace_id.to_string())],
        )?;

        let mut sources = Vec::with_capacity(rows.len());
        for row in rows {
            let id_i64: i64 = row.get(0)?;
            let id = SourceId::try_from(id_i64)?;
            let name: String = row.get(1)?;
            let source_type_raw: String = row.get(2)?;
            let source_type: SourceType = serde_json::from_str(&source_type_raw)?;
            let path: String = row.get(3)?;
            let exec_path: Option<String> = row.get(4).ok().flatten();
            let poll_interval_secs: i64 = row.get(5)?;
            let enabled_raw: i64 = row.get(6)?;
            let file_count: i64 = row.get(7)?;

            sources.push(ScoutSourceRecord {
                source: Source {
                    workspace_id,
                    id,
                    name,
                    source_type,
                    path,
                    exec_path,
                    poll_interval_secs: poll_interval_secs.max(0) as u64,
                    enabled: enabled_raw != 0,
                },
                file_count,
            });
        }

        Ok(sources)
    }

    fn get_source(&self, id: &SourceId) -> Result<Option<Source>> {
        let db = self.open_db()?;
        Ok(db.get_source(id)?)
    }

    fn get_source_by_name(&self, workspace_id: &WorkspaceId, name: &str) -> Result<Option<Source>> {
        let db = self.open_db()?;
        Ok(db.get_source_by_name(workspace_id, name)?)
    }

    fn get_source_by_path(&self, workspace_id: &WorkspaceId, path: &str) -> Result<Option<Source>> {
        let db = self.open_db()?;
        Ok(db.get_source_by_path(workspace_id, path)?)
    }

    fn upsert_source(&self, source: &Source) -> Result<()> {
        let db = self.open_db()?;
        db.upsert_source(source)?;
        Ok(())
    }

    fn delete_source(&self, id: &SourceId) -> Result<bool> {
        let db = self.open_db()?;
        Ok(db.delete_source(id)?)
    }

    fn touch_source(&self, id: &SourceId) -> Result<()> {
        let db = self.open_db()?;
        db.touch_source(id)?;
        Ok(())
    }

    fn list_tagging_rules(&self, workspace_id: &WorkspaceId) -> Result<Vec<TaggingRule>> {
        let db = self.open_db()?;
        Ok(db.list_tagging_rules(workspace_id)?)
    }

    fn get_tagging_rule(&self, id: &TaggingRuleId) -> Result<Option<TaggingRule>> {
        let db = self.open_db()?;
        Ok(db.get_tagging_rule(id)?)
    }

    fn upsert_tagging_rule(&self, rule: &TaggingRule) -> Result<()> {
        let db = self.open_db()?;
        db.upsert_tagging_rule(rule)?;
        Ok(())
    }

    fn delete_tagging_rule(&self, id: &TaggingRuleId) -> Result<bool> {
        let db = self.open_db()?;
        Ok(db.delete_tagging_rule(id)?)
    }

    fn tag_file(&self, file_id: i64, tag: &str) -> Result<()> {
        let db = self.open_db()?;
        db.tag_file(file_id, tag)?;
        Ok(())
    }

    fn tag_file_by_rule(
        &self,
        file_id: i64,
        tag: &str,
        rule_id: &TaggingRuleId,
    ) -> Result<()> {
        let db = self.open_db()?;
        db.tag_file_by_rule(file_id, tag, rule_id)?;
        Ok(())
    }

    fn tag_stats(&self, workspace_id: WorkspaceId, source_id: SourceId) -> Result<ScoutTagStats> {
        let db = self.open_db()?;

        let total_files = db.conn().query_scalar::<i64>(
            "SELECT COUNT(*) FROM scout_files WHERE workspace_id = ? AND source_id = ?",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(source_id.as_i64()),
            ],
        )?;

        let rows = db.conn().query_all(
            "SELECT t.tag, COUNT(*) AS count \
             FROM scout_file_tags t \
             JOIN scout_files f ON f.id = t.file_id AND f.workspace_id = t.workspace_id \
             WHERE f.workspace_id = ? AND f.source_id = ? \
             GROUP BY t.tag \
             ORDER BY count DESC, t.tag",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(source_id.as_i64()),
            ],
        )?;

        let mut tags = Vec::with_capacity(rows.len());
        for row in rows {
            let tag: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            if count > 0 {
                tags.push(ScoutTagCount { tag, count });
            }
        }

        let untagged_files = db.conn().query_scalar::<i64>(
            "SELECT COUNT(*) \
             FROM scout_files f \
             LEFT JOIN scout_file_tags t \
                ON t.file_id = f.id AND t.workspace_id = f.workspace_id \
             WHERE f.workspace_id = ? AND f.source_id = ? AND t.file_id IS NULL",
            &[
                DbValue::from(workspace_id.to_string()),
                DbValue::from(source_id.as_i64()),
            ],
        )?;

        Ok(ScoutTagStats {
            total_files,
            untagged_files,
            tags,
        })
    }

    fn ensure_default_workspace(&self) -> Result<Workspace> {
        let db = self.open_db()?;
        Ok(db.ensure_default_workspace()?)
    }

    fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        let db = self.open_db()?;
        Ok(db.get_workspace(id)?)
    }

    fn check_source_overlap(&self, workspace_id: &WorkspaceId, new_path: &Path) -> Result<()> {
        let db = self.open_db()?;
        db.check_source_overlap(workspace_id, new_path)?;
        Ok(())
    }

    fn scanner(&self, config: ScanConfig) -> Result<ScoutScanner> {
        let db = self.open_db()?;
        Ok(ScoutScanner::with_config(db, config))
    }
}

// ============================================================================
// Artifact Store
// ============================================================================

#[derive(Debug, Clone)]
pub struct JobArtifactRecord {
    pub job_id: i64,
    pub kind: String,
    pub name: String,
    pub uri: String,
    pub table_name: Option<String>,
    pub rows: Option<i64>,
    pub created_at: i64,
}

pub trait ArtifactStore: Send + Sync {
    fn init_schema(&self) -> Result<()>;
    fn insert_job_artifacts(&self, job_id: i64, artifacts: &[ArtifactV1]) -> Result<()>;
    fn list_job_artifacts(&self, job_id: i64) -> Result<Vec<JobArtifactRecord>>;
}

#[derive(Debug, Clone)]
struct SqliteArtifactStore {
    path: PathBuf,
    busy_timeout_ms: u64,
}

impl SqliteArtifactStore {
    fn new(path: PathBuf, busy_timeout_ms: u64) -> Self {
        Self {
            path,
            busy_timeout_ms,
        }
    }

    fn with_conn<T>(&self, op: impl FnOnce(&DbConnection) -> Result<T>) -> Result<T> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(&self.path, self.busy_timeout_ms)?;
        op(&conn)
    }
}

impl ArtifactStore for SqliteArtifactStore {
    fn init_schema(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS cf_job_artifacts (
                    job_id BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    name TEXT NOT NULL,
                    uri TEXT NOT NULL,
                    table_name TEXT,
                    rows BIGINT,
                    created_at BIGINT NOT NULL,
                    UNIQUE(job_id, kind, name, uri)
                );
                CREATE INDEX IF NOT EXISTS ix_job_artifacts_job ON cf_job_artifacts(job_id);
                "#,
            )?;
            Ok(())
        })
    }

    fn insert_job_artifacts(&self, job_id: i64, artifacts: &[ArtifactV1]) -> Result<()> {
        self.with_conn(|conn| {
            let now = now_millis();
            let sql = r#"
                INSERT OR IGNORE INTO cf_job_artifacts
                    (job_id, kind, name, uri, table_name, rows, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
            "#;

            for artifact in artifacts {
                let (kind, name, uri, table_name, rows) = match artifact {
                    ArtifactV1::Output {
                        output_name,
                        sink_uri,
                        table,
                        rows,
                        ..
                    } => (
                        "output",
                        output_name.as_str(),
                        sink_uri.as_str(),
                        table.as_deref(),
                        rows.map(|r| i64::try_from(r)),
                    ),
                    ArtifactV1::Quarantine {
                        output_name,
                        sink_uri,
                        table,
                        rows,
                    } => (
                        "quarantine",
                        output_name.as_str(),
                        sink_uri.as_str(),
                        table.as_deref(),
                        rows.map(|r| i64::try_from(r)),
                    ),
                    ArtifactV1::Log { name, uri } => ("log", name.as_str(), uri.as_str(), None, None),
                    ArtifactV1::Other { name, uri } => {
                        let Some(uri) = uri.as_ref() else {
                            continue;
                        };
                        ("other", name.as_str(), uri.as_str(), None, None)
                    }
                };

                let rows = rows.transpose().context("artifact row count overflow")?;
                conn.execute(
                    sql,
                    &[
                        DbValue::from(job_id),
                        DbValue::from(kind),
                        DbValue::from(name),
                        DbValue::from(uri),
                        DbValue::from(table_name),
                        DbValue::from(rows),
                        DbValue::from(now),
                    ],
                )?;
            }

            Ok(())
        })
    }

    fn list_job_artifacts(&self, job_id: i64) -> Result<Vec<JobArtifactRecord>> {
        self.with_conn(|conn| {
            let rows = conn.query_all(
                r#"
                SELECT job_id, kind, name, uri, table_name, rows, created_at
                FROM cf_job_artifacts
                WHERE job_id = ?
                ORDER BY created_at ASC
                "#,
                &[DbValue::from(job_id)],
            )?;

            let mut records = Vec::with_capacity(rows.len());
            for row in rows {
                records.push(JobArtifactRecord {
                    job_id: row.get_by_name("job_id")?,
                    kind: row.get_by_name("kind")?,
                    name: row.get_by_name("name")?,
                    uri: row.get_by_name("uri")?,
                    table_name: row.get_by_name("table_name")?,
                    rows: row.get_by_name("rows")?,
                    created_at: row.get_by_name("created_at")?,
                });
            }
            Ok(records)
        })
    }
}

// ============================================================================
// SQLite State Store
// ============================================================================

#[derive(Debug, Clone)]
struct SqliteStateStore {
    path: PathBuf,
    queue: SqliteQueueStore,
    api: SqliteApiStore,
    sessions: SqliteSessionStore,
    routing: SqliteRoutingStore,
    scout: SqliteScoutStore,
    artifacts: SqliteArtifactStore,
}

impl SqliteStateStore {
    fn new(path: PathBuf) -> Self {
        let fast_timeout_ms = 200;
        let bulk_timeout_ms = 5000;
        Self {
            path: path.clone(),
            queue: SqliteQueueStore::new(path.clone(), fast_timeout_ms),
            api: SqliteApiStore::new(path.clone(), fast_timeout_ms),
            sessions: SqliteSessionStore::new(path.clone(), fast_timeout_ms),
            routing: SqliteRoutingStore::new(path.clone(), fast_timeout_ms),
            scout: SqliteScoutStore::new(path.clone(), bulk_timeout_ms),
            artifacts: SqliteArtifactStore::new(path, fast_timeout_ms),
        }
    }
}

impl StateStoreBackend for SqliteStateStore {
    fn init(&self) -> Result<()> {
        self.queue.init_queue_schema()?;
        self.queue.init_registry_schema()?;
        self.queue.init_error_handling_schema()?;
        self.api.init_schema()?;
        self.sessions.init_schema()?;
        self.scout.init_schema()?;
        self.artifacts.init_schema()?;
        Ok(())
    }

    fn queue(&self) -> &dyn QueueStore {
        &self.queue
    }

    fn api(&self) -> &dyn ApiStore {
        &self.api
    }

    fn sessions(&self) -> &dyn SessionStore {
        &self.sessions
    }

    fn routing(&self) -> &dyn RoutingStore {
        &self.routing
    }

    fn scout(&self) -> &dyn ScoutStore {
        &self.scout
    }

    fn artifacts(&self) -> &dyn ArtifactStore {
        &self.artifacts
    }

    fn schema_storage(&self) -> Result<SchemaStorage> {
        let conn = DbConnection::open_sqlite_with_busy_timeout(&self.path, 200)?;
        SchemaStorage::new(conn).map_err(|e| anyhow::anyhow!(e))
    }
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
