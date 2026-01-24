//! Core Module - Single-Owner State Management
//!
//! This module implements a synchronous, message-passing architecture for MCP.
//! The Core thread exclusively owns all mutable state (jobs, approvals, DB),
//! receiving Commands and emitting Events via bounded channels.
//!
//! # Design Principles
//!
//! 1. **Single owner**: Core owns JobManager, ApprovalManager, DbConnection
//! 2. **Message passing**: All state changes flow through Command/Event channels
//! 3. **No async**: Synchronous execution using std threads and channels
//! 4. **No locks on managers**: Arc<Mutex<>> pattern replaced with message passing

mod command;
mod event;

pub use command::{Command, Responder};
pub use event::Event;
pub use casparian_worker::cancel::CancellationToken;

use crate::approvals::ApprovalManager;
use crate::jobs::{Job, JobId, JobManager, JobProgress, JobSpec};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use tracing::{debug, error, info, warn};

/// Configuration for Core backend selection.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub db_path: PathBuf,
    pub control_addr: Option<String>,
    pub standalone_db_writer: bool,
}

impl CoreConfig {
    pub fn db_only(db_path: PathBuf) -> Self {
        Self {
            db_path,
            control_addr: None,
            standalone_db_writer: true,
        }
    }
}

/// Handle for interacting with the Core from other threads.
///
/// Can be cloned and shared. All operations send Commands to Core
/// and wait for responses via one-shot channels.
#[derive(Clone)]
pub struct CoreHandle {
    /// Channel to send commands to Core
    cmd_tx: Sender<Command>,
}

impl CoreHandle {
    /// Create a new handle (internal)
    fn new(cmd_tx: Sender<Command>) -> Self {
        Self { cmd_tx }
    }

    /// Send a command and wait for response
    fn send_and_wait<T>(&self, make_cmd: impl FnOnce(Responder<T>) -> Command) -> Result<T>
    where
        T: Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let cmd = make_cmd(tx);
        self.cmd_tx
            .send(cmd)
            .map_err(|_| anyhow::anyhow!("Core channel closed"))?;
        rx.recv()
            .map_err(|_| anyhow::anyhow!("Core response channel closed"))
    }

    /// Create a new job
    pub fn create_job(&self, spec: JobSpec, approval_id: Option<String>) -> Result<Job> {
        self.send_and_wait(|respond| Command::CreateJob {
            spec,
            approval_id,
            respond,
        })?
    }

    /// Get a job by ID
    pub fn get_job(&self, id: JobId) -> Result<Option<Job>> {
        self.send_and_wait(|respond| Command::GetJob { id, respond })?
    }

    /// Start a job
    pub fn start_job(&self, id: JobId) -> Result<()> {
        self.send_and_wait(|respond| Command::StartJob { id, respond })?
    }

    /// Update job progress
    pub fn update_progress(&self, id: JobId, progress: JobProgress) -> Result<()> {
        self.send_and_wait(|respond| Command::UpdateProgress {
            id,
            progress,
            respond,
        })?
    }

    /// Complete a job successfully
    pub fn complete_job(&self, id: JobId, result: serde_json::Value) -> Result<()> {
        self.send_and_wait(|respond| Command::CompleteJob {
            id,
            result,
            respond,
        })?
    }

    /// Fail a job with an error
    pub fn fail_job(&self, id: JobId, error: String) -> Result<()> {
        self.send_and_wait(|respond| Command::FailJob { id, error, respond })?
    }

    /// Cancel a job
    pub fn cancel_job(&self, id: JobId) -> Result<bool> {
        self.send_and_wait(|respond| Command::CancelJob { id, respond })?
    }

    /// List jobs with optional status filter
    pub fn list_jobs(&self, status_filter: Option<&str>, limit: usize) -> Result<Vec<Job>> {
        self.send_and_wait(|respond| Command::ListJobs {
            status_filter: status_filter.map(|s| s.to_string()),
            limit,
            respond,
        })?
    }

    // ========================================================================
    // Approval Methods
    // ========================================================================

    /// Create a new approval request
    pub fn create_approval(
        &self,
        operation: crate::approvals::ApprovalOperation,
        summary: crate::types::ApprovalSummary,
    ) -> Result<crate::approvals::ApprovalRequest> {
        self.send_and_wait(|respond| Command::CreateApproval {
            operation,
            summary,
            respond,
        })?
    }

    /// Get an approval by ID
    pub fn get_approval(
        &self,
        id: crate::approvals::ApprovalId,
    ) -> Result<Option<crate::approvals::ApprovalRequest>> {
        self.send_and_wait(|respond| Command::GetApproval { id, respond })?
    }

    /// Approve a pending request
    pub fn approve(&self, id: crate::approvals::ApprovalId) -> Result<bool> {
        self.send_and_wait(|respond| Command::ApproveRequest { id, respond })?
    }

    /// Reject a pending request
    pub fn reject(&self, id: crate::approvals::ApprovalId, reason: Option<String>) -> Result<bool> {
        self.send_and_wait(|respond| Command::RejectRequest {
            id,
            reason,
            respond,
        })?
    }

    /// Set job ID on an approval
    pub fn set_approval_job_id(
        &self,
        approval_id: crate::approvals::ApprovalId,
        job_id: String,
    ) -> Result<()> {
        self.send_and_wait(|respond| Command::SetApprovalJobId {
            approval_id,
            job_id,
            respond,
        })?
    }

    /// List approvals with optional status filter
    pub fn list_approvals(
        &self,
        status_filter: Option<&str>,
    ) -> Result<Vec<crate::approvals::ApprovalRequest>> {
        self.send_and_wait(|respond| Command::ListApprovals {
            status_filter: status_filter.map(|s| s.to_string()),
            respond,
        })?
    }

    /// Request shutdown
    pub fn shutdown(&self) -> Result<()> {
        self.cmd_tx
            .send(Command::Shutdown)
            .map_err(|_| anyhow::anyhow!("Core channel closed"))
    }
}

