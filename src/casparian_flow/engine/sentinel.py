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
    msg_conclude,
    msg_err,
    SinkConfig,
    JobReceipt,
    DeployCommand,
    pack_header,
)
from urllib.request import url2pathname
from casparian_flow.engine.queue import JobQueue
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import FileVersion, FileLocation, TopicConfig, ProcessingJob, SourceRoot
from casparian_flow.services.architect import ArchitectService
from casparian_flow.security.identity import User
from urllib.parse import urlparse

logger = logging.getLogger(__name__)

@dataclass
class ConnectedWorker:
    identity: bytes
    status: str = "IDLE"
    last_seen: float = 0.0
    capabilities: Set[str] = field(default_factory=set)
    current_job_id: int = None

class Sentinel:
    def __init__(self, config: WorkerConfig, bind_addr: str = "tcp://127.0.0.1:5555", secret_key: str = "default-secret"):
        self.config = config
        self.engine = create_engine(config.database.connection_string)
        self.queue = JobQueue(self.engine)

        # v5.0: Architect Service for deployment lifecycle
        self.architect = ArchitectService(self.engine, secret_key)

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

            elif opcode == OpCode.RELOAD:
                # Config Reload Signal (from API or Admin)
                logger.info("Received RELOAD signal. Refreshing configuration...")
                self._load_topic_configs()

                # Broadcast RELOAD to all workers
                # They should re-scan their plugin directories / DB
                reload_msg = [pack_header(OpCode.RELOAD, 0, 0)]
                for worker_id, worker in self.workers.items():
                    self.socket.send_multipart([worker_id] + reload_msg)
                logger.info("Broadcasted RELOAD to all workers.")

            elif opcode == OpCode.DEPLOY:
                # v5.0 Bridge Mode: Artifact Deployment
                self._handle_deploy(identity, payload_dict)

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

    def _handle_deploy(self, identity, payload_dict):
        """
        Handle DEPLOY OpCode - Artifact deployment lifecycle.

        v5.0 Bridge Mode: Treats plugin code as data, extracts MANIFEST via AST,
        and projects state to database (RoutingRule, PluginSubscription, TopicConfig).

        Args:
            identity: ZMQ identity of sender (CLI client)
            payload_dict: Parsed DeployCommand payload
        """
        try:
            # Parse DeployCommand
            cmd = DeployCommand(**payload_dict)

            # Create User object from payload
            publisher = User(
                id=0,  # Will be assigned by database
                name=cmd.publisher_name,
                email=cmd.publisher_email,
                azure_oid=cmd.azure_oid,
            )

            logger.info(
                f"[Sentinel] DEPLOY received: {cmd.plugin_name} v{cmd.version} "
                f"from {publisher.name}"
            )

            # Delegate to Architect Service
            result = self.architect.deploy_artifact(cmd, publisher)

            if result.success:
                logger.info(
                    f"[Sentinel] ✓ Deployment successful: {result.plugin_name} "
                    f"(manifest_id={result.manifest_id})"
                )
                # Reply with CONCLUDE (success)
                receipt = JobReceipt(
                    status="SUCCESS",
                    metrics={"manifest_id": result.manifest_id or 0},
                    artifacts=[],
                )
                reply = msg_conclude(0, receipt)
                self.socket.send_multipart([identity] + reply)

                # Reload topic configs to pick up new routing rules
                self._load_topic_configs()

            else:
                logger.error(
                    f"[Sentinel] ✗ Deployment failed: {result.plugin_name} - "
                    f"{result.error_message}"
                )
                # Reply with ERR
                reply = msg_err(0, result.error_message or "Deployment failed")
                self.socket.send_multipart([identity] + reply)

        except Exception as e:
            logger.error(f"[Sentinel] DEPLOY handler exception: {e}", exc_info=True)
            # Reply with ERR
            reply = msg_err(0, f"DEPLOY exception: {e}")
            self.socket.send_multipart([identity] + reply)


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

        # 2. Load Topic Configs from Cache (non-blocking)
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

        # 3. Apply Overrides (Key-Based Replacement)
        if job.config_overrides:
            try:
                overrides = json.loads(job.config_overrides)
                sink_configs = self._merge_configs(sink_configs, overrides)
                logger.info(f"Applied config overrides for Job {job.id}")
            except Exception as e:
                logger.error(f"Failed to apply overrides for Job {job.id}: {e}")

        # 4. Send DISPATCH message with lineage context
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

    def _merge_configs(self, defaults: list[SinkConfig], overrides: Dict) -> list[SinkConfig]:
        """
        Robust Merge: Overrides strictly replace defaults by Topic Key.
        """
        if not overrides:
            return defaults

        # 1. Group defaults by topic
        # Note: If defaults has multiple sinks for same topic, this naive dict will overwrite.
        # But SinkConfig list is the source.
        # Better: keep map of topic -> list[SinkConfig]
        final_map = {}
        for sc in defaults:
            if sc.topic not in final_map:
                final_map[sc.topic] = []
            final_map[sc.topic].append(sc)

        # 2. Apply Overrides
        for topic, sink_def in overrides.items():
            # Override implies complete replacement for that topic
            # sink_def could be a single dict or list of dicts
            sinks_to_create = sink_def if isinstance(sink_def, list) else [sink_def]

            new_configs = []
            for s in sinks_to_create:
                # Ensure topic is set in the dict
                s_copy = s.copy()
                s_copy["topic"] = topic
                # Validate via Pydantic model before using
                new_configs.append(SinkConfig(**s_copy))

            final_map[topic] = new_configs

        # 3. Flatten
        return [s for sublist in final_map.values() for s in sublist]

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

            # Persist Artifacts to Metadata Store
            try:
                with Session(self.engine) as s:
                    for artifact in receipt.artifacts:
                        uri = artifact['uri']
                        logger.info(f"  Artifact: {artifact['topic']} -> {uri}")
                        
                        try:
                            parsed = urlparse(uri)
                            if parsed.scheme in ["parquet", "file"]:
                                # It's a file. Let's track it.
                                # URI format: parquet:///abs/path/to/file.parquet
                                # Use url2pathname to handle Windows drive letters from /C:/path
                                full_path = Path(url2pathname(parsed.path))
                                
                                # 1. Find SourceRoot (assume Output root is a SourceRoot, or create ad-hoc?)
                                # For strictness, Sentinel usually only tracks files within known SourceRoots.
                                # But output dirs might be separate. 
                                # Strategy: Find best matching SourceRoot parent.
                                roots = s.query(SourceRoot).filter(SourceRoot.active == 1).all()
                                best_root = None
                                for root in roots:
                                    if str(full_path).startswith(root.path):
                                        best_root = root
                                        break
                                
                                if best_root:
                                    rel_path = str(full_path.relative_to(best_root.path))
                                    filename = full_path.name
                                    
                                    # 2. Upsert FileLocation
                                    loc = s.query(FileLocation).filter_by(
                                        source_root_id=best_root.id, rel_path=rel_path
                                    ).first()
                                    
                                    if not loc:
                                        loc = FileLocation(
                                            source_root_id=best_root.id,
                                            rel_path=rel_path,
                                            filename=filename,
                                            last_known_mtime=time.time(), # Approximate
                                            last_known_size=0 # Unknown unless we check fs
                                        )
                                        s.add(loc)
                                        s.flush() # Get ID
                                    
                                    # 3. Create FileVersion linked to this Job
                                    # Use a dummy hash or "generated" marker since we didn't hash it yet
                                    # Note: FileHashRegistry fk constraint requires a valid hash. 
                                    # TODO: Worker should calculate hash? For now, we might skip Version creation 
                                    # if we don't have a hash, OR we just track Location.
                                    # Given the constraints, let's just ensure Location is tracked so Scout picks it up later.
                                    # Scout will see mtime change and create the Version properly.
                                    
                                    # UPDATE: Actually, forcing a Scout scan on this file would be better?
                                    # Or just touching the Location so the user sees it exists.
                                    loc.last_seen_time = func.now()
                                else:
                                    # Ad-Hoc artifact: Outside known SourceRoots
                                    # Log for visibility but don't fail job
                                    logger.warning(
                                        f"Artifact {uri} is outside known SourceRoots - "
                                        f"not persisted to FileLocation. Consider registering "
                                        f"output directory as a SourceRoot for tracking."
                                    )

                            elif parsed.scheme in ["mssql", "sqlite"]:
                                # Database artifact. 
                                # Maybe log as a special 'Virtual File' later?
                                pass

                        except Exception as e:
                            logger.error(f"Failed to persist artifact {uri}: {e}")
                    
                    s.commit()
            except Exception as e:
                logger.error(f"Failed to commit artifact persistence: {e}")

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