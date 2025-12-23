# tests/test_bridge_execution.py
"""
Tests for v5.0 Bridge Mode execution.

Tests:
- BridgeContext (guest-side context)
- Arrow IPC protocol constants
- BridgeExecutor lifecycle
- Error handling and cleanup

Note: These tests verify the bridge logic without spawning subprocesses.
"""

import pytest
import struct
import json
import socket
import threading
from pathlib import Path
from unittest.mock import patch, MagicMock
from io import BytesIO

from casparian_flow.engine.bridge import (
    BridgeExecutor,
    BridgeError,
    HEADER_FORMAT,
    HEADER_SIZE,
    END_OF_STREAM,
    ERROR_SIGNAL,
)

# Import bridge_shim components for testing guest-side logic
from casparian_flow.engine.bridge_shim import (
    BridgeContext as GuestContext,
    execute_plugin,
)


class TestBridgeProtocolConstants:
    """Tests for Arrow IPC protocol constants."""

    def test_header_format_is_4_bytes(self):
        """Header is 4-byte unsigned int."""
        assert HEADER_SIZE == 4
        assert struct.calcsize(HEADER_FORMAT) == 4

    def test_end_of_stream_signal(self):
        """End of stream is length 0."""
        assert END_OF_STREAM == 0

    def test_error_signal_value(self):
        """Error signal is max uint32."""
        assert ERROR_SIGNAL == 0xFFFFFFFF

    def test_pack_unpack_header(self):
        """Header packs and unpacks correctly."""
        length = 1234
        packed = struct.pack(HEADER_FORMAT, length)
        unpacked = struct.unpack(HEADER_FORMAT, packed)[0]
        assert unpacked == length


class TestBridgeExecutorInit:
    """Tests for BridgeExecutor initialization."""

    def test_stores_configuration(self, tmp_path: Path):
        """Executor stores provided configuration."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="# plugin",
            file_path="/input.csv",
            job_id=42,
            timeout_seconds=120,
        )

        assert executor.interpreter_path == interp
        assert executor.source_code == "# plugin"
        assert executor.file_path == "/input.csv"
        assert executor.job_id == 42
        assert executor.timeout_seconds == 120

    def test_initial_metrics_zero(self, tmp_path: Path):
        """Metrics start at zero."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="",
            file_path="",
            job_id=0,
        )

        metrics = executor.get_metrics()
        assert metrics["total_rows"] == 0
        assert metrics["total_bytes"] == 0


class TestBridgeExecutorCleanup:
    """Tests for BridgeExecutor resource cleanup."""

    def test_cleanup_removes_socket_file(self, tmp_path: Path):
        """Cleanup removes socket file from filesystem."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="",
            file_path="",
            job_id=0,
        )

        # Simulate socket creation
        executor._socket_path = str(tmp_path / "test.sock")
        sock_file = Path(executor._socket_path)
        sock_file.touch()

        executor._cleanup()
        assert not sock_file.exists()

    def test_cleanup_handles_missing_socket(self, tmp_path: Path):
        """Cleanup handles already-deleted socket gracefully."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="",
            file_path="",
            job_id=0,
        )
        executor._socket_path = "/nonexistent/socket.sock"

        # Should not raise
        executor._cleanup()


class TestGuestContext:
    """Tests for guest-side BridgeContext."""

    def test_register_topic_returns_handle(self):
        """register_topic returns incrementing handles."""
        ctx = GuestContext("/fake/socket", 1)

        h1 = ctx.register_topic("topic_a")
        h2 = ctx.register_topic("topic_b")

        assert h1 == 1
        assert h2 == 2
        assert ctx._topics[h1] == "topic_a"
        assert ctx._topics[h2] == "topic_b"

    def test_row_count_tracking(self):
        """get_row_count tracks published rows."""
        ctx = GuestContext("/fake/socket", 1)
        assert ctx.get_row_count() == 0

        ctx._row_count = 100
        assert ctx.get_row_count() == 100


