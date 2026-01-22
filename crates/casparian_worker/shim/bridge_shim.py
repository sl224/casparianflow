#!/usr/bin/env python3
# src/casparian_flow/engine/bridge_shim.py
"""
v6.0 Bridge Mode: Guest Process Shim.

This script runs inside the isolated venv and acts as the "Guest" side
of the Host/Guest privilege separation model.

Guest Side Role:
- Pure Logic execution (no credentials, no heavy drivers)
- Minimal dependencies: pandas, pyarrow
- Receives plugin code and file path via environment or CLI args
- Streams Arrow IPC batches to the Host via TCP socket

Security Model:
- No access to AWS credentials, DB passwords, or heavy drivers
- stdout/stderr redirected to Host logging (unless BRIDGE_STDIO_MODE=inherit)
- Sandboxed execution with minimal attack surface

Communication Protocol:
1. Read configuration from environment variables and CLI args
2. Connect to Host via TCP on 127.0.0.1:BRIDGE_PORT
3. Execute plugin code with provided file path
4. Stream Arrow IPC batches to socket
5. Send completion signal and exit

Exit Code Convention:
- 0: Success
- 1: Permanent error (no retry) - parse errors, validation failures, bad data
- 2: Transient error (retry eligible) - timeout, OOM, network issues

Safety Features:
- safe_to_arrow(): Handles mixed-type columns with string fallback
- check_memory_for_batch(): OOM prevention (3x batch size rule)
- PermanentError/TransientError: Explicit error classification

Usage (Environment Variables):
    BRIDGE_PORT=12345 \
    BRIDGE_PLUGIN_CODE=<base64> \
    BRIDGE_FILE_PATH=/path/to/file.csv \
    BRIDGE_JOB_ID=123 \
    BRIDGE_FILE_ID=456 \
    python bridge_shim.py

Usage (CLI Arguments - Dev Mode):
    BRIDGE_PORT=12345 \
    BRIDGE_FILE_PATH=/path/to/file.csv \
    python bridge_shim.py --parser-path /path/to/parser.py

Usage (CLI Arguments - Prod Mode):
    BRIDGE_PORT=12345 \
    BRIDGE_FILE_PATH=/path/to/file.csv \
    python bridge_shim.py --parser-archive <base64-zip>
"""

import os
import sys
import json
import base64
import socket
import struct
import logging
import traceback
import argparse
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


# --- Error Classes ---

class PermanentError(Exception):
    """Error that should not be retried (parse error, validation failure, bad data)."""
    pass


class TransientError(Exception):
    """Error eligible for retry (timeout, connection reset, OOM, network issues)."""
    pass


# --- Safety Functions ---

def safe_to_arrow(df: "pd.DataFrame") -> "pa.Table":
    """
    Convert DataFrame to Arrow with string fallback for mixed-type columns.

    Ensures data always reaches Rust for quarantine processing,
    rather than crashing in Python due to mixed types.

    Optimization: On fallback, builds Arrow arrays directly instead of
    retry-calling from_pandas() which would re-process all columns.
    """
    import pandas as pd
    import pyarrow as pa

    try:
        return pa.Table.from_pandas(df)
    except (pa.ArrowInvalid, pa.ArrowTypeError) as e:
        logger.warning(f"Arrow conversion failed, attempting column-by-column fallback: {e}")

        # Build Arrow arrays directly (single pass, no retry)
        arrays = []
        names = []
        for col in df.columns:
            names.append(col)
            try:
                # Try to convert column directly
                arr = pa.array(df[col], from_pandas=True)
            except (pa.ArrowInvalid, pa.ArrowTypeError):
                # Fallback: convert to string
                logger.warning(f"Column '{col}' has mixed types, converting to string")
                arr = pa.array(df[col].astype(str), type=pa.string())
            arrays.append(arr)

        return pa.table(dict(zip(names, arrays)))


def check_memory_for_batch(df: "pd.DataFrame") -> bool:
    """
    Check if enough RAM for Arrow conversion (3x batch size rule).

    Returns True if safe to proceed, False if OOM risk.
    """
    try:
        import psutil
        available = psutil.virtual_memory().available
        estimated = df.memory_usage(deep=True).sum() * 3

        if estimated > available:
            logger.error(
                f"OOM risk: batch requires ~{estimated / 1e9:.1f}GB, "
                f"only {available / 1e9:.1f}GB available"
            )
            return False
        return True
    except ImportError:
        # psutil not available - OOM protection disabled! This is a bug.
        logger.warning(
            "psutil not installed - OOM protection DISABLED. "
            "Install psutil to enable memory checks: pip install psutil"
        )
        return True


