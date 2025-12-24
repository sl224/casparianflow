#!/usr/bin/env python3
# src/casparian_flow/engine/bridge_shim.py
"""
v5.0 Bridge Mode: Guest Process Shim.

This script runs inside the isolated venv and acts as the "Guest" side
of the Host/Guest privilege separation model.

Guest Side Role:
- Pure Logic execution (no credentials, no heavy drivers)
- Minimal dependencies: pandas, pyarrow
- Receives plugin code and file path via environment
- Streams Arrow IPC batches to the Host via AF_UNIX socket

Security Model:
- No access to AWS credentials, DB passwords, or heavy drivers
- stdout/stderr redirected to Host logging (keeps data pipe binary-pure)
- Sandboxed execution with minimal attack surface

Communication Protocol:
1. Read configuration from environment variables
2. Execute plugin code with provided file path
3. Stream Arrow IPC batches to socket
4. Send completion signal and exit

Usage:
    BRIDGE_SOCKET=/tmp/bridge.sock \
    BRIDGE_PLUGIN_CODE=<base64> \
    BRIDGE_FILE_PATH=/path/to/file.csv \
    BRIDGE_JOB_ID=123 \
    BRIDGE_FILE_VERSION_ID=456 \
    python bridge_shim.py
"""

import os
import sys
import json
import base64
import socket
import struct
import logging
import traceback
from typing import Iterator, Any, Optional
from pathlib import Path
from io import BytesIO

# Configure logging to stderr (keeps stdout for structured output)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [GUEST] %(levelname)s: %(message)s",
    stream=sys.stderr,
)
logger = logging.getLogger(__name__)


# --- Arrow IPC Protocol ---
# Message format: [LENGTH:4][ARROW_IPC_BATCH]
# Special messages:
#   LENGTH=0: End of stream
#   LENGTH=0xFFFFFFFF: Error (followed by UTF-8 error message)

HEADER_FORMAT = "!I"  # 4-byte unsigned int (big-endian)
HEADER_SIZE = 4
END_OF_STREAM = 0
ERROR_SIGNAL = 0xFFFFFFFF


class BridgeContext:
    """
    Minimal context for plugin execution in Bridge Mode.

    Provides a publish() method that streams Arrow IPC to the Host.
    """

    def __init__(self, socket_path: str, job_id: int):
        self.socket_path = socket_path
        self.job_id = job_id
        self._socket: Optional[socket.socket] = None
        self._topics: dict[int, str] = {}
        self._next_handle = 1
        self._row_count = 0

    def connect(self):
        """Connect to the Host via AF_UNIX socket."""
        self._socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._socket.connect(self.socket_path)
        logger.info(f"Connected to bridge socket: {self.socket_path}")

    def close(self):
        """Send end-of-stream and close the socket."""
        if self._socket:
            try:
                # Send end-of-stream signal
                self._socket.sendall(struct.pack(HEADER_FORMAT, END_OF_STREAM))
                self._socket.close()
            except Exception as e:
                logger.error(f"Error closing socket: {e}")
            finally:
                self._socket = None

    def send_error(self, message: str):
        """Send an error signal to the Host."""
        if self._socket:
            try:
                # Error signal header
                self._socket.sendall(struct.pack(HEADER_FORMAT, ERROR_SIGNAL))
                # Error message
                error_bytes = message.encode("utf-8")
                self._socket.sendall(struct.pack(HEADER_FORMAT, len(error_bytes)))
                self._socket.sendall(error_bytes)
            except Exception as e:
                logger.error(f"Error sending error signal: {e}")

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        """Register a topic and return a handle."""
        handle = self._next_handle
        self._topics[handle] = topic
        self._next_handle += 1
        return handle

    def publish(self, handle: int, data: Any):
        """
        Publish data via Arrow IPC.

        Supports:
        - pandas DataFrame
        - pyarrow Table
        - pyarrow RecordBatch
        """
        try:
            import pandas as pd
            import pyarrow as pa

            # Convert to Arrow Table
            if isinstance(data, pd.DataFrame):
                table = pa.Table.from_pandas(data)
            elif isinstance(data, pa.Table):
                table = data
            elif isinstance(data, pa.RecordBatch):
                table = pa.Table.from_batches([data])
            else:
                raise TypeError(f"Unsupported data type: {type(data)}")

            # Serialize to IPC
            sink = BytesIO()
            with pa.ipc.new_stream(sink, table.schema) as writer:
                for batch in table.to_batches():
                    writer.write_batch(batch)

            ipc_bytes = sink.getvalue()
            self._row_count += table.num_rows

            # Send to Host: [LENGTH][IPC_DATA]
            self._socket.sendall(struct.pack(HEADER_FORMAT, len(ipc_bytes)))
            self._socket.sendall(ipc_bytes)

            logger.debug(f"Published {table.num_rows} rows ({len(ipc_bytes)} bytes)")

        except Exception as e:
            logger.error(f"Publish failed: {e}")
            raise

    def get_row_count(self) -> int:
        """Get total rows published."""
        return self._row_count


