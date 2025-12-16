# src/casparian_flow/sidecar.py
"""
ZMQ Plugin Sidecar - Isolated Plugin Execution Process.

This module runs plugins in a separate process from the Worker,
providing crash isolation and dependency isolation.

Usage:
    python -m casparian_flow.sidecar --plugin path/to/plugin.py --connect ipc:///tmp/casparian

Design:
- DEALER socket connects to Worker's ROUTER
- Blocking recv loop (simple, robust)
- Streaming Arrow outputs for memory efficiency
"""

import sys
import logging
import importlib.util
import traceback
from pathlib import Path
from typing import Callable, Generator, Any, Type
from dataclasses import dataclass, field

import zmq
import pyarrow as pa

from casparian_flow.protocol import (
    OpCode,
    HEADER_SIZE,
    pack_header,
    unpack_header,
    validate_header,
    msg_register,
    msg_data,
    msg_done,
    msg_error,
    msg_heartbeat,
    msg_deploy,
)

logger = logging.getLogger(__name__)


@dataclass
class SidecarContext:
    """
    Context provided to plugins running in the Sidecar.
    Mimics WorkerContext but tailored for the isolated process.
    """
    outbox: list = field(default_factory=list)

    def send_deploy(self, plugin_name: str, version: str, source_code: str, signature: str):
        """
        Queue a deployment message to be sent to the Router.
        """
        self.outbox.append({
            "type": OpCode.DEPLOY,
            "plugin_name": plugin_name,
            "source_code": source_code,
            "signature": signature
        })

    def register_topic(self, topic: str):
        # No-op for now in sidecar, or store to send REG updates?
        pass

    def publish(self, topic: str, data: Any):
        # For legacy compatibility, we might support this.
        # But prefer yielding from execute().
        pass