impl std::fmt::Debug for CoreHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreHandle")
            .field("cmd_tx", &"<Sender>")
            .finish()
    }
}

/// Core - Single-owner state management thread
///
/// Owns all mutable state and processes Commands in a synchronous loop.
/// Events are emitted for interested subscribers.
pub struct Core {
    // Owned state (no Arc, no Mutex)
    job_manager: JobManager,
    approval_manager: ApprovalManager,

    // Channels
    commands: Receiver<Command>,
    events: Sender<Event>,

    // Cancellation tokens for active jobs
    cancel_tokens: HashMap<JobId, CancellationToken>,
}

impl Core {
    /// Create a new Core and its handle.
    ///
    /// Returns (Core, CoreHandle, EventReceiver).
    /// The Core should be run in its own thread via `run()`.
    pub fn new(db_path: PathBuf) -> Result<(Self, CoreHandle, Receiver<Event>)> {
        Self::new_with_config(CoreConfig::db_only(db_path))
    }

    /// Create a new Core from config.
    pub fn new_with_config(config: CoreConfig) -> Result<(Self, CoreHandle, Receiver<Event>)> {
        // Create channels
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        // Initialize managers
        let (job_manager, approval_manager) = if config.standalone_db_writer {
            let job_manager = JobManager::new(config.db_path.clone())?;
            let approval_manager = ApprovalManager::new(config.db_path)?;
            (job_manager, approval_manager)
        } else {
            let control_addr = config.control_addr;
            let job_manager = JobManager::new_control(control_addr.clone())?;
            let approval_manager = ApprovalManager::new_control(control_addr)?;
            (job_manager, approval_manager)
        };

        let core = Self {
            job_manager,
            approval_manager,
            commands: cmd_rx,
            events: event_tx,
            cancel_tokens: HashMap::new(),
        };

        let handle = CoreHandle::new(cmd_tx);

        Ok((core, handle, event_rx))
    }

    /// Run the Core loop. Blocks until shutdown.
    pub fn run(&mut self) {
        info!("Core started");

        loop {
            match self.commands.recv() {
                Ok(Command::Shutdown) => {
                    info!("Core received shutdown command");
                    break;
                }
                Ok(cmd) => self.handle_command(cmd),
                Err(_) => {
                    info!("Core command channel closed");
                    break;
                }
            }
        }

        info!("Core stopped");
    }

    /// Handle a single command
    fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::CreateJob {
                spec,
                approval_id,
                respond,
            } => {
                let result = self.job_manager.create_job(spec, approval_id);
                if let Ok(ref job) = result {
                    let _ = self.events.send(Event::JobCreated { job_id: job.id });
                }
                let _ = respond.send(result);
            }

