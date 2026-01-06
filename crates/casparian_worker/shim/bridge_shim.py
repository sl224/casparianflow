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

# Add shim directory to sys.path so casparian_types can be imported
_shim_dir = Path(__file__).parent.resolve()
if str(_shim_dir) not in sys.path:
    sys.path.insert(0, str(_shim_dir))

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
#   LENGTH=0xFFFFFFFE: Log message (sideband logging)

HEADER_FORMAT = "!I"  # 4-byte unsigned int (big-endian)
HEADER_SIZE = 4
END_OF_STREAM = 0
ERROR_SIGNAL = 0xFFFFFFFF
LOG_SIGNAL = 0xFFFFFFFE  # Sideband logging signal

# Log levels for protocol
LOG_LEVEL_STDOUT = 0
LOG_LEVEL_STDERR = 1
LOG_LEVEL_DEBUG = 2
LOG_LEVEL_INFO = 3
LOG_LEVEL_WARNING = 4
LOG_LEVEL_ERROR = 5


class SocketWriter:
    """
    File-like object that captures writes and sends them via sideband logging.

    Implements the minimal file interface needed to replace sys.stdout/stderr.
    Buffers small writes until newline to reduce socket overhead.
    """

    def __init__(self, context: "BridgeContext", level: int):
        self.context = context
        self.level = level
        self._buffer = ""
        self._original = sys.stdout if level == LOG_LEVEL_STDOUT else sys.stderr

    def write(self, text: str) -> int:
        """Buffer writes and flush on newline."""
        if not text:
            return 0

        self._buffer += text

        # Flush complete lines immediately
        while "\n" in self._buffer:
            line, self._buffer = self._buffer.split("\n", 1)
            if line:  # Skip empty lines
                self.context.send_log(self.level, line)

        return len(text)

    def flush(self) -> None:
        """Flush any remaining buffered content."""
        if self._buffer:
            self.context.send_log(self.level, self._buffer)
            self._buffer = ""

    def fileno(self) -> int:
        """Return original file descriptor for compatibility."""
        return self._original.fileno()

    def isatty(self) -> bool:
        """Not a TTY when redirected."""
        return False

    def readable(self) -> bool:
        return False

    def writable(self) -> bool:
        return True

    def seekable(self) -> bool:
        return False


