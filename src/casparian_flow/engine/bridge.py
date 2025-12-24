# src/casparian_flow/engine/bridge.py
"""
v5.0 Bridge Mode: Host-Side Execution Bridge.

This module implements the "Host" side of the Host/Guest privilege separation:
- Creates AF_UNIX socket for IPC
- Spawns Guest process in isolated venv
- Reads Arrow IPC batches from socket
- Writes to configured sinks

Host Side Role:
- Holds AWS credentials, DB passwords, heavy drivers (pyodbc)
- Reads Arrow IPC from socket
- Writes to Sinks (Parquet, SQL, etc.)
- Manages subprocess lifecycle

Security Model:
- Guest has no access to credentials (passed via env to sinks)
- Data flows as binary Arrow IPC (efficient, typed)
- Clean privilege separation between Infrastructure and Logic
"""

import os
import sys
import json
import base64
import socket
import struct
import tempfile
import subprocess
import logging
from pathlib import Path
from typing import Optional, Iterator, List
from io import BytesIO

logger = logging.getLogger(__name__)

try:
    import pyarrow as pa
    from pyarrow import ipc
    HAS_PYARROW = True
except ImportError:
    HAS_PYARROW = False
    logger.warning("pyarrow not installed. Bridge Mode unavailable.")


# Arrow IPC Protocol constants
HEADER_FORMAT = "!I"
HEADER_SIZE = 4
END_OF_STREAM = 0
ERROR_SIGNAL = 0xFFFFFFFF


class BridgeError(Exception):
    """Raised when bridge execution fails."""
    pass