            Command::GetJob { id, respond } => {
                let result = self.job_manager.get_job(&id);
                let _ = respond.send(result);
            }

            Command::StartJob { id, respond } => {
                let result = self.job_manager.start_job(&id);
                if result.is_ok() {
                    let _ = self.events.send(Event::JobStarted { job_id: id });
                }
                let _ = respond.send(result);
            }

            Command::UpdateProgress {
                id,
                progress,
                respond,
            } => {
                let result = self.job_manager.update_progress(&id, progress.clone());
                if result.is_ok() {
                    let _ = self.events.send(Event::JobProgress {
                        job_id: id,
                        progress,
                    });
                }
                let _ = respond.send(result);
            }

            Command::CompleteJob {
                id,
                result,
                respond,
            } => {
                let complete_result = self.job_manager.complete_job(&id, result);
                if complete_result.is_ok() {
                    self.cancel_tokens.remove(&id);
                    let _ = self.events.send(Event::JobCompleted { job_id: id });
                }
                let _ = respond.send(complete_result);
            }

            Command::FailJob { id, error, respond } => {
                let fail_result = self.job_manager.fail_job(&id, &error);
                if fail_result.is_ok() {
                    self.cancel_tokens.remove(&id);
                    let _ = self.events.send(Event::JobFailed { job_id: id, error });
                }
                let _ = respond.send(fail_result);
            }

            Command::CancelJob { id, respond } => {
                // First signal the cancellation token if exists
                if let Some(token) = self.cancel_tokens.get(&id) {
                    token.cancel();
                }
                let result = self.job_manager.cancel_job(&id);
                if let Ok(true) = result {
                    self.cancel_tokens.remove(&id);
                    let _ = self.events.send(Event::JobCancelled { job_id: id });
                }
                let _ = respond.send(result);
            }

            Command::ListJobs {
                status_filter,
                limit,
                respond,
            } => {
                let result = self.job_manager.list_jobs(status_filter.as_deref(), limit);
                let _ = respond.send(result);
            }

            // ====================================================================
            // Approval Commands
            // ====================================================================
            Command::CreateApproval {
                operation,
                summary,
                respond,
            } => {
                let result = self.approval_manager.create_approval(operation, summary);
                let _ = respond.send(result);
            }

            Command::GetApproval { id, respond } => {
                let result = self.approval_manager.get_approval(&id);
                let _ = respond.send(result);
            }

            Command::ApproveRequest { id, respond } => {
                let result = self.approval_manager.approve(&id);
                let _ = respond.send(result);
            }

            Command::RejectRequest {
                id,
                reason,
                respond,
            } => {
                let result = self.approval_manager.reject(&id, reason);
                let _ = respond.send(result);
            }

            Command::SetApprovalJobId {
                approval_id,
                job_id,
                respond,
            } => {
                let result = self.approval_manager.set_job_id(&approval_id, job_id);
                let _ = respond.send(result);
            }

            Command::ListApprovals {
                status_filter,
                respond,
            } => {
                let result = self
                    .approval_manager
                    .list_approvals(status_filter.as_deref());
                let _ = respond.send(result);
            }

            Command::Shutdown => {
                // Handled in main loop
            }
        }
    }

    /// Register a cancellation token for an active job
    pub fn register_cancel_token(&mut self, job_id: JobId) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancel_tokens.insert(job_id, token.clone());
        token
    }
}

/// Spawn Core in a dedicated thread
pub fn spawn_core_with_config(
    config: CoreConfig,
) -> Result<(CoreHandle, Receiver<Event>, JoinHandle<()>)> {
    let (mut core, handle, events) = Core::new_with_config(config)?;

    let thread_handle = thread::Builder::new()
        .name("mcp-core".to_string())
        .spawn(move || {
            core.run();
        })?;

    Ok((handle, events, thread_handle))
}

/// Spawn Core in a dedicated thread (DB-only backend).
pub fn spawn_core(db_path: PathBuf) -> Result<(CoreHandle, Receiver<Event>, JoinHandle<()>)> {
    spawn_core_with_config(CoreConfig::db_only(db_path))
}