# --- Arrow IPC Protocol ---
# Message format: [LENGTH:4][ARROW_IPC_BATCH]
# Special messages:
#   LENGTH=0: End of stream
#   LENGTH=0xFFFFFFFF: Error (followed by UTF-8 error message)
#   LENGTH=0xFFFFFFFE: Log message (sideband logging)
#   LENGTH=0xFFFFFFFD: Output start (followed by output index)
#   LENGTH=0xFFFFFFFC: Output end (followed by output index)
#   LENGTH=0xFFFFFFFB: Metrics payload (JSON)

HEADER_FORMAT = "!I"  # 4-byte unsigned int (big-endian)
HEADER_SIZE = 4
END_OF_STREAM = 0
ERROR_SIGNAL = 0xFFFFFFFF
LOG_SIGNAL = 0xFFFFFFFE  # Sideband logging signal
OUTPUT_START_SIGNAL = 0xFFFFFFFD
OUTPUT_END_SIGNAL = 0xFFFFFFFC
METRICS_SIGNAL = 0xFFFFFFFB

# Limit batch size by rows to avoid huge IPC frames.
MAX_ROWS_PER_BATCH = 50_000
MAX_METRICS_BYTES = 1024 * 1024

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

    def __init__(self, port: int, job_id: int):
        self.port = port
        self.job_id = job_id
        self._socket: Optional[socket.socket] = None
        self._topics: dict[int, str] = {}
        self._next_handle = 1
        self._row_count = 0
        self._output_index = 0

    def connect(self):
        """Connect to the Host via TCP."""
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        if self.port == 0:
            raise RuntimeError("BRIDGE_PORT environment variable not set or is 0")
        self._socket.connect(('127.0.0.1', self.port))
        logger.info(f"Connected to host on port {self.port}")

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

    def send_error(self, message: str, retryable: bool, kind: str):
        """Send an error signal to the Host."""
        if self._socket:
            try:
                # Error signal header
                self._socket.sendall(struct.pack(HEADER_FORMAT, ERROR_SIGNAL))
                # Error message
                payload = {
                    "error": message,
                    "retryable": retryable,
                    "kind": kind,
                }
                error_bytes = json.dumps(payload).encode("utf-8")
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

    def send_metrics(self, metrics: dict):
        """Send metrics JSON through the sideband channel."""
        if not self._socket:
            return

        try:
            payload = json.dumps(metrics).encode("utf-8")
            if len(payload) > MAX_METRICS_BYTES:
                sys.__stderr__.write("Metrics payload too large; skipping metrics send\n")
                return

            self._socket.sendall(struct.pack(HEADER_FORMAT, METRICS_SIGNAL))
            self._socket.sendall(struct.pack(HEADER_FORMAT, len(payload)))
            self._socket.sendall(payload)
        except Exception as e:
            sys.__stderr__.write(f"Failed to send metrics: {e}\n")

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        """Register a topic and return a handle."""
        handle = self._next_handle
        self._topics[handle] = topic
        self._next_handle += 1
        return handle

    def publish(self, handle: int, data: Any, skip_memory_check: bool = False):
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
                # Check memory before conversion
                if not skip_memory_check and not check_memory_for_batch(data):
                    raise TransientError("Insufficient memory for Arrow conversion")
                table = safe_to_arrow(data)
            elif isinstance(data, pa.Table):
                table = data
            elif isinstance(data, pa.RecordBatch):
                table = pa.Table.from_batches([data])
            else:
                raise TypeError(f"Unsupported data type: {type(data)}")

            # Stream IPC batches to host with output boundaries
            self._output_index += 1
            output_index = self._output_index

            self._socket.sendall(struct.pack(HEADER_FORMAT, OUTPUT_START_SIGNAL))
            self._socket.sendall(struct.pack(HEADER_FORMAT, output_index))

            total_rows = 0
            batches = []
            if table.num_rows == 0:
                empty_arrays = [
                    pa.array([], type=field.type) for field in table.schema
                ]
                batches = [pa.RecordBatch.from_arrays(empty_arrays, schema=table.schema)]
            else:
                batches = table.to_batches(max_chunksize=MAX_ROWS_PER_BATCH)

            for batch in batches:
                total_rows += batch.num_rows
                sink = BytesIO()
                with pa.ipc.new_stream(sink, batch.schema) as writer:
                    writer.write_batch(batch)

                ipc_bytes = sink.getvalue()
                self._socket.sendall(struct.pack(HEADER_FORMAT, len(ipc_bytes)))
                self._socket.sendall(ipc_bytes)

            self._socket.sendall(struct.pack(HEADER_FORMAT, OUTPUT_END_SIGNAL))
            self._socket.sendall(struct.pack(HEADER_FORMAT, output_index))

            self._row_count += total_rows
            logger.debug(f"Published {total_rows} rows across output {output_index}")

        except Exception as e:
            logger.error(f"Publish failed: {e}")
            raise

    def get_row_count(self) -> int:
        """Get total rows published."""
        return self._row_count


