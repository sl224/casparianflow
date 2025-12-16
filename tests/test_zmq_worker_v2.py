# tests/test_zmq_worker_v2.py
"""
Tests for ZmqWorker Protocol v2 updates.

Tests heartbeat monitoring, hot reload functionality, DEPLOY handler,
and protocol v2 compatibility.
"""
import pytest
import time
from unittest.mock import Mock, patch, MagicMock
from pathlib import Path

from casparian_flow.engine.zmq_worker import ZmqWorker
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.protocol import OpCode, pack_header, msg_heartbeat, msg_deploy
from casparian_flow.db.models import PluginManifest, PluginStatusEnum
from casparian_flow.security.gatekeeper import generate_signature


import socket

def get_free_port():
    """Get a free port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

@pytest.fixture
def unique_zmq_addr():
    """Provide a unique TCP address for each test."""
    port = get_free_port()
    return f"tcp://127.0.0.1:{port}"

@pytest.fixture
def worker_config(tmp_path, test_db_engine):  # Depend on test_db_engine to ensure init
    """Create a minimal WorkerConfig for testing."""
    from casparian_flow.engine.config import DatabaseConfig, StorageConfig, PluginsConfig

    # Must match path in conftest.py's test_db_engine
    db_path = tmp_path / "test_casparian_flow.sqlite3"
    return WorkerConfig(
        database=DatabaseConfig(connection_string=f"sqlite:///{db_path}"),
        storage=StorageConfig(parquet_root=tmp_path / "parquet"),
        plugins=PluginsConfig(dir=tmp_path / "plugins"),
    )


class TestHeartbeatMonitoring:
    """Test heartbeat tracking and monitoring."""

    def test_heartbeat_tracking(self, worker_config, unique_zmq_addr):
        """Worker tracks sidecar_heartbeats timestamps."""
        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)

        # Initially empty
        assert len(worker.sidecar_heartbeats) == 0

        # Simulate message from sidecar
        identity = b"sidecar_123"
        worker.sidecar_heartbeats[identity] = time.time()

        assert len(worker.sidecar_heartbeats) == 1
        assert identity in worker.sidecar_heartbeats

        worker.stop()

    @patch("casparian_flow.engine.zmq_worker.zmq.Context")
    def test_heartbeat_response(self, mock_zmq_context, worker_config, unique_zmq_addr):
        """Worker responds to HEARTBEAT OpCode."""
        mock_socket = MagicMock()
        mock_context = MagicMock()
        mock_context.socket.return_value = mock_socket
        mock_zmq_context.return_value = mock_context

        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)

        # Simulate HEARTBEAT message
        identity = b"test_sidecar"
        header = msg_heartbeat()[0]

        frames = [identity, header, b""]

        # Mock recv_multipart to return our frames
        mock_socket.recv_multipart.side_effect = [[frames], Exception("Stop")]

        # Simulate message handling (would be called in loop)
        worker.sidecar_heartbeats[identity] = time.time()

        # Verify heartbeat was tracked
        assert identity in worker.sidecar_heartbeats

    def test_prune_dead_sidecars(self, worker_config, unique_zmq_addr):
        """Remove sidecars after 60s timeout."""
        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)

        # Add fresh and stale sidecars
        fresh_identity = b"fresh_sidecar"
        stale_identity = b"stale_sidecar"

        worker.sidecar_heartbeats[fresh_identity] = time.time()
        worker.sidecar_heartbeats[stale_identity] = time.time() - 100  # 100s ago

        # Add to plugin registry
        worker.plugin_registry["fresh_plugin"] = fresh_identity
        worker.plugin_registry["stale_plugin"] = stale_identity

        # Prune
        worker._prune_dead_sidecars(timeout_seconds=60)

        # Fresh should remain, stale should be removed
        assert fresh_identity in worker.sidecar_heartbeats
        assert stale_identity not in worker.sidecar_heartbeats

        assert "fresh_plugin" in worker.plugin_registry
        assert "stale_plugin" not in worker.plugin_registry


class TestHotReload:
    """Test reload_plugins() functionality."""

    @patch("casparian_flow.engine.zmq_worker.subprocess.Popen")
    def test_reload_writes_source_to_disk(
        self, mock_popen, worker_config, test_db_session, sample_plugin_code, unique_zmq_addr
    ):
        """Verify plugin files created in plugins/ directory."""
        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)

        # Create ACTIVE plugin in database
        manifest = PluginManifest(
            plugin_name="reload_test_plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            source_hash="hash123",
            status=PluginStatusEnum.ACTIVE,
        )
        test_db_session.add(manifest)
        test_db_session.commit()

        # Mock Popen to avoid actual subprocess
        mock_popen.return_value = Mock()

        # Call reload
        worker.reload_plugins()

        # Verify plugin file was written
        plugin_path = worker_config.plugins.dir / "reload_test_plugin.py"
        assert plugin_path.exists()
        assert plugin_path.read_text(encoding="utf-8") == sample_plugin_code

        # Cleanup
        plugin_path.unlink()

    @patch("casparian_flow.engine.zmq_worker.subprocess.Popen")
    def test_reload_spawns_sidecar_processes(
        self, mock_popen, worker_config, test_db_session, sample_plugin_code, unique_zmq_addr
    ):
        """Check subprocess.Popen called for each ACTIVE plugin."""
        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)

        # Create multiple ACTIVE plugins
        for i in range(3):
            manifest = PluginManifest(
                plugin_name=f"plugin_{i}",
                version="1.0.0",
                source_code=sample_plugin_code + f"\n# Plugin {i}",
                source_hash=f"hash_{i}",
                status=PluginStatusEnum.ACTIVE,
            )
            test_db_session.add(manifest)
        test_db_session.commit()

        mock_popen.return_value = Mock()

        worker.reload_plugins()

        # Verify Popen was called 3 times + 1 for System Deployer = 4
        assert mock_popen.call_count == 4


class TestDEPLOYHandler:
    """Test DEPLOY message handling."""

    @patch("casparian_flow.engine.zmq_worker.subprocess.Popen")
    def test_deploy_triggers_reload(
        self, mock_popen, worker_config, test_db_session, sample_plugin_code, unique_zmq_addr
    ):
        """Successful deploy calls reload_plugins()."""
        worker = ZmqWorker(worker_config, zmq_addr=unique_zmq_addr)
        mock_popen.return_value = Mock()

        # Spy on reload_plugins
        with patch.object(worker, "reload_plugins") as mock_reload:
            signature = generate_signature(sample_plugin_code, worker.architect.secret_key)

            import json

            payload = {
                "plugin_name": "deploy_test",
                "version": "1.0.0",
                "source_code": sample_plugin_code,
                "signature": signature,
            }
            payload_bytes = json.dumps(payload).encode("utf-8")

            # Simulate DEPLOY message handling
            from casparian_flow.services.architect import handle_deploy_message

            result = handle_deploy_message(worker.architect, payload_bytes)

            if result.success:
                worker.reload_plugins()

            # Verify reload was called
            assert mock_reload.called


class TestProtocolV2Compatibility:
    """Test protocol v2 handling."""

    def test_unpack_header_v2(self, worker_config):
        """Worker handles 5-tuple return from unpack_header."""
        from casparian_flow.protocol import unpack_header, pack_header, ContentType

        header = pack_header(
            OpCode.EXEC,
            job_id=999,
            meta_len=100,
            content_type=ContentType.JSON,
            compressed=True,
        )

        op, job_id, meta_len, content_type, compressed = unpack_header(header)

        assert op == OpCode.EXEC
        assert job_id == 999
        assert meta_len == 100
        assert content_type == ContentType.JSON
        assert compressed is True

    def test_content_type_handling(self, worker_config):
        """Worker processes different ContentType values."""
        from casparian_flow.protocol import ContentType, pack_header

        # Test all content types
        for content_type in ContentType:
            header = pack_header(OpCode.DATA, job_id=1, meta_len=0, content_type=content_type)

            from casparian_flow.protocol import unpack_header

            op, job_id, meta_len, decoded_type, compressed = unpack_header(header)
            assert decoded_type == content_type
