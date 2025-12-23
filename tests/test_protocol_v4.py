# tests/test_protocol_v4.py
"""
Tests for Protocol v4: The Split Plane Protocol.

Tests the simplified JSON-over-ZMQ protocol with OpCodes:
IDENTIFY, DISPATCH, ABORT, HEARTBEAT, CONCLUDE, ERR
"""
import pytest
import json

from casparian_flow.protocol import (
    pack_header,
    unpack_header,
    unpack_msg,
    OpCode,
    PROTOCOL_VERSION,
    HEADER_SIZE,
    SinkConfig,
    DispatchCommand,
    JobReceipt,
    IdentifyPayload,
    HeartbeatPayload,
    ErrorPayload,
    msg_identify,
    msg_dispatch,
    msg_abort,
    msg_heartbeat,
    msg_conclude,
    msg_err,
)


class TestProtocolVersion:
    """Test protocol version is v4."""

    def test_protocol_version(self):
        """Verify protocol version is 0x04."""
        assert PROTOCOL_VERSION == 0x04

    def test_header_size(self):
        """Verify header is 16 bytes."""
        assert HEADER_SIZE == 16


class TestHeaderFormat:
    """Test the 16-byte header format."""

    def test_header_packing(self):
        """Test basic header packing."""
        header = pack_header(OpCode.DISPATCH, job_id=123, payload_len=456)
        assert len(header) == 16
        assert isinstance(header, bytes)

    def test_header_unpacking(self):
        """Test basic header unpacking."""
        header = pack_header(OpCode.DISPATCH, job_id=123, payload_len=456)
        opcode, job_id, payload_len = unpack_header(header)

        assert opcode == OpCode.DISPATCH
        assert job_id == 123
        assert payload_len == 456

    def test_version_field(self):
        """Verify VERSION field is at byte 0."""
        header = pack_header(OpCode.IDENTIFY, job_id=0, payload_len=0)
        version = header[0]
        assert version == PROTOCOL_VERSION

    def test_opcode_field(self):
        """Verify OpCode field is at byte 1."""
        header = pack_header(OpCode.CONCLUDE, job_id=0, payload_len=0)
        opcode = header[1]
        assert opcode == OpCode.CONCLUDE

    def test_version_mismatch_raises(self):
        """Invalid version should raise ValueError."""
        import struct

        bad_header = struct.pack("!BBHIQ", 0x03, OpCode.DISPATCH, 0, 123, 456)
        with pytest.raises(ValueError, match="Version mismatch"):
            unpack_header(bad_header)

    def test_header_too_short_raises(self):
        """Header < 16 bytes should raise ValueError."""
        short_header = b"tooshort"
        with pytest.raises(ValueError, match="Header too short"):
            unpack_header(short_header)


class TestOpCodes:
    """Test all OpCodes for Protocol v4."""

    def test_opcode_values(self):
        """Verify OpCode enum values."""
        assert OpCode.UNKNOWN == 0
        assert OpCode.IDENTIFY == 1
        assert OpCode.DISPATCH == 2
        assert OpCode.ABORT == 3
        assert OpCode.HEARTBEAT == 4
        assert OpCode.CONCLUDE == 5
        assert OpCode.ERR == 6

    def test_all_opcodes_pack(self):
        """Verify all OpCodes can be packed."""
        for opcode in [
            OpCode.IDENTIFY,
            OpCode.DISPATCH,
            OpCode.ABORT,
            OpCode.HEARTBEAT,
            OpCode.CONCLUDE,
            OpCode.ERR,
        ]:
            header = pack_header(opcode, job_id=1, payload_len=0)
            assert len(header) == 16
            assert header[1] == opcode