def execute_plugin(
    source_code: str,
    file_path: str,
    file_id: int,
    context: BridgeContext,
) -> dict:
    """
    Execute plugin code with the provided file path.

    Supports a single pattern:
    1. parse() function - returns DataFrame or list[Output]

    Args:
        source_code: Plugin source code
        file_path: Path to input file
        file_id: File ID for lineage tracking
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

    # Check for parse() function
    if "parse" in plugin_namespace and callable(plugin_namespace["parse"]):
        parse_fn = plugin_namespace["parse"]

        # Get TOPIC constant for single-output wrapping
        topic = plugin_namespace.get("TOPIC", "default")

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
                    "table": out.table,
                })
        else:
            try:
                import pandas as pd
            except Exception:  # pragma: no cover - pandas optional
                pd = None

            is_pandas_df = pd is not None and isinstance(result, pd.DataFrame)

            if is_pandas_df or hasattr(result, "to_arrow") or hasattr(result, "to_pandas") or hasattr(result, "schema"):
                # Single output: bare DataFrame/Table - wrap with TOPIC/SINK constants
                context.publish(1, result)
                output_info.append({
                    "name": topic,
                    "table": None,
                })
            else:
                raise TypeError(
                    f"parse() must return DataFrame, Table, or list[Output], got {type(result)}"
                )

    else:
        raise ValueError("Plugin must define a 'parse' function")

    return {
        "rows_published": context.get_row_count(),
        "status": "SUCCESS",
        "output_info": output_info,
    }


def main():
    """Main entry point for Bridge Shim."""
    # Parse command line arguments
    parser = argparse.ArgumentParser(description="Bridge Shim for Casparian Flow")
    parser.add_argument('--parser-path', help='Dev mode: path to parser.py file')
    parser.add_argument('--parser-archive', help='Prod mode: base64-encoded ZIP archive')
    args = parser.parse_args()

    # Read configuration from environment
    port_str = os.environ.get("BRIDGE_PORT", "0")
    plugin_code_b64 = os.environ.get("BRIDGE_PLUGIN_CODE")
    file_path = os.environ.get("BRIDGE_FILE_PATH")
    job_id = int(os.environ.get("BRIDGE_JOB_ID", "0"))
    file_id = int(os.environ.get("BRIDGE_FILE_ID", "0"))

    # Handle different loader modes
    source_code = None

    if args.parser_path:
        # Dev mode: load from file path
        try:
            with open(args.parser_path, 'r') as f:
                source_code = f.read()
        except Exception as e:
            logger.error(f"Failed to read parser file: {e}")
            sys.exit(1)
    elif args.parser_archive:
        # Prod mode: base64-encoded ZIP archive
        try:
            import zipfile
            import tempfile
            archive_bytes = base64.b64decode(args.parser_archive)
            with tempfile.TemporaryDirectory() as tmpdir:
                with zipfile.ZipFile(BytesIO(archive_bytes)) as zf:
                    zf.extractall(tmpdir)
                    # Look for main parser file
                    parser_file = Path(tmpdir) / "parser.py"
                    if not parser_file.exists():
                        # Try to find any .py file
                        py_files = list(Path(tmpdir).glob("*.py"))
                        if py_files:
                            parser_file = py_files[0]
                        else:
                            raise FileNotFoundError("No parser.py found in archive")
                    source_code = parser_file.read_text()
        except Exception as e:
            logger.error(f"Failed to extract parser archive: {e}")
            sys.exit(1)
    elif plugin_code_b64:
        # Inline source: base64-encoded source code from env
        try:
            source_code = base64.b64decode(plugin_code_b64).decode("utf-8")
        except Exception as e:
            logger.error(f"Failed to decode plugin code: {e}")
            sys.exit(1)

    # Validate required configuration
    try:
        port = int(port_str)
    except ValueError:
        logger.error(f"BRIDGE_PORT must be an integer, got: {port_str}")
        sys.exit(1)

    if port == 0:
        logger.error("BRIDGE_PORT environment variable not set or is 0")
        sys.exit(1)

    if not file_path:
        logger.error("BRIDGE_FILE_PATH environment variable not set")
        sys.exit(1)

    if source_code is None:
        logger.error(
            "No parser source provided. Use --parser-path, --parser-archive, "
            "or set BRIDGE_PLUGIN_CODE environment variable"
        )
        sys.exit(1)

    stdio_mode = os.environ.get("BRIDGE_STDIO_MODE", "piped").lower()
    inherit_stdio = stdio_mode in ("inherit", "tty")

    # Create context and connect
    context = BridgeContext(port, job_id)

    # Save original stdio for restoration
    original_stdout = sys.stdout
    original_stderr = sys.stderr

    try:
        context.connect()

        redirected = False
        if not inherit_stdio:
            # === SIDEBAND LOGGING SETUP ===
            # Hijack stdout/stderr BEFORE executing user code to capture print() statements

            # Replace with socket writers
            sys.stdout = SocketWriter(context, LOG_LEVEL_STDOUT)
            sys.stderr = SocketWriter(context, LOG_LEVEL_STDERR)
            redirected = True

            # Route logging through sideband channel
            bridge_handler = BridgeLogHandler(context)
            root_logger = logging.getLogger()
            # Remove default handlers
            for handler in root_logger.handlers[:]:
                root_logger.removeHandler(handler)
            root_logger.addHandler(bridge_handler)

        try:
            # Execute plugin
            metrics = execute_plugin(source_code, file_path, file_id, context)

            # Flush any remaining buffered output
            sys.stdout.flush()
            sys.stderr.flush()

            # Log success through sideband
            logger.info(f"Plugin execution completed: {metrics}")

            # Send completion
            context.send_metrics(metrics)
            context.close()

            if not inherit_stdio:
                # Restore stdout for final JSON output to host process
                sys.stdout = original_stdout
                print(json.dumps(metrics))
            sys.exit(0)

        finally:
            # Always restore stdio in case of exception
            if redirected:
                sys.stdout = original_stdout
                sys.stderr = original_stderr

    except PermanentError as e:
        logger.error(f"Permanent error (no retry): {e}")
        # Send error to host if possible
        try:
            context.send_error(str(e), retryable=False, kind="permanent")
        except Exception:
            pass
        try:
            context.close()
        except Exception:
            pass
        if not inherit_stdio:
            # Print error metrics
            print(json.dumps({
                "status": "FAILED",
                "error": str(e),
                "retryable": False,
            }))
        sys.exit(1)  # Permanent - no retry

    except TransientError as e:
        logger.error(f"Transient error (retry eligible): {e}")
        try:
            context.send_error(str(e), retryable=True, kind="transient")
        except Exception:
            pass
        try:
            context.close()
        except Exception:
            pass
        if not inherit_stdio:
            print(json.dumps({
                "status": "FAILED",
                "error": str(e),
                "retryable": True,
            }))
        sys.exit(2)  # Transient - retry eligible

    except MemoryError as e:
        logger.error(f"Memory error: {e}")
        try:
            context.send_error(f"Memory error: {e}", retryable=True, kind="transient")
        except Exception:
            pass
        try:
            context.close()
        except Exception:
            pass
        if not inherit_stdio:
            print(json.dumps({
                "status": "FAILED",
                "error": str(e),
                "retryable": True,
            }))
        sys.exit(2)  # Transient - retry eligible

    except Exception as e:
        error_msg = traceback.format_exc()

        # Try to send through sideband if socket still connected
        if context._socket:
            try:
                context.send_log(LOG_LEVEL_ERROR, f"Plugin execution failed: {e}\n{error_msg}")
            except Exception:
                pass

        # Send error to Host via error signal
        try:
            context.send_error(str(e), retryable=False, kind="permanent")
        except Exception:
            pass
        try:
            context.close()
        except Exception:
            pass

        if not inherit_stdio:
            # Print error metrics to original stdout
            print(json.dumps({
                "status": "FAILED",
                "error": str(e),
                "retryable": False,
            }))
        sys.exit(1)  # Assume permanent for unknown errors


if __name__ == "__main__":
    main()
