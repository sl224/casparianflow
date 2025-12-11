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
from typing import Callable, Generator, Any

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
)

logger = logging.getLogger(__name__)


def load_plugin(plugin_path: str) -> Callable:
    """
    Dynamically load a plugin and return its execute function.

    The plugin must define an `execute(filepath)` function that yields
    pandas DataFrames or PyArrow RecordBatches.
    """
    path = Path(plugin_path)
    if not path.exists():
        raise FileNotFoundError(f"Plugin not found: {plugin_path}")

    spec = importlib.util.spec_from_file_location("plugin", plugin_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load plugin spec: {plugin_path}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    if not hasattr(module, "execute"):
        raise AttributeError(f"Plugin must define 'execute' function: {plugin_path}")

    return module.execute


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

    1. Load plugin
    2. Connect to Worker
    3. Register plugin name
    4. Wait for EXEC messages
    5. Stream DATA back, send DONE/ERR
    """
    # Load plugin once at startup
    execute_fn = load_plugin(plugin_path)
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

    # Main loop
    try:
        while True:
            frames = sock.recv_multipart()

            if len(frames) < 1:
                logger.warning("Empty message received")
                continue

            # Validate header
            err = validate_header(frames[0])
            if err:
                logger.warning(f"Invalid header: {err}")
                continue

            op, job_id, meta_len = unpack_header(frames[0])

            if op == OpCode.EXEC:
                # Get filepath from frame 1
                if len(frames) < 2:
                    logger.error(f"EXEC missing filepath frame: job_id={job_id}")
                    sock.send_multipart(msg_error(job_id, "Missing filepath"))
                    continue

                filepath = frames[1].decode("utf-8")
                logger.info(f"Executing job {job_id}: {filepath}")

                try:
                    # Execute plugin - expect generator yielding batches
                    result = execute_fn(filepath)

                    # Handle both generator and direct return
                    if hasattr(result, "__iter__") and hasattr(result, "__next__"):
                        # Generator - stream batches
                        for batch in result:
                            payload = serialize_batch(batch)
                            sock.send_multipart(msg_data(job_id, payload))
                    elif result is not None:
                        # Single result - wrap and send
                        payload = serialize_batch(result)
                        sock.send_multipart(msg_data(job_id, payload))

                    # Signal completion
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
