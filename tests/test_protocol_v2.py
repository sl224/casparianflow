# tests/test_protocol_v2.py
"""
Tests for Protocol v2 binary format and new OpCodes.

Tests the !BBHIQ header format, VERSION field, new OpCodes (HEARTBEAT, DEPLOY),
ContentType encoding, compression flags, and message builders.
"""
import pytest
import struct
import json

from casparian_flow.protocol import (
    pack_header,
    unpack_header,
    validate_header,
    OpCode,
    ContentType,
    HeaderFlags,
    PROTOCOL_VERSION,
    HEADER_SIZE,
    HEADER_FORMAT,
    msg_register,
    msg_execute,
    msg_data,
    msg_done,
    msg_error,
    msg_heartbeat,
    msg_deploy,
)


class TestHeaderFormat:
    """Test the 16-byte !BBHIQ header format."""

    def test_header_size(self):
        """Verify header is exactly 16 bytes."""
        assert HEADER_SIZE == 16
        assert struct.calcsize(HEADER_FORMAT) == 16

    def test_header_format_v2(self):
        """Verify !BBHIQ packing/unpacking."""
        header = pack_header(OpCode.EXEC, job_id=12345, meta_len=100)
        assert len(header) == 16
        assert isinstance(header, bytes)

    def test_version_field(self):
        """Ensure VERSION byte is 0x01."""
        header = pack_header(OpCode.EXEC, job_id=1, meta_len=0)
        version = header[0]
        assert version == PROTOCOL_VERSION
        assert version == 0x01

    def test_opcode_position(self):
        """Verify OpCode is at byte offset 1."""
        header = pack_header(OpCode.DATA, job_id=1, meta_len=0)
        opcode = header[1]
        assert opcode == OpCode.DATA

    def test_flags_position(self):
        """Verify FLAGS field is at bytes 2-3 (uint16)."""
        header = pack_header(
            OpCode.EXEC, job_id=1, meta_len=0, content_type=ContentType.JSON
        )
        # Extract flags as uint16 in network byte order
        flags = struct.unpack("!H", header[2:4])[0]
        # ContentType.JSON (1) should be in bits 1-4: (1 << 1) = 2
        assert flags == 2

    def test_meta_len_position(self):
        """Verify META_LEN is at bytes 4-7 (uint32)."""
        meta_len_value = 9999
        header = pack_header(OpCode.EXEC, job_id=1, meta_len=meta_len_value)
        meta_len = struct.unpack("!I", header[4:8])[0]
        assert meta_len == meta_len_value

    def test_job_id_position(self):
        """Verify JOB_ID is at bytes 8-15 (uint64)."""
        job_id_value = 0xDEADBEEF12345678
        header = pack_header(OpCode.EXEC, job_id=job_id_value, meta_len=0)
        job_id = struct.unpack("!Q", header[8:16])[0]
        assert job_id == job_id_value


class TestOpCodes:
    """Test all OpCodes including new HEARTBEAT and DEPLOY."""

    def test_existing_opcodes(self):
        """Test legacy OpCodes 1-5."""
        assert OpCode.REG == 1
        assert OpCode.EXEC == 2
        assert OpCode.DATA == 3
        assert OpCode.ERR == 4
        assert OpCode.DONE == 5

    def test_new_opcodes(self):
        """Test HEARTBEAT(6) and DEPLOY(7)."""
        assert OpCode.HEARTBEAT == 6
        assert OpCode.DEPLOY == 7

    def test_all_opcodes_pack(self):
        """Verify all OpCodes can be packed."""
        for opcode in OpCode:
            header = pack_header(opcode, job_id=1, meta_len=0)
            assert len(header) == 16
            assert header[1] == opcode