class TestPydanticModels:
    """Test Pydantic model validation."""

    def test_sink_config(self):
        """Test SinkConfig model."""
        config = SinkConfig(
            topic="output", uri="parquet://output.parquet", mode="append"
        )
        assert config.topic == "output"
        assert config.uri == "parquet://output.parquet"
        assert config.mode == "append"
        assert config.schema_def is None

    def test_sink_config_with_schema(self):
        """Test SinkConfig with schema."""
        schema = '{"type": "struct", "fields": []}'
        config = SinkConfig(
            topic="data",
            uri="sqlite:///db.sqlite/table",
            mode="replace",
            schema_def=schema,
        )
        assert config.schema_def == schema

    def test_dispatch_command(self):
        """Test DispatchCommand model."""
        sinks = [
            SinkConfig(topic="output", uri="parquet://output.parquet", mode="append")
        ]
        cmd = DispatchCommand(
            plugin_name="test_plugin",
            file_path="/path/to/file.csv",
            sinks=sinks,
            file_version_id=42,
        )
        assert cmd.plugin_name == "test_plugin"
        assert cmd.file_path == "/path/to/file.csv"
        assert len(cmd.sinks) == 1
        assert cmd.file_version_id == 42

    def test_job_receipt_success(self):
        """Test JobReceipt for successful job."""
        receipt = JobReceipt(
            status="SUCCESS",
            metrics={"rows": 1000, "size_bytes": 50000},
            artifacts=[{"topic": "output", "uri": "parquet://output.parquet"}],
        )
        assert receipt.status == "SUCCESS"
        assert receipt.metrics["rows"] == 1000
        assert receipt.error_message is None

    def test_job_receipt_failure(self):
        """Test JobReceipt for failed job."""
        receipt = JobReceipt(
            status="FAILED",
            metrics={},
            artifacts=[],
            error_message="File not found",
        )
        assert receipt.status == "FAILED"
        assert receipt.error_message == "File not found"

    def test_identify_payload(self):
        """Test IdentifyPayload model."""
        payload = IdentifyPayload(
            capabilities=["plugin_a", "plugin_b"], worker_id="worker-123"
        )
        assert len(payload.capabilities) == 2
        assert payload.worker_id == "worker-123"

    def test_heartbeat_payload(self):
        """Test HeartbeatPayload model."""
        payload = HeartbeatPayload(status="BUSY", current_job_id=42)
        assert payload.status == "BUSY"
        assert payload.current_job_id == 42

    def test_error_payload(self):
        """Test ErrorPayload model."""
        payload = ErrorPayload(message="Test error", traceback="line1\nline2")
        assert payload.message == "Test error"
        assert payload.traceback == "line1\nline2"


class TestMessageBuilders:
    """Test message builder functions."""

    def test_msg_identify(self):
        """Test msg_identify builder."""
        frames = msg_identify(["plugin_a", "plugin_b"], worker_id="w-123")
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.IDENTIFY
        assert job_id == 0

        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["capabilities"] == ["plugin_a", "plugin_b"]
        assert payload["worker_id"] == "w-123"

    def test_msg_dispatch(self):
        """Test msg_dispatch builder."""
        sinks = [
            SinkConfig(topic="output", uri="parquet://output.parquet", mode="append")
        ]
        frames = msg_dispatch(123, "test_plugin", "/path/to/file.csv", sinks, 99)
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.DISPATCH
        assert job_id == 123

        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["plugin_name"] == "test_plugin"
        assert payload["file_path"] == "/path/to/file.csv"
        assert len(payload["sinks"]) == 1
        assert payload["file_version_id"] == 99

    def test_msg_abort(self):
        """Test msg_abort builder."""
        frames = msg_abort(456)
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.ABORT
        assert job_id == 456
        assert payload_len == 0
        assert frames[1] == b""

    def test_msg_heartbeat(self):
        """Test msg_heartbeat builder."""
        frames = msg_heartbeat(0, status="IDLE")
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.HEARTBEAT
        assert job_id == 0

        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["status"] == "IDLE"

    def test_msg_conclude(self):
        """Test msg_conclude builder."""
        receipt = JobReceipt(
            status="SUCCESS",
            metrics={"rows": 100},
            artifacts=[{"topic": "output", "uri": "parquet://output.parquet"}],
        )
        frames = msg_conclude(789, receipt)
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.CONCLUDE
        assert job_id == 789

        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["status"] == "SUCCESS"
        assert payload["metrics"]["rows"] == 100

    def test_msg_err(self):
        """Test msg_err builder."""
        frames = msg_err(999, "Something went wrong", traceback="line1\nline2")
        assert len(frames) == 2

        opcode, job_id, payload_len = unpack_header(frames[0])
        assert opcode == OpCode.ERR
        assert job_id == 999

        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["message"] == "Something went wrong"
        assert payload["traceback"] == "line1\nline2"


