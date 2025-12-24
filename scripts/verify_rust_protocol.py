#!/usr/bin/env python3
"""
Cross-Language Protocol Verification Script

This script generates test messages using the Python protocol implementation
and verifies they can be read by Rust.

Usage:
    python scripts/verify_rust_protocol.py
"""

import sys
import json
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from casparian_flow.protocol import (
    pack_header,
    unpack_header,
    msg_identify,
    msg_heartbeat,
    msg_dispatch,
    OpCode,
    SinkConfig,
)


def test_header_generation():
    """Test that Python can generate headers that Rust will accept."""
    print("=" * 60)
    print("Test: Python Header Generation")
    print("=" * 60)

    test_cases = [
        (OpCode.DISPATCH, 12345, 1024),
        (OpCode.HEARTBEAT, 9999, 512),
        (OpCode.IDENTIFY, 0, 256),
        (OpCode.CONCLUDE, 77777, 2048),
    ]

    for opcode, job_id, payload_len in test_cases:
        header = pack_header(opcode, job_id, payload_len)
        assert len(header) == 16, f"Header should be 16 bytes, got {len(header)}"

        # Verify round-trip
        decoded_op, decoded_job, decoded_len = unpack_header(header)
        assert decoded_op == opcode
        assert decoded_job == job_id
        assert decoded_len == payload_len

        print(f"✓ {opcode.name:12} job_id={job_id:6} len={payload_len:4}")

    print()


def test_message_generation():
    """Test complete message generation."""
    print("=" * 60)
    print("Test: Python Message Generation")
    print("=" * 60)

    # IDENTIFY message
    frames = msg_identify(["plugin_a", "plugin_b"], worker_id="python-worker-001")
    header = frames[0]
    payload = frames[1]

    opcode, job_id, payload_len = unpack_header(header)
    assert opcode == OpCode.IDENTIFY
    assert job_id == 0

    payload_dict = json.loads(payload.decode("utf-8"))
    assert payload_dict["worker_id"] == "python-worker-001"
    assert "plugin_a" in payload_dict["capabilities"]
    print(f"✓ IDENTIFY message: {payload_dict}")

    # HEARTBEAT message
    frames = msg_heartbeat(42, "BUSY", current_job_id=12345)
    header = frames[0]
    payload = frames[1]

    opcode, job_id, payload_len = unpack_header(header)
    assert opcode == OpCode.HEARTBEAT
    assert job_id == 42

    payload_dict = json.loads(payload.decode("utf-8"))
    assert payload_dict["status"] == "BUSY"
    assert payload_dict["current_job_id"] == 12345
    print(f"✓ HEARTBEAT message: {payload_dict}")

    # DISPATCH message
    sink = SinkConfig(
        topic="output", uri="s3://test-bucket/output.parquet", mode="append"
    )
    frames = msg_dispatch(
        job_id=99999,
        plugin_name="test_plugin",
        file_path="/data/input.csv",
        sinks=[sink],
        file_version_id=1,
        env_hash="abc123",
        source_code="# Test plugin code",
    )
    header = frames[0]
    payload = frames[1]

    opcode, job_id, payload_len = unpack_header(header)
    assert opcode == OpCode.DISPATCH
    assert job_id == 99999

    payload_dict = json.loads(payload.decode("utf-8"))
    assert payload_dict["plugin_name"] == "test_plugin"
    assert payload_dict["file_path"] == "/data/input.csv"
    assert len(payload_dict["sinks"]) == 1
    print(f"✓ DISPATCH message: plugin={payload_dict['plugin_name']}")

    print()


def test_byte_order():
    """Verify Network Byte Order (Big Endian)."""
    print("=" * 60)
    print("Test: Byte Order Verification")
    print("=" * 60)

    # Create a header with known values
    header = pack_header(OpCode.DISPATCH, 0x0102030405060708, 0x11223344)

    # Manually verify byte order
    assert header[0] == 0x04, "Version should be 0x04"
    assert header[1] == 0x02, "OpCode.DISPATCH should be 0x02"

    # Job ID (8 bytes, big endian)
    job_id_bytes = header[4:12]
    expected_job_id = bytes([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])
    assert job_id_bytes == expected_job_id, f"Job ID bytes: {job_id_bytes.hex()}"

    # Payload length (4 bytes, big endian)
    len_bytes = header[12:16]
    expected_len = bytes([0x11, 0x22, 0x33, 0x44])
    assert len_bytes == expected_len, f"Length bytes: {len_bytes.hex()}"

    print("✓ Byte order is correct (Big Endian / Network Byte Order)")
    print(f"  Header: {header.hex()}")
    print()


if __name__ == "__main__":
    try:
        test_header_generation()
        test_message_generation()
        test_byte_order()

        print("=" * 60)
        print("✓ All Python protocol tests passed!")
        print("=" * 60)
        print()
        print("Next steps:")
        print("  1. Run: cargo test --package cf_protocol")
        print("  2. Verify cross-language compatibility tests pass")

    except AssertionError as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}")
        import traceback

        traceback.print_exc()
        sys.exit(1)
