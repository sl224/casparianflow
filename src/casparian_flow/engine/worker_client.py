# src/casparian_flow/engine/worker_client.py
"""
Generalist Worker for Casparian Flow.

v5.0 Bridge Mode: Supports both Legacy (Host Process) and Bridge Mode execution.
- Legacy Mode: Plugins run in-process via importlib
- Bridge Mode: Plugins run in isolated venv via subprocess + Arrow IPC
"""
import sys
import zmq
import logging
import json
import time
import argparse
import importlib.util
from pathlib import Path
from typing import Dict, Any, Optional
import pyarrow as pa
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

# Project Imports
from casparian_flow.protocol import (
    OpCode,
    unpack_msg,
    msg_identify,
    msg_conclude,
    msg_err,
    msg_env_ready,
    JobReceipt,
    DispatchCommand,
    SinkConfig,
    PrepareEnvCommand,
    BridgeDispatchCommand,
)
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.services.registrar import register_plugins_from_source
from casparian_flow.sdk import FileEvent
from casparian_flow.engine.sinks import SinkFactory, DataSink
from casparian_flow.engine.venv_manager import get_venv_manager, VenvManagerError
from casparian_flow.engine.bridge import BridgeExecutor, BridgeError

logging.basicConfig(level=logging.INFO, format="%(asctime)s [WORKER] %(message)s")
logger = logging.getLogger(__name__)

class ProxyContext:
    """
    Adapts the BasePlugin 'publish' API to the Split Plane Worker's local sink model.
    Data is written directly to sinks instead of being sent over ZMQ.
    """

    def __init__(self, worker: "GeneralistWorker"):
        self.worker = worker
        self.topic_map: Dict[int, str] = {}
        self.sinks: Dict[str, list[DataSink]] = {}  # topic -> list of sinks
        self._next_handle = 1

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        """Register a topic and return a handle for publishing."""
        handle = self._next_handle
        self.topic_map[handle] = topic
        self._next_handle += 1
        return handle

    def add_sink(self, topic: str, sink: DataSink):
        """Add a sink for a specific topic. Multiple sinks per topic = fan-out."""
        if topic not in self.sinks:
            self.sinks[topic] = []
        self.sinks[topic].append(sink)

    def publish(self, handle: int, data: Any):
        """Publish data to all sinks registered for this topic."""
        if self.worker.current_job_id is None:
            raise RuntimeError("Attempted to publish data without an active job context.")

        # Retrieve topic name
        topic = self.topic_map.get(handle, "output")

        # Write to all sinks for this topic
        sinks = self.sinks.get(topic, [])
        if not sinks:
            logger.warning(
                f"No sinks configured for topic '{topic}'. Data will be dropped."
            )
            return

        for sink in sinks:
            try:
                sink.write(data)
            except Exception as e:
                logger.error(f"Failed to write to sink for topic '{topic}': {e}")
                raise

    def close_all(self):
        """Close all sinks."""
        for topic, sink_list in self.sinks.items():
            for sink in sink_list:
                try:
                    sink.close()
                except Exception as e:
                    logger.error(f"Error closing sink for topic '{topic}': {e}")

    def promote_all(self):
        """Promote all staging sinks to production."""
        for topic, sink_list in self.sinks.items():
            for sink in sink_list:
                try:
                    sink.promote()
                except Exception as e:
                    logger.error(f"Error promoting sink for topic '{topic}': {e}")
                    raise