class BridgeExecutor:
    """
    Executes plugins in isolated venvs via subprocess bridge.

    Usage:
        executor = BridgeExecutor(
            interpreter_path="/path/to/venv/bin/python",
            source_code="...",
            file_path="/data/input.csv",
            job_id=123,
        )

        for batch in executor.execute():
            sink.write(batch)
    """

    def __init__(
        self,
        interpreter_path: Path,
        source_code: str,
        file_path: str,
        job_id: int,
        file_version_id: int,
        timeout_seconds: int = 300,
    ):
        """
        Initialize the bridge executor.

        Args:
            interpreter_path: Path to Python interpreter in isolated venv
            source_code: Plugin source code
            file_path: Path to input file
            job_id: Job ID for tracking
            file_version_id: File version ID for lineage tracking
            timeout_seconds: Execution timeout
        """
        self.interpreter_path = Path(interpreter_path)
        self.source_code = source_code
        self.file_path = file_path
        self.job_id = job_id
        self.file_version_id = file_version_id
        self.timeout_seconds = timeout_seconds

        # Socket and process handles
        self._socket_path: Optional[str] = None
        self._server_socket: Optional[socket.socket] = None
        self._client_socket: Optional[socket.socket] = None
        self._process: Optional[subprocess.Popen] = None

        # Metrics
        self._total_rows = 0
        self._total_bytes = 0

    def execute(self) -> Iterator[pa.Table]:
        """
        Execute the plugin and yield Arrow Tables.

        This is a generator that:
        1. Creates socket and spawns subprocess
        2. Reads Arrow IPC batches from socket
        3. Yields pa.Table for each batch
        4. Cleans up on completion or error
        """
        if not HAS_PYARROW:
            raise BridgeError("pyarrow required for Bridge Mode")

        try:
            # Setup socket
            self._create_socket()

            # Spawn guest process
            self._spawn_guest()

            # Accept connection from guest
            self._accept_connection()

            # Stream Arrow batches
            yield from self._stream_batches()

        finally:
            self._cleanup()

    def _create_socket(self):
        """Create AF_UNIX socket for IPC."""
        # Create socket in temp directory
        self._socket_path = tempfile.mktemp(prefix="bridge_", suffix=".sock")

        self._server_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._server_socket.bind(self._socket_path)
        self._server_socket.listen(1)
        self._server_socket.settimeout(30)  # Connection timeout

        logger.debug(f"Bridge socket created: {self._socket_path}")

    def _spawn_guest(self):
        """Spawn the guest process in isolated venv."""
        # Find bridge_shim.py
        shim_path = Path(__file__).parent / "bridge_shim.py"

        if not shim_path.exists():
            raise BridgeError(f"Bridge shim not found: {shim_path}")

        # Encode plugin source code
        source_b64 = base64.b64encode(self.source_code.encode("utf-8")).decode("ascii")

        # Environment for guest process
        env = os.environ.copy()
        env.update({
            "BRIDGE_SOCKET": self._socket_path,
            "BRIDGE_PLUGIN_CODE": source_b64,
            "BRIDGE_FILE_PATH": self.file_path,
            "BRIDGE_JOB_ID": str(self.job_id),
            "BRIDGE_FILE_VERSION_ID": str(self.file_version_id),
        })

        # Spawn subprocess
        self._process = subprocess.Popen(
            [str(self.interpreter_path), str(shim_path)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        logger.info(
            f"Spawned guest process (pid={self._process.pid}) "
            f"with interpreter {self.interpreter_path}"
        )

    def _accept_connection(self):
        """Accept connection from guest process."""
        try:
            self._client_socket, _ = self._server_socket.accept()
            self._client_socket.settimeout(self.timeout_seconds)
            logger.debug("Guest process connected to bridge socket")
        except socket.timeout:
            raise BridgeError("Guest process failed to connect (timeout)")

    def _stream_batches(self) -> Iterator[pa.Table]:
        """Stream Arrow IPC batches from guest process."""
        while True:
            # Read header
            header_data = self._recv_exact(HEADER_SIZE)
            if not header_data:
                raise BridgeError("Connection closed unexpectedly")

            length = struct.unpack(HEADER_FORMAT, header_data)[0]

            # End of stream
            if length == END_OF_STREAM:
                logger.debug("Received end-of-stream signal")
                break

            # Error signal
            if length == ERROR_SIGNAL:
                error_length_data = self._recv_exact(HEADER_SIZE)
                error_length = struct.unpack(HEADER_FORMAT, error_length_data)[0]
                error_message = self._recv_exact(error_length).decode("utf-8")
                raise BridgeError(f"Guest process error: {error_message}")

            # Read Arrow IPC data
            ipc_data = self._recv_exact(length)
            self._total_bytes += length

            # Parse Arrow IPC
            try:
                reader = ipc.open_stream(BytesIO(ipc_data))
                table = reader.read_all()
                self._total_rows += table.num_rows

                logger.debug(f"Received batch: {table.num_rows} rows, {length} bytes")
                yield table

            except Exception as e:
                raise BridgeError(f"Failed to parse Arrow IPC: {e}")

    def _recv_exact(self, size: int) -> bytes:
        """Receive exactly `size` bytes from socket."""
        data = bytearray()
        while len(data) < size:
            chunk = self._client_socket.recv(size - len(data))
            if not chunk:
                if len(data) == 0:
                    return b""
                raise BridgeError("Connection closed mid-message")
            data.extend(chunk)
        return bytes(data)

    def _cleanup(self):
        """Clean up socket and process resources."""
        # Close client socket
        if self._client_socket:
            try:
                self._client_socket.close()
            except Exception:
                pass

        # Close server socket
        if self._server_socket:
            try:
                self._server_socket.close()
            except Exception:
                pass

        # Remove socket file
        if self._socket_path and os.path.exists(self._socket_path):
            try:
                os.unlink(self._socket_path)
            except Exception:
                pass

        # Wait for process
        if self._process:
            try:
                stdout, stderr = self._process.communicate(timeout=5)
                if stderr:
                    logger.debug(f"Guest stderr: {stderr.decode('utf-8', errors='replace')}")
                if stdout:
                    # Try to parse metrics from stdout
                    try:
                        metrics = json.loads(stdout.decode("utf-8"))
                        logger.debug(f"Guest metrics: {metrics}")
                    except json.JSONDecodeError:
                        pass
            except subprocess.TimeoutExpired:
                self._process.kill()
                logger.warning("Force-killed guest process (timeout)")

    def get_metrics(self) -> dict:
        """Get execution metrics."""
        return {
            "total_rows": self._total_rows,
            "total_bytes": self._total_bytes,
        }


def execute_bridge_job(
    interpreter_path: Path,
    source_code: str,
    file_path: str,
    job_id: int,
    sinks: list,
    timeout_seconds: int = 300,
) -> dict:
    """
    High-level function to execute a job in Bridge Mode.

    Args:
        interpreter_path: Path to Python interpreter in isolated venv
        source_code: Plugin source code
        file_path: Path to input file
        job_id: Job ID
        sinks: List of DataSink instances to write to
        timeout_seconds: Execution timeout

    Returns:
        dict with execution metrics
    """
    executor = BridgeExecutor(
        interpreter_path=interpreter_path,
        source_code=source_code,
        file_path=file_path,
        job_id=job_id,
        timeout_seconds=timeout_seconds,
    )

    try:
        for table in executor.execute():
            # Convert Arrow Table to pandas for sink compatibility
            df = table.to_pandas()
            for sink in sinks:
                sink.write(df)

        # Promote all sinks
        for sink in sinks:
            sink.promote()

        metrics = executor.get_metrics()
        metrics["status"] = "SUCCESS"
        return metrics

    except Exception as e:
        logger.error(f"Bridge execution failed: {e}")
        return {
            "status": "FAILED",
            "error": str(e),
            **executor.get_metrics(),
        }
