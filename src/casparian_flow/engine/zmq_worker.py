# src/casparian_flow/engine/zmq_worker.py
import logging
import time
from typing import Dict, Optional
from dataclasses import dataclass

import zmq
import pyarrow as pa
import pyarrow.ipc
from sqlalchemy import create_engine  # <--- Added missing import
from pathlib import Path

from casparian_flow.protocol import OpCode, unpack_header, validate_header, msg_execute
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.context import WorkerContext
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import (
    ProcessingJob,
    FileVersion,
    FileLocation,
    TopicConfig,
)

logger = logging.getLogger(__name__)


@dataclass
class PendingJob:
    job_id: int
    plugin_name: str
    file_path: str
    dispatched_at: float
    context: Optional[WorkerContext] = None


class ZmqWorker:
    def __init__(self, config: WorkerConfig, zmq_addr: str = "ipc:///tmp/casparian"):
        self.config = config
        self.zmq_addr = zmq_addr
        self.active = False

        # FIX: Use create_engine directly with connection_string
        self.engine = create_engine(config.database.connection_string)
        self.queue = JobQueue(self.engine)

        self.ctx = zmq.Context()
        self.router = self.ctx.socket(zmq.ROUTER)
        self.router.bind(zmq_addr)

        self.poller = zmq.Poller()
        self.poller.register(self.router, zmq.POLLIN)

        self.plugin_registry: Dict[str, bytes] = {}
        self.pending_jobs: Dict[int, PendingJob] = {}

        # Ensure parquet root is a Path object
        self.parquet_root = config.storage.parquet_root

        logger.info(f"ZmqWorker initialized on {zmq_addr}")

    def run(self, poll_interval_ms: int = 100):
        self.active = True
        logger.info("ZmqWorker loop started")

        try:
            while self.active:
                try:
                    # FIX: Handle ZMQError if socket is closed during poll
                    socks = dict(self.poller.poll(timeout=poll_interval_ms))
                except zmq.ZMQError:
                    if not self.active:
                        break
                    raise

                if self.router in socks:
                    self._handle_zmq_message()

                self._dispatch_jobs()
        finally:
            # Cleanup only after loop exits
            self.router.close()
            self.ctx.term()
            logger.info("ZmqWorker resources released")

    def _handle_zmq_message(self):
        try:
            frames = self.router.recv_multipart(flags=zmq.NOBLOCK)
        except zmq.Again:
            return

        if len(frames) < 2:
            return

        identity, header_frame = frames[0], frames[1]

        if validate_header(header_frame):
            return  # Ignore invalid headers

        op, job_id, _ = unpack_header(header_frame)

        if op == OpCode.REG:
            if len(frames) >= 3:
                plugin_name = frames[2].decode("utf-8")
                self.plugin_registry[plugin_name] = identity
                logger.info(f"Registered: {plugin_name}")

        elif op == OpCode.DATA:
            if len(frames) >= 4 and job_id in self.pending_jobs:
                self._process_data_payload(job_id, frames[3])

        elif op == OpCode.DONE:
            if job_id in self.pending_jobs:
                pending = self.pending_jobs.pop(job_id)
                if pending.context:
                    pending.context.commit()
                self.queue.complete_job(job_id)

        elif op == OpCode.ERR:
            if job_id in self.pending_jobs:
                err_msg = frames[2].decode("utf-8") if len(frames) > 2 else "Error"
                self.pending_jobs.pop(job_id)
                self.queue.fail_job(job_id, err_msg)

    def _process_data_payload(self, job_id: int, payload: bytes):
        pending = self.pending_jobs[job_id]
        if pending.context is None:
            pending.context = self._create_context(job_id)

        try:
            reader = pa.ipc.open_stream(payload)
            table = reader.read_all()
            df = table.to_pandas()
            df["_job_id"] = job_id

            # Simple single-topic support for MVP
            if not pending.context.sinks:
                handle = pending.context.register_topic("output")
            else:
                handle = 0

            pending.context.sinks[handle]["sink"].write(df)
        except Exception as e:
            logger.error(f"Data processing error: {e}")

    def _create_context(self, job_id: int) -> WorkerContext:
        from sqlalchemy.orm import Session

        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            topics = (
                session.query(TopicConfig).filter_by(plugin_name=job.plugin_name).all()
            )
            topic_config = {
                t.topic_name: {"uri": t.uri, "mode": t.mode} for t in topics
            }

            return WorkerContext(
                sql_engine=self.engine,
                parquet_root=self.parquet_root,
                topic_config=topic_config,
                job_id=job_id,
                file_version_id=job.file_version_id,
                file_location_id=None,
            )

    def _dispatch_jobs(self):
        # ... (Same as before, omitted for brevity) ...
        # Ensure imports are available inside method if needed
        from sqlalchemy.orm import Session

        available_plugins = set(self.plugin_registry.keys())
        if not available_plugins:
            return

        job = self.queue.pop_job()
        if not job:
            return

        if job.plugin_name not in available_plugins:
            self.queue.fail_job(job.id, "No sidecar")
            return

        # Resolve Path
        with Session(self.engine) as session:
            ver = session.get(FileVersion, job.file_version_id)
            loc = session.get(FileLocation, ver.location_id)
            src = session.get(loc.source_root.__class__, loc.source_root_id)
            # Fix path joining
            full_path = str(Path(src.path) / loc.rel_path)

        identity = self.plugin_registry[job.plugin_name]
        self.router.send_multipart([identity] + msg_execute(job.id, full_path))

        self.pending_jobs[job.id] = PendingJob(
            job_id=job.id,
            plugin_name=job.plugin_name,
            file_path=full_path,
            dispatched_at=time.time(),
        )

    def stop(self):
        """Signal the loop to exit. Resources are cleaned up in run()."""
        self.active = False