class TestContentType:
    """Test ContentType enum and encoding in flags."""

    def test_content_type_values(self):
        """Verify ContentType enum values."""
        assert ContentType.UNKNOWN == 0
        assert ContentType.JSON == 1
        assert ContentType.ARROW == 2
        assert ContentType.UTF8 == 3
        assert ContentType.PARQUET == 4

    def test_content_type_encoding(self):
        """Verify ContentType encodes in bits 1-4 of flags."""
        for content_type in ContentType:
            header = pack_header(
                OpCode.DATA, job_id=1, meta_len=0, content_type=content_type
            )
            op, job_id, meta_len, decoded_type, compressed = unpack_header(header)
            assert decoded_type == content_type

    def test_content_type_with_compression(self):
        """Verify ContentType and compression can coexist."""
        header = pack_header(
            OpCode.DATA,
            job_id=1,
            meta_len=0,
            content_type=ContentType.ARROW,
            compressed=True,
        )
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert content_type == ContentType.ARROW
        assert compressed is True


class TestCompressionFlag:
    """Test compression bit (bit 0) in flags."""

    def test_compression_flag_off(self):
        """Verify compression=False sets bit 0 to 0."""
        header = pack_header(OpCode.DATA, job_id=1, meta_len=0, compressed=False)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert compressed is False

    def test_compression_flag_on(self):
        """Verify compression=True sets bit 0 to 1."""
        header = pack_header(OpCode.DATA, job_id=1, meta_len=0, compressed=True)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert compressed is True

    def test_header_flags_enum(self):
        """Verify HeaderFlags enum."""
        assert HeaderFlags.NONE == 0
        assert HeaderFlags.COMPRESSED == 1


class TestHeaderRoundtrip:
    """Test pack → unpack → verify all fields."""

    def test_roundtrip_basic(self):
        """Basic roundtrip test."""
        header = pack_header(OpCode.EXEC, job_id=999, meta_len=50)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)

        assert op == OpCode.EXEC
        assert job_id == 999
        assert meta_len == 50
        assert content_type == ContentType.UNKNOWN
        assert compressed is False

    def test_roundtrip_with_content_type(self):
        """Roundtrip with ContentType."""
        header = pack_header(
            OpCode.DATA, job_id=123, meta_len=200, content_type=ContentType.JSON
        )
        op, job_id, meta_len, content_type, compressed = unpack_header(header)

        assert op == OpCode.DATA
        assert job_id == 123
        assert meta_len == 200
        assert content_type == ContentType.JSON
        assert compressed is False

    def test_roundtrip_with_all_flags(self):
        """Roundtrip with both ContentType and compression."""
        header = pack_header(
            OpCode.DEPLOY,
            job_id=5555,
            meta_len=1024,
            content_type=ContentType.PARQUET,
            compressed=True,
        )
        op, job_id, meta_len, content_type, compressed = unpack_header(header)

        assert op == OpCode.DEPLOY
        assert job_id == 5555
        assert meta_len == 1024
        assert content_type == ContentType.PARQUET
        assert compressed is True

    def test_roundtrip_large_job_id(self):
        """Test with maximum uint64 job_id."""
        max_job_id = 2**64 - 1
        header = pack_header(OpCode.EXEC, job_id=max_job_id, meta_len=0)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert job_id == max_job_id


class TestVersionValidation:
    """Test version mismatch rejection."""

    def test_version_mismatch_rejection(self):
        """Invalid version should raise ValueError."""
        # Manually craft a header with wrong version
        bad_header = struct.pack("!BBHIQ", 0x02, OpCode.EXEC, 0, 0, 1)
        with pytest.raises(ValueError, match="Protocol version mismatch"):
            unpack_header(bad_header)

    def test_validate_header_version_check(self):
        """validate_header should detect version mismatch."""
        bad_header = struct.pack("!BBHIQ", 0xFF, OpCode.EXEC, 0, 0, 1)
        error = validate_header(bad_header)
        assert error is not None
        assert "version mismatch" in error.lower()

    def test_valid_version_passes(self):
        """Valid version (0x01) should pass validation."""
        header = pack_header(OpCode.EXEC, job_id=1, meta_len=0)
        error = validate_header(header)
        assert error is None


