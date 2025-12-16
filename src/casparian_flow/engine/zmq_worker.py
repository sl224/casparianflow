import logging
import os
import time
import subprocess
from typing import Dict, Optional
from dataclasses import dataclass

import zmq
import pyarrow as pa
import pyarrow.ipc
from sqlalchemy import create_engine  # <--- Added missing import
from pathlib import Path

from casparian_flow.protocol import (
    OpCode,
    unpack_header,
    validate_header,
    msg_execute,
    msg_heartbeat,
    msg_done,
    msg_error,
)
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
    file_version_id: int  # Added for zero-lookup context creation
    dispatched_at: float
    context: Optional[WorkerContext] = None


class ZmqWorker:
    def __init__(
        self,
        config: WorkerConfig,
        zmq_addr: str = "ipc:///tmp/casparian",
        architect_secret_key: str = "default-secret-key-change-me",
    ):
        self.config = config
        self.zmq_addr = zmq_addr
        self.active = False

        # FIX: Use create_engine directly with connection_string
        self.engine = create_engine(config.database.connection_string)
        self.queue = JobQueue(self.engine)

        # Initialize Architect service for plugin deployment
        from casparian_flow.services.architect import ArchitectService

        self.architect = ArchitectService(self.engine, architect_secret_key)

        self.ctx = zmq.Context()
        self.router = self.ctx.socket(zmq.ROUTER)
        self.router.bind(zmq_addr)

        self.poller = zmq.Poller()
        self.poller.register(self.router, zmq.POLLIN)

        self.plugin_registry: Dict[str, bytes] = {}
        self.pending_jobs: Dict[int, PendingJob] = {}
        self.sidecar_heartbeats: Dict[bytes, float] = {}  # identity -> last_seen timestamp

        # Routing Table: Cache of plugin_name -> {topic: {uri, mode}}
        # Initialized at startup and reload to avoid DB hits in hot path
        self.routing_table: Dict[str, Dict[str, Dict[str, str]]] = {}
        self._hydrate_routing_table()

        # Ensure parquet root is a Path object
        self.parquet_root = config.storage.parquet_root

        logger.info(f"ZmqWorker initialized on {zmq_addr}")
        
        # Start System Plugins (Built-in)
        self._start_system_plugins()

    def _start_system_plugins(self):
        """
        Launch built-in system sidecars (independent of DB).
        """
        # Locate built-in system deployer
        # It now lives in src/casparian_flow/builtins/system_deployer.py
        import casparian_flow.builtins.system_deployer as sys_deployer_module

        deployer_path = Path(sys_deployer_module.__file__)
        logger.info(f"Starting System Plugin: {deployer_path}")

        import os
        import sys
        env = os.environ.copy()
        # Add src/ to PYTHONPATH to ensure casparian_flow is importable
        # if run from project root, src is needed.
        src_path = str(Path(os.getcwd()) / "src")
        env["PYTHONPATH"] = src_path + os.pathsep + env.get("PYTHONPATH", "")

        try:
            proc = subprocess.Popen(
                [
                    sys.executable,
                    "-m",
                    "casparian_flow.sidecar",
                    "--plugin",
                    str(deployer_path),
                    "--connect",
                    self.zmq_addr,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=env,
            )
            
            # Start Log Pump Threads
            import threading
            def log_pump(stream, prefix, level):
                try:
                    for line in iter(stream.readline, b''):
                        logger.log(level, f"[{prefix}] {line.decode().strip()}")
                except Exception:
                    pass
                finally:
                    stream.close()

            threading.Thread(target=log_pump, args=(proc.stdout, "SystemDeployer", logging.INFO), daemon=True).start()
            threading.Thread(target=log_pump, args=(proc.stderr, "SystemDeployer ERR", logging.ERROR), daemon=True).start()

            logger.info(f"SystemDeployer started (PID: {proc.pid})")
        except Exception as e:
            logger.error(f"Failed to start SystemDeployer: {e}")

    def run(self, poll_interval_ms: int = 100, prune_interval_seconds: int = 30):
        self.active = True
        logger.info("ZmqWorker loop started")
        last_prune = time.time()

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

                # Periodically prune dead sidecars
                now = time.time()
                if now - last_prune > prune_interval_seconds:
                    self._prune_dead_sidecars()
                    last_prune = now
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

        # Protocol v2: unpack_header returns 5 values
        op, job_id, meta_len, content_type, compressed = unpack_header(header_frame)

        # Update heartbeat tracking for all sidecar messages
        self.sidecar_heartbeats[identity] = time.time()

        if op == OpCode.REG:
            if len(frames) >= 3:
                plugin_name = frames[2].decode("utf-8")
                self.plugin_registry[plugin_name] = identity
                logger.info(f"Registered: {plugin_name}")

        elif op == OpCode.HEARTBEAT:
            # Respond to heartbeat immediately
            self.router.send_multipart([identity] + msg_heartbeat())
            logger.debug(f"Heartbeat from sidecar")

        elif op == OpCode.DEPLOY:
            # Handle plugin deployment workflow
            if len(frames) >= 3:
                from casparian_flow.services.architect import handle_deploy_message

                payload = frames[2]
                result = handle_deploy_message(self.architect, payload)

                if result.success:
                    # Hot reload plugins to activate the new plugin
                    self.reload_plugins()
                    self.router.send_multipart([identity] + msg_done(0))
                    logger.info(f"Successfully deployed: {result.plugin_name}")
                else:
                    self.router.send_multipart(
                        [identity] + msg_error(0, result.error_message or "Deployment failed")
                    )
                    logger.error(f"Deployment failed: {result.error_message}")

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
            # ZERO-COPY: Read as Arrow Table, do NOT convert to Pandas here.
            # We pass the Arrow Table directly to the Context/Sinks.
            reader = pa.ipc.open_stream(payload)
            table = reader.read_all()
            
            # Simple single-topic support for MVP
            if not pending.context.sinks:
                handle = pending.context.register_topic("output")
            else:
                handle = 0

            pending.context.publish(handle, table)
        except Exception as e:
            logger.error(f"Data processing error: {e}")


    def _create_context(self, job_id: int) -> WorkerContext:
        # DB-FREE HOT PATH: Uses cached routing table and PendingJob data
        
        pending = self.pending_jobs[job_id]
        
        # Get cached config
        topic_config = self.routing_table.get(pending.plugin_name, {})
        
        return WorkerContext(
            sql_engine=self.engine,
            parquet_root=self.parquet_root,
            topic_config=topic_config,
            job_id=job_id,
            file_version_id=pending.file_version_id,
            file_location_id=None,
        )

    def _hydrate_routing_table(self):
        """Load all TopicConfigs into memory."""
        from sqlalchemy.orm import Session
        
        logger.info("Hydrating Routing Table...")
        with Session(self.engine) as session:
            all_topics = session.query(TopicConfig).all()
            
            new_table = {}
            for t in all_topics:
                if t.plugin_name not in new_table:
                    new_table[t.plugin_name] = {}
                
                new_table[t.plugin_name][t.topic_name] = {
                    "uri": t.uri,
                    "mode": t.mode
                }
            
            self.routing_table = new_table
        logger.info(f"Routing Table Hydrated. Cached {len(self.routing_table)} plugins.")

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
            file_version_id=job.file_version_id,
            dispatched_at=time.time(),
        )

    def _prune_dead_sidecars(self, timeout_seconds: int = 60):
        """
        Remove sidecars that haven't sent a heartbeat in timeout_seconds.

        Args:
            timeout_seconds: Maximum time without heartbeat before pruning
        """
        now = time.time()
        dead_identities = []

        for identity, last_seen in self.sidecar_heartbeats.items():
            if now - last_seen > timeout_seconds:
                dead_identities.append(identity)

        for identity in dead_identities:
            # Find and remove plugin from registry
            plugin_to_remove = None
            for plugin_name, plugin_identity in self.plugin_registry.items():
                if plugin_identity == identity:
                    plugin_to_remove = plugin_name
                    break

            if plugin_to_remove:
                del self.plugin_registry[plugin_to_remove]
                logger.warning(f"Pruned dead sidecar: {plugin_to_remove}")

            del self.sidecar_heartbeats[identity]

    def reload_plugins(self):
        """
        Hot reload plugins from database.

        This method:
        1. Reads active plugins from PluginManifest table
        2. Writes source code to plugins/ directory
        3. Spawns new Sidecar processes dynamically

        Called after successful DEPLOY operations.
        """
        import subprocess
        from sqlalchemy.orm import Session
        from casparian_flow.db.models import PluginManifest, PluginStatusEnum
        
        # 0. Reload Routing Table (New configs might have appeared)
        self._hydrate_routing_table()

        plugins_dir = self.config.plugins.dir
        plugins_dir.mkdir(exist_ok=True, parents=True)

        with Session(self.engine) as session:
            active_plugins = (
                session.query(PluginManifest)
                .filter_by(status=PluginStatusEnum.ACTIVE)
                .all()
            )

            for plugin in active_plugins:
                # Write plugin source to disk
                plugin_path = plugins_dir / f"{plugin.plugin_name}.py"
                plugin_path.write_text(plugin.source_code, encoding="utf-8")
                logger.info(f"Wrote plugin to disk: {plugin_path}")

                import sys
                env = os.environ.copy()
                src_path = str(Path(os.getcwd()) / "src")
                env["PYTHONPATH"] = src_path + os.pathsep + env.get("PYTHONPATH", "")

                # Spawn sidecar process
                try:
                    proc = subprocess.Popen(
                        [
                            sys.executable,
                            "-m",
                            "casparian_flow.sidecar",
                            "--plugin",
                            str(plugin_path),
                            "--connect",
                            self.zmq_addr,
                        ],
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        env=env,
                    )
                    logger.info(
                        f"Spawned sidecar for {plugin.plugin_name} (PID: {proc.pid})"
                    )
                except Exception as e:
                    logger.error(f"Failed to spawn sidecar for {plugin.plugin_name}: {e}")

    def stop(self):
        """Signal the loop to exit. Resources are cleaned up in run()."""
        self.active = False