class BridgeLogHandler(logging.Handler):
    """
    Logging handler that routes log records through the bridge sideband channel.
    """

    LEVEL_MAP = {
        logging.DEBUG: LOG_LEVEL_DEBUG,
        logging.INFO: LOG_LEVEL_INFO,
        logging.WARNING: LOG_LEVEL_WARNING,
        logging.ERROR: LOG_LEVEL_ERROR,
        logging.CRITICAL: LOG_LEVEL_ERROR,
    }

    def __init__(self, context: "BridgeContext"):
        super().__init__()
        self.context = context
        self.setFormatter(logging.Formatter("%(levelname)s: %(message)s"))

    def emit(self, record: logging.LogRecord) -> None:
        """Send log record through sideband channel."""
        try:
            msg = self.format(record)
            level = self.LEVEL_MAP.get(record.levelno, LOG_LEVEL_INFO)
            self.context.send_log(level, msg)
        except Exception:
            # Avoid recursion - don't log failures in logging
            pass


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
                # Can't use logger here - it might route back through socket
                sys.__stderr__.write(f"Error sending error signal: {e}\n")

    def send_log(self, level: int, message: str):
        """
        Send a log message through the sideband channel.

        Protocol: [LOG_SIGNAL:4][LEVEL:1][LENGTH:4][MESSAGE]

        Args:
            level: Log level (LOG_LEVEL_STDOUT, LOG_LEVEL_STDERR, etc.)
            message: Log message text
        """
        if not self._socket:
            return

        try:
            # Encode message
            msg_bytes = message.encode("utf-8", errors="replace")

            # Cap message size to prevent abuse (64KB per message)
            if len(msg_bytes) > 65536:
                msg_bytes = msg_bytes[:65530] + b"[...]"

            # Send: [LOG_SIGNAL:4][LEVEL:1][LENGTH:4][MESSAGE]
            self._socket.sendall(struct.pack(HEADER_FORMAT, LOG_SIGNAL))
            self._socket.sendall(struct.pack("!B", level))  # 1 byte level
            self._socket.sendall(struct.pack(HEADER_FORMAT, len(msg_bytes)))
            self._socket.sendall(msg_bytes)

        except Exception as e:
            # Write to real stderr, not the redirected one
            sys.__stderr__.write(f"Failed to send log: {e}\n")

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
        - polars DataFrame
        - pandas DataFrame
        - pyarrow Table
        - pyarrow RecordBatch
        """
        try:
            import pandas as pd
            import pyarrow as pa

            # Convert to Arrow Table
            # Check for polars first (uses to_arrow() method)
            if hasattr(data, "to_arrow") and callable(data.to_arrow):
                # polars DataFrame
                table = data.to_arrow()
            elif isinstance(data, pd.DataFrame):
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

    Supports two patterns:
    1. parse() function (new) - returns DataFrame or list[Output]
    2. Handler class (legacy) - execute()/consume() methods that yield batches

    Args:
        source_code: Plugin source code
        file_path: Path to input file
        file_version_id: File version ID for lineage tracking
        context: BridgeContext for IPC communication

    Returns:
        dict with execution metrics including output_info for multi-output parsers
    """
    # Import Output class from casparian_types
    from casparian_types import Output, validate_output

    # Create a module namespace for the plugin
    plugin_namespace = {
        "__name__": "__bridge_plugin__",
        "__file__": "<bridge>",
        "__builtins__": __builtins__,
        "Output": Output,  # Inject Output class
    }

    # Execute the plugin source code to define classes/functions
    exec(source_code, plugin_namespace)

    # Track output metadata for multi-output parsers
    output_info = []

    # Check for parse() function (new pattern)
    if "parse" in plugin_namespace and callable(plugin_namespace["parse"]):
        parse_fn = plugin_namespace["parse"]

        # Get TOPIC and SINK constants for single-output wrapping
        topic = plugin_namespace.get("TOPIC", "default")
        sink = plugin_namespace.get("SINK", "parquet")

        # Call the parse function
        result = parse_fn(file_path)

        # Handle return type
        if result is None:
            # Empty result
            pass
        elif isinstance(result, list) and len(result) > 0 and isinstance(result[0], Output):
            # Multi-output: list[Output]
            for out in result:
                validate_output(out)
                context.publish(1, out.data)
                output_info.append({
                    "name": out.name,
                    "sink": out.sink,
                    "table": out.table,
                    "compression": out.compression,
                })
        elif hasattr(result, "to_arrow") or hasattr(result, "to_pandas") or hasattr(result, "schema"):
            # Single output: bare DataFrame/Table - wrap with TOPIC/SINK constants
            context.publish(1, result)
            output_info.append({
                "name": topic,
                "sink": sink,
                "table": None,
                "compression": "snappy",
            })
        else:
            raise TypeError(
                f"parse() must return DataFrame, Table, or list[Output], got {type(result)}"
            )

    # Check for Handler class (legacy pattern)
    elif "Handler" in plugin_namespace:
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
                    # Check if batch is an Output object
                    if isinstance(batch, Output):
                        validate_output(batch)
                        context.publish(1, batch.data)
                        output_info.append({
                            "name": batch.name,
                            "sink": batch.sink,
                            "table": batch.table,
                            "compression": batch.compression,
                        })
                    else:
                        # Legacy: bare DataFrame/Table
                        context.publish(1, batch)

    else:
        raise ValueError("Plugin must define either a 'parse' function or a 'Handler' class")

    return {
        "rows_published": context.get_row_count(),
        "status": "SUCCESS",
        "output_info": output_info,
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

        # === SIDEBAND LOGGING SETUP ===
        # Hijack stdout/stderr BEFORE executing user code to capture print() statements
        # Save originals for fallback
        original_stdout = sys.stdout
        original_stderr = sys.stderr

        # Replace with socket writers
        sys.stdout = SocketWriter(context, LOG_LEVEL_STDOUT)
        sys.stderr = SocketWriter(context, LOG_LEVEL_STDERR)

        # Route logging through sideband channel
        bridge_handler = BridgeLogHandler(context)
        root_logger = logging.getLogger()
        # Remove default handlers
        for handler in root_logger.handlers[:]:
            root_logger.removeHandler(handler)
        root_logger.addHandler(bridge_handler)

        try:
            # Execute plugin
            metrics = execute_plugin(source_code, file_path, file_version_id, context)

            # Flush any remaining buffered output
            sys.stdout.flush()
            sys.stderr.flush()

            # Log success through sideband
            logger.info(f"Plugin execution completed: {metrics}")

            # Send completion
            context.close()

            # Restore stdout for final JSON output to host process
            sys.stdout = original_stdout
            print(json.dumps(metrics))
            sys.exit(0)

        finally:
            # Always restore stdio in case of exception
            sys.stdout = original_stdout
            sys.stderr = original_stderr

    except Exception as e:
        error_msg = traceback.format_exc()

        # Try to send through sideband if socket still connected
        if context._socket:
            context.send_log(LOG_LEVEL_ERROR, f"Plugin execution failed: {e}\n{error_msg}")

        # Send error to Host via error signal
        context.send_error(str(e))
        context.close()

        # Print error metrics to original stdout
        print(json.dumps({
            "status": "FAILED",
            "error": str(e),
        }))
        sys.exit(1)


if __name__ == "__main__":
    main()