def load_plugin_handler(plugin_path: str) -> Type:
    """
    Dynamically load a plugin module and return the Handler class.
    Fallback to 'execute' function if no Handler class found (Legacy).
    """
    path = Path(plugin_path)
    if not path.exists():
        raise FileNotFoundError(f"Plugin not found: {plugin_path}")

    spec = importlib.util.spec_from_file_location("plugin", plugin_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load plugin spec: {plugin_path}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    if hasattr(module, "Handler"):
        return module.Handler
    elif hasattr(module, "execute"):
        # Legacy: Wrap simple function in a pseudo-Handler
        class LegacyHandler:
            def configure(self, ctx, config): pass
            def execute(self, fpath): return module.execute(fpath)
        return LegacyHandler
    else:
        raise AttributeError(f"Plugin must define 'Handler' class or 'execute' function: {plugin_path}")


def serialize_batch(batch: Any) -> bytes:
    """
    Serialize a DataFrame or RecordBatch to Arrow IPC bytes.
    Zero-copy when possible.
    """
    # Convert pandas to Arrow if needed
    if hasattr(batch, "to_arrow"):
        # pandas >= 2.0
        table = batch.to_arrow()
    elif hasattr(batch, "values"):
        # pandas DataFrame
        table = pa.Table.from_pandas(batch)
    elif isinstance(batch, pa.RecordBatch):
        table = pa.Table.from_batches([batch])
    elif isinstance(batch, pa.Table):
        table = batch
    else:
        raise TypeError(f"Cannot serialize type: {type(batch)}")

    # Write to IPC stream format
    sink = pa.BufferOutputStream()
    with pa.ipc.new_stream(sink, table.schema) as writer:
        writer.write_table(table)

    return sink.getvalue().to_pybytes()


def run_sidecar(plugin_path: str, socket_addr: str):
    """
    Main sidecar loop.
    """
    # Load plugin class
    HandlerClass = load_plugin_handler(plugin_path)
    plugin_name = Path(plugin_path).stem

    logger.info(f"Sidecar starting: plugin={plugin_name}, connect={socket_addr}")

    # Create ZMQ context and socket
    ctx = zmq.Context()
    sock = ctx.socket(zmq.DEALER)
    sock.connect(socket_addr)

    # Register with Worker
    reg_frames = msg_register(plugin_name)
    sock.send_multipart(reg_frames)
    logger.info(f"Registered plugin: {plugin_name}")

    # Create Sidecar Context
    sidecar_ctx = SidecarContext()
    
    # Instantiate Handler
    # We pass empty config for now. Real implementations might fetch config from Router via REG response?
    # For now, we assume stateless or default config.
    handler = HandlerClass()
    if hasattr(handler, "configure"):
        handler.configure(sidecar_ctx, {})

    # Main loop
    try:
        while True:
            # 1. Drain Outbox (Control Messages from Plugin)
            while sidecar_ctx.outbox:
                msg = sidecar_ctx.outbox.pop(0)
                if msg["type"] == OpCode.DEPLOY:
                    logger.info(f"Sending DEPLOY for {msg['plugin_name']}")
                    frames = msg_deploy(
                        msg["plugin_name"], 
                        msg["source_code"], 
                        msg["signature"]
                    )
                    sock.send_multipart(frames)

            # 2. Receive Messages (Non-blocking check could be better, but we stick to blocking for simplicity)
            # To allow outbox draining, we might need a poller if plugins are chatty.
            # But currently, plugins only act when triggered by EXEC.
            # So blocking receive is fine. 'send_deploy' happens during 'execute'.
            
            frames = sock.recv_multipart()
            if len(frames) < 1: continue

            # Validate header
            err = validate_header(frames[0])
            if err:
                logger.warning(f"Invalid header: {err}")
                continue

            # Protocol v2
            op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])

            if op == OpCode.HEARTBEAT:
                sock.send_multipart(msg_heartbeat())
                continue

            elif op == OpCode.EXEC:
                if len(frames) < 2:
                    sock.send_multipart(msg_error(job_id, "Missing filepath"))
                    continue

                filepath = frames[1].decode("utf-8")
                logger.info(f"Executing job {job_id}: {filepath}")

                try:
                    result = handler.execute(filepath)

                    # Handle Generator
                    if hasattr(result, "__iter__") and hasattr(result, "__next__"):
                        for batch in result:
                            # Check if plugin queued control messages during iteration
                            while sidecar_ctx.outbox:
                                msg = sidecar_ctx.outbox.pop(0)
                                if msg["type"] == OpCode.DEPLOY:
                                    sock.send_multipart(msg_deploy(msg["plugin_name"], msg["source_code"], msg["signature"]))
                            
                            payload = serialize_batch(batch)
                            sock.send_multipart(msg_data(job_id, payload))

                    elif result is not None:
                        payload = serialize_batch(result)
                        sock.send_multipart(msg_data(job_id, payload))
                    
                    # Check outbox again after completion
                    while sidecar_ctx.outbox:
                        msg = sidecar_ctx.outbox.pop(0)
                        if msg["type"] == OpCode.DEPLOY:
                             sock.send_multipart(msg_deploy(msg["plugin_name"], msg["source_code"], msg["signature"]))

                    sock.send_multipart(msg_done(job_id))
                    logger.info(f"Job {job_id} completed")

                except Exception as e:
                    tb = traceback.format_exc()
                    logger.error(f"Job {job_id} failed: {e}")
                    sock.send_multipart(msg_error(job_id, tb))
            else:
                logger.warning(f"Unexpected op_code: {op}")

    except KeyboardInterrupt:
        logger.info("Sidecar shutting down")
    except Exception as e:
        logger.critical(f"Sidecar fatal error: {e}", exc_info=True)
    finally:
        sock.close()
        ctx.term()


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Casparian Flow Plugin Sidecar")
    parser.add_argument("--plugin", required=True, help="Path to plugin .py file")
    parser.add_argument(
        "--connect",
        required=True,
        help="ZMQ socket address (e.g., ipc:///tmp/casparian)",
    )
    parser.add_argument("--log-level", default="INFO", help="Logging level")

    args = parser.parse_args()

    logging.basicConfig(
        level=getattr(logging, args.log_level.upper()),
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    run_sidecar(args.plugin, args.connect)


if __name__ == "__main__":
    main()
