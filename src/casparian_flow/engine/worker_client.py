# src/casparian_flow/engine/worker_client.py
"""
Generalist Worker for Casparian Flow.

v5.0 Bridge Mode: All execution happens in isolated venvs via subprocess + Arrow IPC.
Legacy in-process execution has been removed.
"""
import zmq
import logging
import time
import argparse
from pathlib import Path
from typing import Any, Optional

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
)
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.engine.sinks import SinkFactory
from casparian_flow.engine.venv_manager import get_venv_manager, VenvManagerError
from casparian_flow.engine.bridge import BridgeExecutor, BridgeError

logging.basicConfig(level=logging.INFO, format="%(asctime)s [WORKER] %(message)s")
logger = logging.getLogger(__name__)


class GeneralistWorker:
    """
    Bridge Mode Worker: Executes plugins in isolated venvs only.

    The worker receives DISPATCH commands with env_hash and source_code,
    spawns subprocess executors, and streams results to configured sinks.
    """

    def __init__(
        self,
        sentinel_addr: str,
        db_engine: Any,
        parquet_root: Optional[Path] = None,
    ):
        self.sentinel_addr = sentinel_addr
        self.db_engine = db_engine
        self.parquet_root = parquet_root or Path("output")

        self.ctx = zmq.Context()
        self.socket = self.ctx.socket(zmq.DEALER)
        # CRITICAL: Prevent hang on exit
        self.socket.setsockopt(zmq.LINGER, 0)

        self.identity = f"w-{time.time_ns()}".encode()
        self.socket.setsockopt(zmq.IDENTITY, self.identity)

        self.running = False
        self.current_job_id: Optional[int] = None

    def start(self):
        """Main Loop."""
        self.running = True

        logger.info(f"Dialing Sentinel at {self.sentinel_addr}...")
        self.socket.connect(self.sentinel_addr)

        # Capabilities: Bridge Mode worker can execute any plugin
        caps = ["*"]  # Universal capability - execution is env-isolated
        worker_id = self.identity.decode()
        self.socket.send_multipart(msg_identify(caps, worker_id))

        poller = zmq.Poller()
        poller.register(self.socket, zmq.POLLIN)

        logger.info("Entering Event Loop...")

        try:
            while self.running:
                try:
                    socks = dict(poller.poll(timeout=100))

                    if self.socket in socks:
                        self._handle_message()
                except zmq.ZMQError as e:
                    if not self.running:
                        break  # Expected during shutdown
                    logger.error(f"ZMQ Error: {e}")
                    break

        except Exception as e:
            logger.critical(f"Worker crashed: {e}", exc_info=True)
        finally:
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
            # Parse the DispatchCommand (now requires env_hash and source_code)
            try:
                cmd = DispatchCommand(**payload_dict)
            except Exception as e:
                logger.error(f"Failed to parse DISPATCH command: {e}")
                self.socket.send_multipart(
                    msg_err(job_id, f"Invalid DISPATCH payload: {e}")
                )
                return

            logger.info(f"Received Job {job_id} -> {cmd.plugin_name}")

            # Execute in Bridge Mode (only mode supported)
            self._execute_job(
                job_id=job_id,
                plugin_name=cmd.plugin_name,
                file_path=cmd.file_path,
                sink_configs=cmd.sinks,
                file_version_id=cmd.file_version_id,
                env_hash=cmd.env_hash,
                source_code=cmd.source_code,
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
                self.socket.send_multipart(msg_err(0, f"PREPARE_ENV failed: {e}"))

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
                file_version_id=file_version_id,  # Pass for lineage
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
            self.socket.send_multipart(msg_err(0, f"PREPARE_ENV failed: {e}"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Casparian Generalist Worker")
    parser.add_argument(
        "--connect", default="tcp://127.0.0.1:5555", help="Sentinel Address"
    )
    parser.add_argument(
        "--output", default="output", help="Parquet output directory"
    )
    args = parser.parse_args()

    engine = sql_io.get_engine(settings.database)
    parquet_root = Path(args.output)
    worker = GeneralistWorker(args.connect, engine, parquet_root)

    try:
        worker.start()
    except KeyboardInterrupt:
        worker.stop()
