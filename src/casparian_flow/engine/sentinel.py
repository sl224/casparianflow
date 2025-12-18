# src/casparian_flow/engine/sentinel.py
import logging
import time
import zmq
import json
import pyarrow as pa
from typing import Dict, Set
from dataclasses import dataclass, field
from sqlalchemy import create_engine
from sqlalchemy.orm import Session
from pathlib import Path

from casparian_flow.protocol import OpCode, unpack_header, msg_exec
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.context import WorkerContext
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import FileVersion, FileLocation, TopicConfig

logger = logging.getLogger(__name__)

@dataclass
class ConnectedWorker:
    identity: bytes
    status: str = "IDLE"  # IDLE, BUSY
    last_seen: float = 0.0
    capabilities: Set[str] = field(default_factory=set)
    current_job_id: int = None

class Sentinel:
    def __init__(self, config: WorkerConfig, bind_addr: str = "tcp://127.0.0.1:5555"):
        self.config = config
        self.engine = create_engine(config.database.connection_string)
        self.queue = JobQueue(self.engine)
        
        self.ctx = zmq.Context()
        self.socket = self.ctx.socket(zmq.ROUTER)
        self.socket.bind(bind_addr)
        
        self.workers: Dict[bytes, ConnectedWorker] = {}
        self.active_contexts: Dict[int, WorkerContext] = {}
        self.running = False
        
        logger.info(f"Sentinel online at {bind_addr}")

    def run(self):
        self.running = True
        poller = zmq.Poller()
        poller.register(self.socket, zmq.POLLIN)
        
        while self.running:
            # 1. Network Poll (Non-blocking)
            socks = dict(poller.poll(timeout=100))
            if self.socket in socks:
                self._handle_message()
            
            # 2. Dispatch
            self._dispatch_loop()

    def _handle_message(self):
        try:
            frames = self.socket.recv_multipart()
            identity, header = frames[0], frames[1]
            payload = frames[2] if len(frames) > 2 else b""
            
            op, job_id, _, _, _ = unpack_header(header)
            
            if op == OpCode.HELLO:
                self._register_worker(identity, payload)
            elif op == OpCode.READY:
                self._worker_ready(identity)
            elif op == OpCode.DATA:
                self._handle_data(job_id, payload)
            elif op == OpCode.ERR:
                self._handle_error(job_id, payload)
                self._worker_ready(identity) # Reset worker state
                
        except Exception as e:
            logger.error(f"Sentinel Error: {e}", exc_info=True)

    def _register_worker(self, identity, payload):
        caps = set(json.loads(payload.decode()))
        self.workers[identity] = ConnectedWorker(
            identity=identity, last_seen=time.time(), capabilities=caps
        )
        logger.info(f"Worker Joined: {len(caps)} capabilities")

    def _worker_ready(self, identity):
        if identity in self.workers:
            w = self.workers[identity]
            # If it had a job, finish it
            if w.current_job_id:
                self._finalize_job(w.current_job_id)
                w.current_job_id = None
            w.status = "IDLE"
            w.last_seen = time.time()

    def _dispatch_loop(self):
        # Simple FIFO dispatch
        idle_workers = [w for w in self.workers.values() if w.status == "IDLE"]
        if not idle_workers: return

        # Peek/Pop Job
        job = self.queue.pop_job()
        if not job: return

        # Match Capability
        candidate = next((w for w in idle_workers if job.plugin_name in w.capabilities), None)
        
        if candidate:
            self._assign_job(candidate, job)
        else:
            logger.warning(f"No worker capable of {job.plugin_name}. Requeuing.")
            self.queue.fail_job(job.id, "No capable worker available")

    def _assign_job(self, worker, job):
        logger.info(f"Assigning Job {job.id} to worker")
        
        # 1. Create Context (Gatekeeper)
        topic_conf = {}
        with Session(self.engine) as s:
            tcs = s.query(TopicConfig).filter_by(plugin_name=job.plugin_name).all()
            for t in tcs:
                topic_conf[t.topic_name] = {"uri": t.uri, "mode": t.mode}

        ctx = WorkerContext(
            sql_engine=self.engine,
            parquet_root=self.config.storage.parquet_root,
            topic_config=topic_conf,
            job_id=job.id,
            file_version_id=job.file_version_id
        )
        # Ensure at least one default topic exists
        ctx.register_topic("output", default_uri=f"parquet://{job.plugin_name}_output")
        self.active_contexts[job.id] = ctx

        # 2. Get Absolute File Path (CRITICAL FIX)
        with Session(self.engine) as s:
            fv = s.get(FileVersion, job.file_version_id)
            fl = s.get(FileLocation, fv.location_id)
            
            # Using the ORM relationship to resolve SourceRoot
            # This ensures we get the absolute path configured in SourceRoot
            source_root = fl.source_root
            if not source_root:
                # Fallback if relationship not loaded (unlikely with Session.get)
                # But to be safe, fetch it explicitly if needed
                from casparian_flow.db.models import SourceRoot
                source_root = s.get(SourceRoot, fl.source_root_id)
                
            full_path = str(Path(source_root.path) / fl.rel_path)

        # 3. Send Command
        worker.status = "BUSY"
        worker.current_job_id = job.id
        self.socket.send_multipart([worker.identity] + msg_exec(job.id, job.plugin_name, full_path))

    def _handle_data(self, job_id, payload):
        if job_id in self.active_contexts:
            try:
                reader = pa.ipc.open_stream(payload)
                table = reader.read_all()
                # Zero-copy write to sink
                self.active_contexts[job_id].publish(0, table)
            except Exception as e:
                logger.error(f"Data Write Error: {e}")

    def _handle_error(self, job_id, payload):
        msg = payload.decode()
        logger.error(f"Job {job_id} Error: {msg}")
        self.queue.fail_job(job_id, msg)
        if job_id in self.active_contexts:
            del self.active_contexts[job_id]

    def _finalize_job(self, job_id):
        if job_id in self.active_contexts:
            try:
                self.active_contexts[job_id].commit()
                self.active_contexts[job_id].close_all()
            except Exception as e:
                logger.error(f"Error finalizing job {job_id}: {e}")
            finally:
                del self.active_contexts[job_id]
            
        self.queue.complete_job(job_id, "Success")
        logger.info(f"Job {job_id} Finished")