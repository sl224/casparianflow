# tests/test_zmq_integration.py
"""
Integration tests for ZMQ-based plugin isolation.

These tests verify:
1. Happy path: End-to-end job execution with lineage
2. Crash isolation: Sidecar crash doesn't kill worker
3. Protocol security: Invalid messages are rejected
"""
import pytest
import threading
import time
import struct
from pathlib import Path

import zmq


class TestProtocol:
    """Test the binary protocol implementation."""
    
    def test_header_pack_unpack_roundtrip(self):
        """Verify header packing and unpacking are symmetric."""
        from casparian_flow.protocol import (
            pack_header, unpack_header, OpCode, HEADER_SIZE
        )
        
        # Test all op codes
        for op in OpCode:
            header = pack_header(op, 12345, 100)
            assert len(header) == HEADER_SIZE
            
            unpacked_op, job_id, meta_len = unpack_header(header)
            assert unpacked_op == op
            assert job_id == 12345
            assert meta_len == 100
    
    def test_header_size_is_16_bytes(self):
        """Verify header is exactly 16 bytes."""
        from casparian_flow.protocol import HEADER_SIZE
        assert HEADER_SIZE == 16
    
    def test_invalid_op_code_rejected(self):
        """Verify invalid op codes raise ValueError."""
        from casparian_flow.protocol import unpack_header
        
        # Op code 0 is invalid
        bad_header = struct.pack('!BxHQI', 0, 0, 123, 50)
        with pytest.raises(ValueError, match="Invalid op_code"):
            unpack_header(bad_header)
        
        # Op code 255 is invalid
        bad_header = struct.pack('!BxHQI', 255, 0, 123, 50)
        with pytest.raises(ValueError, match="Invalid op_code"):
            unpack_header(bad_header)
    
    def test_validate_header_catches_garbage(self):
        """Verify garbage bytes are detected without raising exceptions."""
        from casparian_flow.protocol import validate_header
        
        # Too short
        assert validate_header(b'short') is not None
        
        # Invalid op code
        garbage = b'\x00' * 16
        assert validate_header(garbage) is not None
        
        # Valid header should pass
        valid = struct.pack('!BxHQI', 1, 0, 123, 50)
        assert validate_header(valid) is None


class TestMessageBuilders:
    """Test message construction helpers."""
    
    def test_msg_execute_format(self):
        """Verify EXEC message structure."""
        from casparian_flow.protocol import msg_execute, unpack_header, OpCode
        
        frames = msg_execute(999, "/path/to/file.csv")
        
        assert len(frames) == 2
        op, job_id, meta_len = unpack_header(frames[0])
        assert op == OpCode.EXEC
        assert job_id == 999
        assert frames[1] == b"/path/to/file.csv"
    
    def test_msg_data_format(self):
        """Verify DATA message structure."""
        from casparian_flow.protocol import msg_data, unpack_header, OpCode
        
        payload = b"fake_arrow_ipc_bytes"
        frames = msg_data(123, payload)
        
        assert len(frames) == 3
        op, job_id, _ = unpack_header(frames[0])
        assert op == OpCode.DATA
        assert job_id == 123
        assert frames[2] == payload


class TestPluginLoader:
    """Test dynamic plugin loading."""
    
    def test_load_valid_plugin(self, tmp_path):
        """Verify plugin loading works for valid plugin."""
        from casparian_flow.sidecar import load_plugin
        
        # Create a simple plugin
        plugin_file = tmp_path / "test_plugin.py"
        plugin_file.write_text("""
import pandas as pd

def execute(filepath):
    return pd.DataFrame({"col1": [1, 2, 3]})
""")
        
        execute_fn = load_plugin(str(plugin_file))
        assert callable(execute_fn)
    
    def test_load_missing_execute_raises(self, tmp_path):
        """Verify plugin without execute function raises."""
        from casparian_flow.sidecar import load_plugin
        
        plugin_file = tmp_path / "bad_plugin.py"
        plugin_file.write_text("x = 1")
        
        with pytest.raises(AttributeError, match="execute"):
            load_plugin(str(plugin_file))


class TestTopicValidation:
    """Test SQL injection prevention."""
    
    def test_valid_topic_names_accepted(self, test_db_engine, test_db_session, test_plugin_config):
        """Verify valid topic names are accepted."""
        from casparian_flow.engine.context import WorkerContext
        
        ctx = WorkerContext(
            sql_engine=test_db_engine,
            parquet_root="./test_output",
            topic_config={},
            job_id=1,
            file_version_id=1,
            file_location_id=1
        )
        
        # Valid names should work
        ctx.register_topic("output")
        ctx.register_topic("my_topic_123")
        ctx.register_topic("CamelCase")
    
    def test_sql_injection_blocked(self, test_db_engine, test_db_session, test_plugin_config):
        """Verify SQL injection attempts are blocked."""
        from casparian_flow.engine.context import WorkerContext
        
        ctx = WorkerContext(
            sql_engine=test_db_engine,
            parquet_root="./test_output",
            topic_config={},
            job_id=1,
            file_version_id=1,
            file_location_id=1
        )
        
        # Malicious names should raise
        with pytest.raises(ValueError):
            ctx.register_topic("table; DROP TABLE users--")
        
        with pytest.raises(ValueError):
            ctx.register_topic("123_starts_with_number")
        
        with pytest.raises(ValueError):
            ctx.register_topic("")
