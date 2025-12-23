# src/casparian_flow/engine/sentinel.py
import logging
import time
import zmq
import json
from typing import Dict, Set
from dataclasses import dataclass, field
from sqlalchemy import create_engine
from sqlalchemy.orm import Session
from pathlib import Path

from casparian_flow.protocol import (
    OpCode,
    unpack_msg,
    msg_dispatch,
    SinkConfig,
    JobReceipt,
)
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import FileVersion, FileLocation, TopicConfig, ProcessingJob

logger = logging.getLogger(__name__)

@dataclass
class ConnectedWorker:
    identity: bytes
    status: str = "IDLE"
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
        self.socket.setsockopt(zmq.LINGER, 0)
        self.socket.bind(bind_addr)

        self.workers: Dict[bytes, ConnectedWorker] = {}
        self.running = False

        # Cache topic configurations to avoid blocking I/O in event loop
        self.topic_map: Dict[str, list[SinkConfig]] = {}
        self._load_topic_configs()

        logger.info(f"Sentinel online at {bind_addr}")

    def _load_topic_configs(self):
        """
        Load all topic configurations into memory on startup.
        This avoids blocking database queries in the event loop.
        """
        with Session(self.engine) as s:
            all_configs = s.query(TopicConfig).all()

            for tc in all_configs:
                if tc.plugin_name not in self.topic_map:
                    self.topic_map[tc.plugin_name] = []

                self.topic_map[tc.plugin_name].append(
                    SinkConfig(
                        topic=tc.topic_name,
                        uri=tc.uri,
                        mode=tc.mode or "append",
                        schema_def=tc.schema_json,
                    )
                )

        logger.info(
            f"Loaded topic configs for {len(self.topic_map)} plugins "
            f"({sum(len(v) for v in self.topic_map.values())} total sinks)"
        )

    def run(self):
        self.running = True
        poller = zmq.Poller()
        poller.register(self.socket, zmq.POLLIN)
        
        logger.info("Sentinel loop started.")
        try:
            while self.running:
                socks = dict(poller.poll(timeout=100))
                if self.socket in socks:
                    self._handle_message()
                self._dispatch_loop()
        except Exception as e:
            logger.critical(f"Sentinel crashed: {e}", exc_info=True)
        finally:
            logger.info("Sentinel shutting down...")
            try:
                self.socket.close()
                self.ctx.term()
            except Exception as e:
                logger.error(f"Error closing Sentinel ZMQ: {e}")
            logger.info("Sentinel resources released.")

    def stop(self):
        self.running = False

    def _handle_message(self):
        try:
            frames = self.socket.recv_multipart()
            if len(frames) < 2:
                return

            identity = frames[0]
            # Strip identity from frames for unpacking
            message_frames = frames[1:]

            try:
                opcode, job_id, payload_dict = unpack_msg(message_frames)
            except ValueError as e:
                logger.error(f"Failed to unpack message: {e}")
                return

            if opcode == OpCode.IDENTIFY:
                self._register_worker(identity, payload_dict)

            elif opcode == OpCode.CONCLUDE:
                try:
                    receipt = JobReceipt(**payload_dict)
                    self._handle_conclude(identity, job_id, receipt)
                except Exception as e:
                    logger.error(f"Failed to parse CONCLUDE receipt: {e}")

            elif opcode == OpCode.ERR:
                error_msg = payload_dict.get("message", "Unknown error")
                error_trace = payload_dict.get("traceback", None)
                self._handle_error(identity, job_id, error_msg, error_trace)

            elif opcode == OpCode.HEARTBEAT:
                # Update worker last_seen timestamp
                if identity in self.workers:
                    self.workers[identity].last_seen = time.time()

            else:
                logger.warning(f"Unhandled OpCode: {opcode}")

        except Exception as e:
            logger.error(f"Sentinel Error: {e}", exc_info=True)

    def _register_worker(self, identity, payload_dict):
        """Register a worker from IDENTIFY message."""
        try:
            caps = set(payload_dict.get("capabilities", []))
            worker_id = payload_dict.get("worker_id", identity.decode())

            self.workers[identity] = ConnectedWorker(
                identity=identity, last_seen=time.time(), capabilities=caps
            )
            logger.info(
                f"Worker Joined [{worker_id}]: {len(caps)} capabilities - {caps}"
            )
        except Exception as e:
            logger.error(f"Bad IDENTIFY payload: {e}")


    def _dispatch_loop(self):
        idle_workers = [w for w in self.workers.values() if w.status == "IDLE"]
        if not idle_workers: return

        job = self.queue.pop_job()
        if not job: return

        candidate = next((w for w in idle_workers if job.plugin_name in w.capabilities), None)
        
        if candidate:
            self._assign_job(candidate, job)
        else:
            logger.warning(f"No worker capable of {job.plugin_name}. Requeuing.")
            self.queue.fail_job(job.id, "No capable worker available")

    def _assign_job(self, worker, job):
        """
        Assign a job to a worker using the Split Plane DISPATCH protocol.
        Uses cached sink configurations and resolves file paths from the database.
        """
        logger.info(f"Assigning Job {job.id} to worker")

        # 1. Load Topic Configs from Cache (non-blocking)
        sink_configs = self.topic_map.get(job.plugin_name, []).copy()

        # Add default 'output' sink if not configured
        if not any(sc.topic == "output" for sc in sink_configs):
            default_output_uri = (
                f"parquet://{job.plugin_name}_output.parquet"
            )
            sink_configs.append(
                SinkConfig(topic="output", uri=default_output_uri, mode="append")
            )

        # 2. Resolve file path (still requires database access)
        with Session(self.engine) as s:
            fv = s.get(FileVersion, job.file_version_id)
            fl = s.get(FileLocation, fv.location_id)
            source_root = fl.source_root
            if not source_root:
                from casparian_flow.db.models import SourceRoot

                source_root = s.get(SourceRoot, fl.source_root_id)
            full_path = str(Path(source_root.path) / fl.rel_path)

        # 3. Send DISPATCH message with lineage context
        worker.status = "BUSY"
        worker.current_job_id = job.id
        dispatch_msg = msg_dispatch(
            job.id,
            job.plugin_name,
            full_path,
            sink_configs,
            job.file_version_id  # Pass file_version_id for lineage restoration
        )
        self.socket.send_multipart([worker.identity] + dispatch_msg)

        logger.info(
            f"Dispatched Job {job.id} with {len(sink_configs)} sink configs"
        )

    def _handle_conclude(self, identity, job_id, receipt: JobReceipt):
        """
        Handle CONCLUDE message from worker.
        Process the receipt and update the database with job results.
        """
        # Mark worker as idle
        if identity in self.workers:
            worker = self.workers[identity]
            worker.status = "IDLE"
            worker.current_job_id = None
            worker.last_seen = time.time()

        if receipt.status == "SUCCESS":
            logger.info(
                f"Job {job_id} completed successfully. "
                f"Artifacts: {len(receipt.artifacts)}, "
                f"Metrics: {receipt.metrics}"
            )
            self.queue.complete_job(job_id, "Success")

            # TODO: Update FileLocation / FileVersion records based on artifacts
            # For now, we just log the artifacts
            for artifact in receipt.artifacts:
                logger.info(
                    f"  Artifact: {artifact['topic']} -> {artifact['uri']}"
                )

        elif receipt.status == "FAILED":
            error_msg = receipt.error_message or "Unknown error"
            logger.error(f"Job {job_id} failed: {error_msg}")
            self.queue.fail_job(job_id, error_msg)

        else:
            logger.warning(f"Job {job_id} concluded with unknown status: {receipt.status}")

    def _handle_error(self, identity, job_id, error_msg, error_trace):
        """
        Handle ERR message from worker.
        Mark the job as failed and the worker as idle.
        """
        logger.error(f"Job {job_id} Error: {error_msg}")
        if error_trace:
            logger.error(f"Traceback:\n{error_trace}")

        self.queue.fail_job(job_id, error_msg)

        # Mark worker as idle
        if identity in self.workers:
            worker = self.workers[identity]
            worker.status = "IDLE"
            worker.current_job_id = None
            worker.last_seen = time.time()