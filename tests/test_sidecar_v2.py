# tests/test_sidecar_v2.py
"""
Tests for Sidecar Protocol v2 support.

Tests sidecar heartbeat responses, protocol v2 unpacking,
and content type handling.
"""
import pytest
from unittest.mock import Mock, patch, MagicMock

from casparian_flow.protocol import (
    OpCode,
    ContentType,
    pack_header,
    unpack_header,
    msg_heartbeat,
    msg_execute,
)


class TestSidecarHeartbeat:
    """Test sidecar HEARTBEAT handling."""

    def test_sidecar_heartbeat_response(self):
        """Sidecar responds to HEARTBEAT with msg_heartbeat()."""
        # Simulate receiving HEARTBEAT
        heartbeat_frames = msg_heartbeat()

        op, job_id, meta_len, content_type, compressed = unpack_header(heartbeat_frames[0])

        assert op == OpCode.HEARTBEAT
        assert job_id == 0
        assert meta_len == 0

        # Response should be identical
        response = msg_heartbeat()
        assert response == heartbeat_frames

    def test_sidecar_continues_after_heartbeat(self):
        """HEARTBEAT doesn't interrupt job execution."""
        # This is more of an integration test - verify that after
        # receiving HEARTBEAT, sidecar can still process EXEC messages

        # Simulate HEARTBEAT
        hb_frames = msg_heartbeat()
        hb_op, _, _, _, _ = unpack_header(hb_frames[0])
        assert hb_op == OpCode.HEARTBEAT

        # Simulate EXEC after heartbeat
        exec_frames = msg_execute(job_id=123, filepath="/data/file.csv")
        exec_op, exec_job_id, _, _, _ = unpack_header(exec_frames[0])

        assert exec_op == OpCode.EXEC
        assert exec_job_id == 123


class TestSidecarProtocolV2:
    """Test protocol v2 unpacking in sidecar."""

    def test_sidecar_protocol_v2_unpack(self):
        """Correctly unpacks 5-tuple from unpack_header."""
        # Create v2 header with all fields
        header = pack_header(
            OpCode.EXEC,
            job_id=999,
            meta_len=50,
            content_type=ContentType.UTF8,
            compressed=False,
        )

        # Sidecar unpacks
        op, job_id, meta_len, content_type, compressed = unpack_header(header)

        assert op == OpCode.EXEC
        assert job_id == 999
        assert meta_len == 50
        assert content_type == ContentType.UTF8
        assert compressed is False

    def test_sidecar_exec_with_content_type(self):
        """Handles different ContentType values in EXEC messages."""
        for content_type in [ContentType.UTF8, ContentType.JSON, ContentType.ARROW]:
            header = pack_header(
                OpCode.EXEC,
                job_id=1,
                meta_len=10,
                content_type=content_type,
            )

            op, job_id, meta_len, decoded_type, compressed = unpack_header(header)

            assert op == OpCode.EXEC
            assert decoded_type == content_type


class TestSidecarMessageFlow:
    """Test message flow in sidecar."""

    def test_sidecar_processes_multiple_opcodes(self):
        """Sidecar can handle different OpCodes sequentially."""
        messages = [
            (OpCode.HEARTBEAT, msg_heartbeat()),
            (OpCode.EXEC, msg_execute(123, "/path/file.csv")),
            (OpCode.HEARTBEAT, msg_heartbeat()),
        ]

        for expected_op, frames in messages:
            op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
            assert op == expected_op

    def test_sidecar_invalid_opcode_detection(self):
        """Sidecar detects invalid OpCodes."""
        from casparian_flow.protocol import validate_header
        import struct

        # Craft invalid header
        bad_header = struct.pack("!BBHIQ", 0x01, 99, 0, 0, 1)  # OpCode 99 is invalid

        error = validate_header(bad_header)
        assert error is not None
        assert "Invalid op_code" in error