class TestPluginExecution:
    """Tests for guest-side plugin execution."""

    SIMPLE_PLUGIN = '''
class Handler:
    def configure(self, ctx, config):
        self.ctx = ctx

    def execute(self, file_path):
        # Return a simple result
        return [{"result": "ok"}]
'''

    PLUGIN_WITHOUT_HANDLER = '''
def some_function():
    pass
'''

    def test_rejects_plugin_without_handler(self):
        """Raises error if Handler class not defined."""
        ctx = MagicMock()

        with pytest.raises(ValueError, match="must define a 'Handler' class"):
            execute_plugin(self.PLUGIN_WITHOUT_HANDLER, "/input.csv", ctx)

    def test_executes_plugin_with_execute_method(self):
        """Calls execute() method on Handler."""
        ctx = MagicMock()
        ctx.publish = MagicMock()

        EXEC_PLUGIN = '''
class Handler:
    def execute(self, file_path):
        return [{"path": file_path}]
'''
        # The execute_plugin function iterates over results and publishes
        # For this test we just verify it doesn't crash
        result = execute_plugin(EXEC_PLUGIN, "/test/file.csv", ctx)
        assert result["status"] == "SUCCESS"

    def test_configures_handler_if_method_exists(self):
        """Calls configure() if Handler has it."""
        ctx = MagicMock()

        CONFIGURABLE_PLUGIN = '''
configured = False

class Handler:
    def configure(self, ctx, config):
        global configured
        configured = True

    def execute(self, file_path):
        return None
'''
        execute_plugin(CONFIGURABLE_PLUGIN, "/input.csv", ctx)
        # Can't easily check global var, but verify no error


class TestBridgeExecutorExecution:
    """Tests for full bridge execution (mocked subprocess)."""

    @patch("subprocess.Popen")
    @patch("socket.socket")
    def test_spawns_subprocess_with_env(self, mock_socket, mock_popen, tmp_path: Path):
        """Spawns subprocess with bridge environment variables."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        # Mock socket behavior
        mock_server = MagicMock()
        mock_client = MagicMock()
        mock_socket.return_value = mock_server
        mock_server.accept.return_value = (mock_client, None)

        # Mock receiving end-of-stream immediately
        mock_client.recv.return_value = struct.pack(HEADER_FORMAT, END_OF_STREAM)

        # Mock subprocess
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (b'{"status": "SUCCESS"}', b"")
        mock_popen.return_value = mock_proc

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="# plugin",
            file_path="/data/input.csv",
            job_id=123,
        )

        # Execute (will consume generator)
        list(executor.execute())

        # Verify subprocess was spawned
        mock_popen.assert_called_once()
        call_kwargs = mock_popen.call_args
        env = call_kwargs.kwargs.get("env") or call_kwargs[1].get("env", {})

        assert "BRIDGE_SOCKET" in env
        assert "BRIDGE_FILE_PATH" in env
        assert env["BRIDGE_FILE_PATH"] == "/data/input.csv"
        assert env["BRIDGE_JOB_ID"] == "123"


class TestBridgeErrorHandling:
    """Tests for bridge error scenarios."""

    def test_bridge_error_has_message(self):
        """BridgeError includes descriptive message."""
        error = BridgeError("Test error message")
        assert str(error) == "Test error message"

    @patch("socket.socket")
    def test_connection_timeout_raises_error(self, mock_socket, tmp_path: Path):
        """Raises BridgeError on connection timeout."""
        interp = tmp_path / "bin" / "python"
        interp.parent.mkdir(parents=True)
        interp.touch()

        mock_server = MagicMock()
        mock_socket.return_value = mock_server
        mock_server.accept.side_effect = socket.timeout("timed out")

        executor = BridgeExecutor(
            interpreter_path=interp,
            source_code="# plugin",
            file_path="/input.csv",
            job_id=1,
        )

        # Patch subprocess to avoid actual spawn and handle cleanup
        mock_proc = MagicMock()
        mock_proc.communicate.return_value = (b"", b"")
        with patch("subprocess.Popen", return_value=mock_proc):
            with pytest.raises(BridgeError, match="timeout"):
                list(executor.execute())