class TestInvalidHeaders:
    """Test invalid header rejection."""

    def test_invalid_opcode_rejection(self):
        """OpCode > 7 should fail validation."""
        bad_header = struct.pack("!BBHIQ", PROTOCOL_VERSION, 99, 0, 0, 1)
        error = validate_header(bad_header)
        assert error is not None
        assert "Invalid op_code" in error

    def test_header_too_short(self):
        """Header < 16 bytes should raise ValueError."""
        short_header = b"tooshort"
        with pytest.raises(ValueError, match="Header too short"):
            unpack_header(short_header)

        error = validate_header(short_header)
        assert error is not None
        assert "too short" in error.lower()

    def test_empty_header(self):
        """Empty bytes should fail validation."""
        error = validate_header(b"")
        assert error is not None
        assert "too short" in error.lower()


class TestMessageBuilders:
    """Test convenience message builder functions."""

    def test_msg_register(self):
        """Test msg_register() builder."""
        frames = msg_register("my_plugin")
        assert len(frames) == 2
        assert len(frames[0]) == 16  # Header

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.REG
        assert job_id == 0
        assert meta_len == len(b"my_plugin")
        assert content_type == ContentType.UTF8

        assert frames[1] == b"my_plugin"

    def test_msg_execute(self):
        """Test msg_execute() builder."""
        frames = msg_execute(12345, "/path/to/file.csv")
        assert len(frames) == 2

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.EXEC
        assert job_id == 12345
        assert content_type == ContentType.UTF8

        assert frames[1] == b"/path/to/file.csv"

    def test_msg_data(self):
        """Test msg_data() builder with Arrow payload."""
        payload = b"arrow_ipc_data_here"
        frames = msg_data(999, payload)
        assert len(frames) == 3  # Header + empty + payload

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.DATA
        assert job_id == 999
        assert content_type == ContentType.ARROW

        assert frames[1] == b""  # Empty frame
        assert frames[2] == payload

    def test_msg_done(self):
        """Test msg_done() builder."""
        frames = msg_done(555)
        assert len(frames) == 2

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.DONE
        assert job_id == 555

        assert frames[1] == b""

    def test_msg_error(self):
        """Test msg_error() builder."""
        error_msg = "Something went wrong"
        frames = msg_error(777, error_msg)
        assert len(frames) == 2

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.ERR
        assert job_id == 777
        assert content_type == ContentType.UTF8

        assert frames[1] == error_msg.encode("utf-8")

    def test_msg_heartbeat(self):
        """Test msg_heartbeat() builder (empty payload)."""
        frames = msg_heartbeat()
        assert len(frames) == 2

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.HEARTBEAT
        assert job_id == 0
        assert meta_len == 0

        assert frames[1] == b""

    def test_msg_deploy(self):
        """Test msg_deploy() builder (JSON payload)."""
        source_code = "print('hello')"
        signature = "abc123signature"
        frames = msg_deploy("test_plugin", source_code, signature)
        assert len(frames) == 2

        op, job_id, meta_len, content_type, compressed = unpack_header(frames[0])
        assert op == OpCode.DEPLOY
        assert job_id == 0
        assert content_type == ContentType.JSON

        # Verify JSON payload structure
        payload = json.loads(frames[1].decode("utf-8"))
        assert payload["plugin_name"] == "test_plugin"
        assert payload["source_code"] == source_code
        assert payload["signature"] == signature


class TestBackwardCompatibility:
    """Ensure Protocol v2 doesn't break existing functionality."""

    def test_old_opcodes_still_work(self):
        """Legacy OpCodes (REG, EXEC, DATA, ERR, DONE) still function."""
        old_opcodes = [OpCode.REG, OpCode.EXEC, OpCode.DATA, OpCode.ERR, OpCode.DONE]
        for opcode in old_opcodes:
            header = pack_header(opcode, job_id=1, meta_len=0)
            op, job_id, meta_len, content_type, compressed = unpack_header(header)
            assert op == opcode

    def test_default_content_type_is_unknown(self):
        """When not specified, ContentType defaults to UNKNOWN."""
        header = pack_header(OpCode.DATA, job_id=1, meta_len=0)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert content_type == ContentType.UNKNOWN

    def test_default_compression_is_false(self):
        """When not specified, compression defaults to False."""
        header = pack_header(OpCode.DATA, job_id=1, meta_len=0)
        op, job_id, meta_len, content_type, compressed = unpack_header(header)
        assert compressed is False
