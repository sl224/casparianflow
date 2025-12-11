# src/casparian_flow/engine/zmq_worker.py
"""
ZMQ-based Worker with Process Isolation.

This worker uses ZMQ ROUTER socket to communicate with Sidecar processes,
providing crash isolation and dependency isolation for plugins.

Architecture:
- ROUTER socket binds and accepts connections from DEALER sidecars
- Poller monitors both ZMQ socket and database queue
- Plugin registry maps plugin names to ZMQ identities
"""
import logging
import time
from typing import Dict, Optional, Set
from dataclasses import dataclass

import zmq
import pyarrow as pa
import pyarrow.ipc

from casparian_flow.protocol import (
    OpCode, HEADER_SIZE,
    pack_header, unpack_header, validate_header,
    msg_execute
)
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.context import WorkerContext
from casparian_flow.engine.sinks import SinkFactory
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import ProcessingJob, FileVersion, FileLocation, TopicConfig

logger = logging.getLogger(__name__)


@dataclass
class PendingJob:
    """Tracks a job dispatched to a sidecar."""
    job_id: int
    plugin_name: str
    file_path: str
    dispatched_at: float
    context: Optional[WorkerContext] = None


class ZmqWorker:
    """
    Worker using ZMQ ROUTER for plugin isolation.
    
    Unlike the original CasparianWorker which loads plugins in-process,
    this worker sends jobs to external Sidecar processes.
    """
    
    def __init__(self, config: WorkerConfig, zmq_addr: str = "ipc:///tmp/casparian"):
        self.config = config
        self.zmq_addr = zmq_addr
        self.active = False
        
        # Database
        from casparian_flow.db.access import get_engine
        self.engine = get_engine(config.database)
        self.queue = JobQueue(self.engine)
        
        # ZMQ
        self.ctx = zmq.Context()
        self.router = self.ctx.socket(zmq.ROUTER)
        self.router.bind(zmq_addr)
        
        self.poller = zmq.Poller()
        self.poller.register(self.router, zmq.POLLIN)
        
        # Plugin registry: plugin_name -> zmq_identity
        self.plugin_registry: Dict[str, bytes] = {}
        
        # Pending jobs: job_id -> PendingJob
        self.pending_jobs: Dict[int, PendingJob] = {}
        
        # Parquet root from config
        self.parquet_root = config.storage.parquet_root if hasattr(config.storage, 'parquet_root') else "./data/parquet"
        
        logger.info(f"ZmqWorker initialized, listening on {zmq_addr}")
    
    def run(self, poll_interval_ms: int = 100):
        """Main worker loop."""
        self.active = True
        
        logger.info("ZmqWorker starting main loop")
        
        while self.active:
            # Poll ZMQ socket
            socks = dict(self.poller.poll(timeout=poll_interval_ms))
            
            if self.router in socks:
                self._handle_zmq_message()
            
            # Check DB queue for new jobs
            self._dispatch_jobs()
    
    def _handle_zmq_message(self):
        """Process incoming ZMQ message from sidecar."""
        frames = self.router.recv_multipart()
        
        if len(frames) < 2:
            logger.warning("Incomplete message received")
            return
        
        # Frame 0 is the identity
        identity = frames[0]
        header_frame = frames[1]
        
        # Validate header
        err = validate_header(header_frame)
        if err:
            logger.warning(f"Invalid header from {identity.hex()}: {err}")
            return
        
        op, job_id, meta_len = unpack_header(header_frame)
        
        if op == OpCode.REG:
            # Plugin registration
            if len(frames) < 3:
                logger.warning("REG missing plugin name")
                return
            plugin_name = frames[2].decode('utf-8')
            self.plugin_registry[plugin_name] = identity
            logger.info(f"Registered plugin: {plugin_name} -> {identity.hex()[:16]}")
            
        elif op == OpCode.DATA:
            # Data payload from plugin
            if job_id not in self.pending_jobs:
                logger.warning(f"DATA for unknown job: {job_id}")
                return
            
            if len(frames) < 4:
                logger.warning(f"DATA missing payload for job {job_id}")
                return
            
            payload = frames[3]
            self._process_data_payload(job_id, payload)
            
        elif op == OpCode.DONE:
            # Job completed
            if job_id not in self.pending_jobs:
                logger.warning(f"DONE for unknown job: {job_id}")
                return
            
            pending = self.pending_jobs.pop(job_id)
            
            # Commit context (promotes staging to production)
            if pending.context:
                pending.context.commit()
            
            # Mark job complete in DB
            self.queue.complete_job(job_id)
            logger.info(f"Job {job_id} completed")
            
        elif op == OpCode.ERR:
            # Job failed
            if job_id not in self.pending_jobs:
                logger.warning(f"ERR for unknown job: {job_id}")
                return
            
            error_msg = frames[2].decode('utf-8') if len(frames) > 2 else "Unknown error"
            
            pending = self.pending_jobs.pop(job_id)
            
            # Rollback context (discards staging)
            if pending.context:
                pending.context.rollback()
            
            # Mark job failed in DB
            self.queue.fail_job(job_id, error_msg[:500])  # Truncate long errors
            logger.error(f"Job {job_id} failed: {error_msg[:200]}")
    
    def _process_data_payload(self, job_id: int, payload: bytes):
        """Process Arrow IPC payload and write to sink."""
        pending = self.pending_jobs[job_id]
        
        # Create context if not exists
        if pending.context is None:
            pending.context = self._create_context(job_id)
        
        # Deserialize Arrow IPC
        try:
            reader = pa.ipc.open_stream(payload)
            table = reader.read_all()
            
            # Convert to pandas for compatibility with existing sinks
            df = table.to_pandas()
            
            # Inject lineage headers
            df['_job_id'] = job_id
            
            # Get or create topic handle
            if not pending.context.sinks:
                handle = pending.context.register_topic("output")
            else:
                handle = 0
            
            # Write to sink (staging)
            pending.context.sinks[handle]['sink'].write(df)
            
        except Exception as e:
            logger.error(f"Failed to process DATA for job {job_id}: {e}")
    
    def _create_context(self, job_id: int) -> WorkerContext:
        """Create a WorkerContext for a job."""
        from sqlalchemy.orm import Session
        
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            if not job:
                raise ValueError(f"Job not found: {job_id}")
            
            # Get topic configs for this plugin
            topics = session.query(TopicConfig).filter_by(plugin_name=job.plugin_name).all()
            topic_config = {t.topic_name: {"uri": t.uri, "mode": t.mode} for t in topics}
        
        return WorkerContext(
            sql_engine=self.engine,
            parquet_root=self.parquet_root,
            topic_config=topic_config,
            job_id=job_id,
            file_version_id=job.file_version_id if hasattr(job, 'file_version_id') else None,
            file_location_id=None
        )
    
    def _dispatch_jobs(self):
        """Check queue and dispatch jobs to available sidecars."""
        # Get available plugin names
        available_plugins = set(self.plugin_registry.keys())
        if not available_plugins:
            return  # No sidecars connected
        
        # Don't dispatch more if we have pending jobs for all plugins
        busy_plugins = {p.plugin_name for p in self.pending_jobs.values()}
        idle_plugins = available_plugins - busy_plugins
        
        if not idle_plugins:
            return  # All connected plugins are busy
        
        # Try to pop a job for an idle plugin
        job = self.queue.pop_job()
        if not job:
            return
        
        if job.plugin_name not in available_plugins:
            # No sidecar for this plugin, put job back (fail it after timeout)
            logger.warning(f"No sidecar for plugin: {job.plugin_name}")
            self.queue.fail_job(job.id, f"No sidecar registered for plugin: {job.plugin_name}")
            return
        
        # Resolve file path
        from sqlalchemy.orm import Session
        with Session(self.engine) as session:
            version = session.get(FileVersion, job.file_version_id)
            if not version:
                self.queue.fail_job(job.id, "FileVersion not found")
                return
            location = session.get(FileLocation, version.location_id)
            if not location:
                self.queue.fail_job(job.id, "FileLocation not found")
                return
            source = session.get(location.source_root.__class__, location.source_root_id)
            file_path = f"{source.path}/{location.rel_path}" if source else location.rel_path
        
        # Get identity for plugin
        identity = self.plugin_registry[job.plugin_name]
        
        # Send EXEC message
        exec_frames = [identity] + msg_execute(job.id, file_path)
        self.router.send_multipart(exec_frames)
        
        # Track pending job
        self.pending_jobs[job.id] = PendingJob(
            job_id=job.id,
            plugin_name=job.plugin_name,
            file_path=file_path,
            dispatched_at=time.time()
        )
        
        logger.info(f"Dispatched job {job.id} to {job.plugin_name}: {file_path}")
    
    def stop(self):
        """Gracefully stop the worker."""
        self.active = False
        self.router.close()
        self.ctx.term()
        logger.info("ZmqWorker stopped")