class GeneralistWorker:
    def __init__(
        self,
        sentinel_addr: str,
        plugin_dir: Path,
        db_engine: Any,
        parquet_root: Optional[Path] = None,
    ):
        self.sentinel_addr = sentinel_addr
        self.plugin_dir = plugin_dir
        self.db_engine = db_engine
        self.parquet_root = parquet_root or Path("output")  # Default to ./output
        self.plugins = {}

        self.ctx = zmq.Context()
        self.socket = self.ctx.socket(zmq.DEALER)
        # CRITICAL: Prevent hang on exit
        self.socket.setsockopt(zmq.LINGER, 0)

        self.identity = f"w-{time.time_ns()}".encode()
        self.socket.setsockopt(zmq.IDENTITY, self.identity)

        self.running = False
        self.current_job_id: Optional[int] = None
        self.proxy_context = ProxyContext(self)

    def start(self):
        """Main Loop."""
        self.running = True
        
        logger.info(f"Scanning plugins in {self.plugin_dir}...")
        self._load_plugins()
        
        if not self.plugins:
            logger.warning("No valid plugins found. Exiting.")
            return

        with Session(self.db_engine) as session:
            register_plugins_from_source(self.plugin_dir, session)
            logger.info("Auto-registered plugin configurations in Database.")

        logger.info(f"Dialing Sentinel at {self.sentinel_addr}...")
        self.socket.connect(self.sentinel_addr)

        caps = list(self.plugins.keys())
        worker_id = self.identity.decode()
        self.socket.send_multipart(msg_identify(caps, worker_id))

        poller = zmq.Poller()
        poller.register(self.socket, zmq.POLLIN)

        logger.info("Entering Event Loop...")
        
        try:
            while self.running:
                try:
                    # 1. Poll (timeout allows checking self.running)
                    socks = dict(poller.poll(timeout=100))
                    
                    # 2. Handle Message
                    if self.socket in socks:
                        self._handle_message()
                except zmq.ZMQError as e:
                    if not self.running: break # Expected during shutdown
                    logger.error(f"ZMQ Error: {e}")
                    break
                
        except Exception as e:
            logger.critical(f"Worker crashed: {e}", exc_info=True)
        finally:
            # 3. Clean Shutdown IN THREAD
            logger.info("Worker shutting down...")
            try:
                self.socket.close()
                self.ctx.term()
            except Exception as e:
                logger.error(f"Error closing Worker ZMQ: {e}")
            logger.info("Worker stopped.")

    def stop(self):
        """Safe to call from main thread."""
        self.running = False

    def _handle_message(self):
        try:
            frames = self.socket.recv_multipart(flags=zmq.NOBLOCK)
        except zmq.Again:
            return

        if not frames:
            return

        try:
            opcode, job_id, payload_dict = unpack_msg(frames)
        except ValueError as e:
            logger.error(f"Failed to unpack message: {e}")
            return

        if opcode == OpCode.DISPATCH:
            # Parse the DispatchCommand (try BridgeDispatchCommand first for v5.0 fields)
            try:
                cmd = BridgeDispatchCommand(**payload_dict)
            except Exception:
                # Fall back to legacy DispatchCommand
                try:
                    cmd = DispatchCommand(**payload_dict)
                except Exception as e:
                    logger.error(f"Failed to parse DISPATCH command: {e}")
                    self.socket.send_multipart(
                        msg_err(job_id, f"Invalid DISPATCH payload: {e}")
                    )
                    return

            logger.info(f"Received Job {job_id} -> {cmd.plugin_name}")

            # Check if this is a Bridge Mode job
            env_hash = getattr(cmd, "env_hash", None)
            source_code = getattr(cmd, "source_code", None)

            if env_hash and source_code:
                # v5.0 Bridge Mode execution
                self._execute_bridge_job(
                    job_id=job_id,
                    plugin_name=cmd.plugin_name,
                    file_path=cmd.file_path,
                    sink_configs=cmd.sinks,
                    file_version_id=cmd.file_version_id,
                    env_hash=env_hash,
                    source_code=source_code,
                )
            else:
                # Legacy Host Process execution
                self._execute_job(
                    job_id, cmd.plugin_name, cmd.file_path, cmd.sinks, cmd.file_version_id
                )

        elif opcode == OpCode.ABORT:
            logger.warning(f"Received ABORT for job {job_id}")
            # TODO: Implement job cancellation logic if needed

        elif opcode == OpCode.PREPARE_ENV:
            # v5.0 Bridge Mode: Environment provisioning
            try:
                cmd = PrepareEnvCommand(**payload_dict)
                self._handle_prepare_env(cmd)
            except Exception as e:
                logger.error(f"Failed to handle PREPARE_ENV: {e}")
                self.socket.send_multipart(
                    msg_err(0, f"PREPARE_ENV failed: {e}")
                )

        elif opcode == OpCode.ERR:
            error_msg = payload_dict.get("message", "Unknown error")
            logger.error(f"Received ERR from Sentinel: {error_msg}")

        else:
            logger.warning(f"Unhandled OpCode: {opcode}")

    def _execute_job(
        self,
        job_id: int,
        plugin_name: str,
        file_path: str,
        sink_configs: list[SinkConfig],
        file_version_id: int,
    ):
        """
        Execute a job with Split Plane architecture:
        - Data is written directly to local sinks
        - Lineage columns (_job_id, _file_version_id) are injected
        - Receipt is sent to Sentinel on completion
        """
        self.current_job_id = job_id
        self.proxy_context.topic_map.clear()
        self.proxy_context.sinks.clear()

        receipt = None
        error_traceback = None

        try:
            # 1. Validate plugin
            handler = self.plugins.get(plugin_name)
            if not handler:
                raise ValueError(f"Plugin {plugin_name} not loaded.")

            # 2. Instantiate sinks from config
            for sink_config in sink_configs:
                try:
                    sink = SinkFactory.create(
                        uri=sink_config.uri,
                        sql_engine=self.db_engine,
                        parquet_root=self.parquet_root,
                        job_id=job_id,
                        file_version_id=file_version_id,
                    )
                    self.proxy_context.add_sink(sink_config.topic, sink)
                    logger.info(
                        f"Created sink for topic '{sink_config.topic}': {sink_config.uri}"
                    )
                except Exception as e:
                    raise ValueError(
                        f"Failed to create sink for {sink_config.uri}: {e}"
                    )

            # 3. Create File Event
            event = FileEvent(path=file_path, file_id=0)

            # 4. Execute plugin
            if hasattr(handler, "consume") and callable(handler.consume):
                try:
                    result = handler.consume(event)
                except NotImplementedError:
                    result = handler.execute(file_path)
            else:
                result = handler.execute(file_path)

            # 5. Handle Return/Yield Results (Implicit Publishing to 'output' topic)
            if result:
                # Ensure 'output' topic has sinks
                if "output" not in self.proxy_context.sinks:
                    logger.warning(
                        "Plugin yielded data but no sink configured for 'output' topic"
                    )
                else:
                    for batch in result:
                        if batch is not None:
                            for sink in self.proxy_context.sinks["output"]:
                                sink.write(batch)

            # 6. Promote all sinks (atomic commit)
            self.proxy_context.promote_all()

            # 7. Generate success receipt
            artifacts = []
            metrics = {"rows": 0, "size_bytes": 0}  # TODO: Track actual metrics

            # Iterate over actual active sinks to get truth
            for topic, sink_list in self.proxy_context.sinks.items():
                for sink in sink_list:
                    try:
                        final_uri = sink.get_final_uri()
                        artifacts.append({"topic": topic, "uri": final_uri})
                    except Exception as e:
                        logger.warning(f"Failed to get final URI for sink on topic {topic}: {e}")
                        # Fallback to configured URI if possible, or skip
                        pass

            receipt = JobReceipt(
                status="SUCCESS", metrics=metrics, artifacts=artifacts
            )
            logger.info(f"Job {job_id} completed successfully")

        except Exception as e:
            # 8. Handle failure
            import traceback

            error_traceback = traceback.format_exc()
            logger.error(f"Job {job_id} Failed: {e}", exc_info=True)

            receipt = JobReceipt(
                status="FAILED",
                metrics={},
                artifacts=[],
                error_message=str(e),
            )

        finally:
            # 9. Close all sinks
            self.proxy_context.close_all()
            self.current_job_id = None

            # 10. Send CONCLUDE message with receipt
            if receipt:
                try:
                    self.socket.send_multipart(msg_conclude(job_id, receipt))
                except Exception as e:
                    logger.error(f"Failed to send CONCLUDE message: {e}")
                    # Try to send ERR as fallback
                    try:
                        self.socket.send_multipart(
                            msg_err(job_id, f"Receipt send failed: {e}", error_traceback)
                        )
                    except Exception:
                        pass

    # =========================================================================
    # v5.0 Bridge Mode Methods
    # =========================================================================

    def _handle_prepare_env(self, cmd: PrepareEnvCommand):
        """
        Handle PREPARE_ENV command - eagerly provision a venv.

        This enables zero-latency execution by pre-provisioning the environment
        before the actual job is dispatched.
        """
        logger.info(f"Preparing environment: {cmd.env_hash[:12]}...")

        try:
            venv_manager = get_venv_manager()
            interpreter_path = venv_manager.get_or_create_env(
                env_hash=cmd.env_hash,
                lockfile_content=cmd.lockfile_content,
                python_version=cmd.python_version,
            )

            # Send ENV_READY response
            cached = venv_manager.exists(cmd.env_hash)
            self.socket.send_multipart(
                msg_env_ready(
                    env_hash=cmd.env_hash,
                    interpreter_path=str(interpreter_path),
                    cached=cached,
                )
            )
            logger.info(f"Environment ready: {cmd.env_hash[:12]}")

        except VenvManagerError as e:
            logger.error(f"Failed to prepare environment: {e}")
            self.socket.send_multipart(
                msg_err(0, f"PREPARE_ENV failed: {e}")
            )

    def _execute_bridge_job(
        self,
        job_id: int,
        plugin_name: str,
        file_path: str,
        sink_configs: list[SinkConfig],
        file_version_id: int,
        env_hash: str,
        source_code: str,
    ):
        """
        Execute a job in Bridge Mode (isolated venv via subprocess).

        This is the v5.0 execution path:
        1. Get interpreter path from VenvManager
        2. Create sinks for output
        3. Spawn BridgeExecutor subprocess
        4. Stream Arrow IPC batches from Guest to Sinks
        5. Send CONCLUDE with receipt
        """
        self.current_job_id = job_id
        receipt = None
        sinks_list = []

        try:
            # 1. Get interpreter path
            venv_manager = get_venv_manager()
            interpreter_path = venv_manager.get_interpreter_path(env_hash)

            if not interpreter_path.exists():
                raise ValueError(
                    f"Environment {env_hash[:12]} not provisioned. "
                    "Send PREPARE_ENV first."
                )

            # 2. Create sinks
            for sink_config in sink_configs:
                sink = SinkFactory.create(
                    uri=sink_config.uri,
                    sql_engine=self.db_engine,
                    parquet_root=self.parquet_root,
                    job_id=job_id,
                    file_version_id=file_version_id,
                )
                sinks_list.append((sink_config.topic, sink))

            # 3. Create Bridge Executor
            executor = BridgeExecutor(
                interpreter_path=interpreter_path,
                source_code=source_code,
                file_path=file_path,
                job_id=job_id,
            )

            # 4. Execute and stream to sinks
            for table in executor.execute():
                df = table.to_pandas()
                for topic, sink in sinks_list:
                    sink.write(df)

            # 5. Promote all sinks
            for topic, sink in sinks_list:
                sink.promote()

            # 6. Build success receipt
            metrics = executor.get_metrics()
            artifacts = []
            for topic, sink in sinks_list:
                try:
                    artifacts.append({"topic": topic, "uri": sink.get_final_uri()})
                except Exception:
                    pass

            receipt = JobReceipt(
                status="SUCCESS",
                metrics={"rows": metrics.get("total_rows", 0)},
                artifacts=artifacts,
            )
            logger.info(f"Bridge Job {job_id} completed: {metrics}")

        except (BridgeError, VenvManagerError, ValueError) as e:
            logger.error(f"Bridge Job {job_id} failed: {e}")
            receipt = JobReceipt(
                status="FAILED",
                metrics={},
                artifacts=[],
                error_message=str(e),
            )

        except Exception as e:
            import traceback
            logger.error(f"Bridge Job {job_id} failed: {e}", exc_info=True)
            receipt = JobReceipt(
                status="FAILED",
                metrics={},
                artifacts=[],
                error_message=str(e),
            )

        finally:
            # Close all sinks
            for topic, sink in sinks_list:
                try:
                    sink.close()
                except Exception:
                    pass

            self.current_job_id = None

            # Send CONCLUDE
            if receipt:
                try:
                    self.socket.send_multipart(msg_conclude(job_id, receipt))
                except Exception as e:
                    logger.error(f"Failed to send CONCLUDE: {e}")

    def _load_plugins(self):
        if not self.plugin_dir.exists(): return
        sys.path.insert(0, str(self.plugin_dir.resolve()))
        for f in self.plugin_dir.glob("*.py"):
            if f.name.startswith("_"): continue
            try:
                spec = importlib.util.spec_from_file_location(f.stem, f)
                mod = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(mod)
                if hasattr(mod, "Handler"):
                    instance = mod.Handler()
                    if hasattr(instance, "configure"):
                        instance.configure(self.proxy_context, {})
                    self.plugins[f.stem] = instance
                    logger.info(f"Loaded: {f.stem}")
            except Exception as e:
                logger.error(f"Failed to load {f.name}: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Casparian Generalist Worker")
    parser.add_argument("--connect", default="tcp://127.0.0.1:5555", help="Sentinel Address")
    parser.add_argument("--plugins", default="plugins", help="Path to plugins directory")
    parser.add_argument("--output", default="output", help="Parquet output directory")
    args = parser.parse_args()

    engine = sql_io.get_engine(settings.database)
    parquet_root = Path(args.output)
    worker = GeneralistWorker(args.connect, Path(args.plugins), engine, parquet_root)

    try:
        worker.start()
    except KeyboardInterrupt:
        worker.stop()