class TestMessageUnpacking:
    """Test unpack_msg function."""

    def test_unpack_identify(self):
        """Test unpacking IDENTIFY message."""
        frames = msg_identify(["plugin_a"], worker_id="w-1")
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.IDENTIFY
        assert job_id == 0
        assert payload_dict["capabilities"] == ["plugin_a"]
        assert payload_dict["worker_id"] == "w-1"

    def test_unpack_dispatch(self):
        """Test unpacking DISPATCH message."""
        sinks = [
            SinkConfig(topic="output", uri="parquet://output.parquet", mode="append")
        ]
        frames = msg_dispatch(123, "test_plugin", "/path/to/file.csv", sinks, 55)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.DISPATCH
        assert job_id == 123
        assert payload_dict["plugin_name"] == "test_plugin"
        assert payload_dict["file_version_id"] == 55

    def test_unpack_conclude(self):
        """Test unpacking CONCLUDE message."""
        receipt = JobReceipt(
            status="SUCCESS", metrics={"rows": 100}, artifacts=[]
        )
        frames = msg_conclude(456, receipt)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.CONCLUDE
        assert job_id == 456
        assert payload_dict["status"] == "SUCCESS"

    def test_unpack_empty_payload(self):
        """Test unpacking message with empty payload (ABORT)."""
        frames = msg_abort(789)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.ABORT
        assert job_id == 789
        assert payload_dict == {}

    def test_unpack_invalid_frames_raises(self):
        """Test that unpack_msg raises on invalid frames."""
        with pytest.raises(ValueError, match="Expected at least 2 frames"):
            unpack_msg([b"single_frame"])

    def test_unpack_invalid_json_raises(self):
        """Test that unpack_msg raises on invalid JSON."""
        header = pack_header(OpCode.DISPATCH, job_id=1, payload_len=10)
        bad_payload = b"not json!!"
        with pytest.raises(ValueError, match="Failed to decode JSON"):
            unpack_msg([header, bad_payload])


class TestRoundtrip:
    """Test pack → unpack roundtrips."""

    def test_identify_roundtrip(self):
        """Test IDENTIFY message roundtrip."""
        original_caps = ["plugin_a", "plugin_b", "plugin_c"]
        frames = msg_identify(original_caps, worker_id="w-123")
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.IDENTIFY
        assert payload_dict["capabilities"] == original_caps
        assert payload_dict["worker_id"] == "w-123"

    def test_dispatch_roundtrip(self):
        """Test DISPATCH message roundtrip."""
        sinks = [
            SinkConfig(topic="output", uri="parquet://output.parquet", mode="append"),
            SinkConfig(topic="errors", uri="sqlite:///db.sqlite/errors", mode="append"),
        ]
        frames = msg_dispatch(999, "csv_parser", "/data/file.csv", sinks, 777)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.DISPATCH
        assert job_id == 999
        assert payload_dict["plugin_name"] == "csv_parser"
        assert payload_dict["file_path"] == "/data/file.csv"
        assert len(payload_dict["sinks"]) == 2
        assert payload_dict["file_version_id"] == 777

    def test_conclude_success_roundtrip(self):
        """Test CONCLUDE SUCCESS message roundtrip."""
        receipt = JobReceipt(
            status="SUCCESS",
            metrics={"rows": 5000, "size_bytes": 250000},
            artifacts=[
                {"topic": "output", "uri": "parquet://output.parquet"},
                {"topic": "metadata", "uri": "sqlite:///db.sqlite/metadata"},
            ],
        )
        frames = msg_conclude(111, receipt)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.CONCLUDE
        assert job_id == 111
        assert payload_dict["status"] == "SUCCESS"
        assert payload_dict["metrics"]["rows"] == 5000
        assert len(payload_dict["artifacts"]) == 2

    def test_conclude_failure_roundtrip(self):
        """Test CONCLUDE FAILED message roundtrip."""
        receipt = JobReceipt(
            status="FAILED",
            metrics={},
            artifacts=[],
            error_message="File not found: /data/missing.csv",
        )
        frames = msg_conclude(222, receipt)
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.CONCLUDE
        assert job_id == 222
        assert payload_dict["status"] == "FAILED"
        assert "File not found" in payload_dict["error_message"]

    def test_error_roundtrip(self):
        """Test ERR message roundtrip."""
        frames = msg_err(333, "Division by zero", traceback="Traceback...\nZeroDivisionError")
        opcode, job_id, payload_dict = unpack_msg(frames)

        assert opcode == OpCode.ERR
        assert job_id == 333
        assert payload_dict["message"] == "Division by zero"
        assert "ZeroDivisionError" in payload_dict["traceback"]