def execute_plugin(
    source_code: str,
    file_path: str,
    file_version_id: int,
    context: BridgeContext,
) -> dict:
    """
    Execute plugin code with the provided file path.

    Args:
        source_code: Plugin source code
        file_path: Path to input file
        file_version_id: File version ID for lineage tracking
        context: BridgeContext for IPC communication

    Returns:
        dict with execution metrics
    """
    # Create a module namespace for the plugin
    plugin_namespace = {
        "__name__": "__bridge_plugin__",
        "__file__": "<bridge>",
        "__builtins__": __builtins__,
    }

    # Execute the plugin source code to define classes/functions
    exec(source_code, plugin_namespace)

    # Look for the Handler class
    if "Handler" not in plugin_namespace:
        raise ValueError("Plugin must define a 'Handler' class")

    handler_class = plugin_namespace["Handler"]
    handler = handler_class()

    # Configure the handler with context
    if hasattr(handler, "configure"):
        handler.configure(context, {})

    # Create file event with lineage tracking
    file_event = type("FileEvent", (), {"path": file_path, "file_id": file_version_id})()

    # Execute the plugin
    if hasattr(handler, "consume") and callable(handler.consume):
        try:
            result = handler.consume(file_event)
        except NotImplementedError:
            result = handler.execute(file_path)
    elif hasattr(handler, "execute"):
        result = handler.execute(file_path)
    else:
        raise ValueError("Handler must have 'consume' or 'execute' method")

    # Handle generator results
    if result:
        for batch in result:
            if batch is not None:
                context.publish(1, batch)

    return {
        "rows_published": context.get_row_count(),
        "status": "SUCCESS",
    }


def main():
    """Main entry point for Bridge Shim."""
    # Read configuration from environment
    socket_path = os.environ.get("BRIDGE_SOCKET")
    plugin_code_b64 = os.environ.get("BRIDGE_PLUGIN_CODE")
    file_path = os.environ.get("BRIDGE_FILE_PATH")
    job_id = int(os.environ.get("BRIDGE_JOB_ID", "0"))
    file_version_id = int(os.environ.get("BRIDGE_FILE_VERSION_ID", "0"))

    if not all([socket_path, plugin_code_b64, file_path]):
        logger.error(
            "Missing required environment variables: "
            "BRIDGE_SOCKET, BRIDGE_PLUGIN_CODE, BRIDGE_FILE_PATH"
        )
        sys.exit(1)

    # Decode plugin source code
    try:
        source_code = base64.b64decode(plugin_code_b64).decode("utf-8")
    except Exception as e:
        logger.error(f"Failed to decode plugin code: {e}")
        sys.exit(1)

    # Create context and connect
    context = BridgeContext(socket_path, job_id)

    try:
        context.connect()

        # Execute plugin
        metrics = execute_plugin(source_code, file_path, file_version_id, context)

        # Log success
        logger.info(f"Plugin execution completed: {metrics}")

        # Send completion
        context.close()

        # Print metrics to stdout for Host to capture
        print(json.dumps(metrics))
        sys.exit(0)

    except Exception as e:
        error_msg = traceback.format_exc()
        logger.error(f"Plugin execution failed: {e}\n{error_msg}")

        # Send error to Host
        context.send_error(str(e))
        context.close()

        # Print error metrics
        print(json.dumps({
            "status": "FAILED",
            "error": str(e),
        }))
        sys.exit(1)


if __name__ == "__main__":
    main()
