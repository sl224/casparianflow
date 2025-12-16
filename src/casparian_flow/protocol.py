# src/casparian_flow/protocol.py
"""
Binary Protocol for ZMQ-based Plugin Isolation (Protocol v2).

This module defines the wire format for communication between the
Worker (Router) and Sidecar (Plugin) processes.

Design Principles (Data-Oriented):
- Fixed-width binary headers (no JSON parsing overhead)
- Zero-copy payloads via Arrow IPC
- Network byte order for cross-platform compatibility

Header Format: 16 bytes, packed as '!BBHIQ'
┌──────────┬──────────┬──────────┬──────────────┬──────────────┐
│ VERSION  │ OP_CODE  │ FLAGS    │ META_LEN     │ JOB_ID       │
│ uint8    │ uint8    │ uint16   │ uint32       │ uint64       │
│ 1 byte   │ 1 byte   │ 2 bytes  │ 4 bytes      │ 8 bytes      │
└──────────┴──────────┴──────────┴──────────────┴──────────────┘

Flags Bitmask (uint16):
- Bit 0: Compression (0=uncompressed, 1=compressed)
- Bits 1-4: ContentType (4-bit enum)
- Bits 5-15: Reserved
"""

import struct
from enum import IntEnum, IntFlag
from typing import Tuple, Optional


# Protocol version constant
PROTOCOL_VERSION = 0x01


class OpCode(IntEnum):
    """Message operation codes."""

    REG = 1  # Plugin registration (Sidecar → Router)
    EXEC = 2  # Execute job (Router → Sidecar)
    DATA = 3  # Data payload with Arrow IPC (Sidecar → Router)
    ERR = 4  # Error with traceback (Sidecar → Router)
    DONE = 5  # Job complete (Sidecar → Router)
    HEARTBEAT = 6  # Keep-alive ping (Bidirectional)
    DEPLOY = 7  # Deploy new plugin code (Router → Sidecar)


class ContentType(IntEnum):
    """Payload content type identifier."""

    UNKNOWN = 0
    JSON = 1
    ARROW = 2
    UTF8 = 3
    PARQUET = 4


class HeaderFlags(IntFlag):
    """Header flags bitmask."""

    NONE = 0
    COMPRESSED = 1 << 0  # Bit 0: payload is compressed


# Header format: Network byte order (!), B=uint8, H=uint16, I=uint32, Q=uint64
HEADER_FORMAT = "!BBHIQ"
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)  # Should be 16

# Validate header size at module load
assert HEADER_SIZE == 16, f"Header size mismatch: expected 16, got {HEADER_SIZE}"


def pack_header(
    op: OpCode,
    job_id: int,
    meta_len: int,
    content_type: ContentType = ContentType.UNKNOWN,
    compressed: bool = False,
    version: int = PROTOCOL_VERSION,
) -> bytes:
    """
    Pack a header into 16 bytes (Protocol v2).

    Args:
        op: Operation code
        job_id: The job ID (lineage key)
        meta_len: Length of variable tail (filename, error string, etc.)
        content_type: Type of payload content
        compressed: Whether payload is compressed
        version: Protocol version (default: PROTOCOL_VERSION)

    Returns:
        16 bytes representing the header
    """
    # Build flags field
    flags = HeaderFlags.NONE
    if compressed:
        flags |= HeaderFlags.COMPRESSED

    # Encode content_type in bits 1-4 of flags
    flags |= (content_type & 0x0F) << 1

    return struct.pack(HEADER_FORMAT, version, op, flags, meta_len, job_id)


def unpack_header(data: bytes) -> Tuple[OpCode, int, int, ContentType, bool]:
    """
    Unpack a header from bytes (Protocol v2).

    Args:
        data: At least 16 bytes

    Returns:
        Tuple of (op_code, job_id, meta_len, content_type, compressed)

    Raises:
        ValueError: If data is too short, version mismatch, or op_code is invalid
    """
    if len(data) < HEADER_SIZE:
        raise ValueError(f"Header too short: expected {HEADER_SIZE}, got {len(data)}")

    version, op_raw, flags, meta_len, job_id = struct.unpack(
        HEADER_FORMAT, data[:HEADER_SIZE]
    )

    # Version checking
    if version != PROTOCOL_VERSION:
        raise ValueError(
            f"Protocol version mismatch: expected {PROTOCOL_VERSION}, got {version}"
        )

    # Parse OpCode
    try:
        op = OpCode(op_raw)
    except ValueError:
        raise ValueError(f"Invalid op_code: {op_raw}")

    # Parse flags
    compressed = bool(flags & HeaderFlags.COMPRESSED)
    content_type = ContentType((flags >> 1) & 0x0F)

    return op, job_id, meta_len, content_type, compressed


def validate_header(data: bytes) -> Optional[str]:
    """
    Validate header bytes without raising exceptions.
    Returns error message or None if valid.
    """
    if len(data) < HEADER_SIZE:
        return f"Header too short: {len(data)} bytes"

    version = data[0]
    if version != PROTOCOL_VERSION:
        return f"Protocol version mismatch: expected {PROTOCOL_VERSION}, got {version}"

    op_raw = data[1]
    if op_raw < 1 or op_raw > 7:
        return f"Invalid op_code: {op_raw}"

    return None


# Convenience message builders
def msg_register(plugin_name: str) -> list:
    """Build REG message frames."""
    name_bytes = plugin_name.encode("utf-8")
    return [
        pack_header(OpCode.REG, 0, len(name_bytes), content_type=ContentType.UTF8),
        name_bytes,
    ]


def msg_execute(job_id: int, filepath: str) -> list:
    """Build EXEC message frames."""
    path_bytes = filepath.encode("utf-8")
    return [
        pack_header(OpCode.EXEC, job_id, len(path_bytes), content_type=ContentType.UTF8),
        path_bytes,
    ]


def msg_data(job_id: int, payload: bytes) -> list:
    """Build DATA message frames with Arrow IPC payload."""
    return [
        pack_header(OpCode.DATA, job_id, 0, content_type=ContentType.ARROW),
        b"",
        payload,
    ]


def msg_done(job_id: int) -> list:
    """Build DONE message frames."""
    return [pack_header(OpCode.DONE, job_id, 0), b""]


def msg_error(job_id: int, error: str) -> list:
    """Build ERR message frames."""
    err_bytes = error.encode("utf-8")
    return [
        pack_header(OpCode.ERR, job_id, len(err_bytes), content_type=ContentType.UTF8),
        err_bytes,
    ]


def msg_heartbeat() -> list:
    """Build HEARTBEAT message frames (empty payload)."""
    return [pack_header(OpCode.HEARTBEAT, 0, 0), b""]


def msg_deploy(plugin_name: str, source_code: str, signature: str) -> list:
    """
    Build DEPLOY message frames with JSON payload.

    Args:
        plugin_name: Name of the plugin to deploy
        source_code: Python source code
        signature: Cryptographic signature (HMAC)

    Returns:
        ZMQ message frames
    """
    import json

    payload = {
        "plugin_name": plugin_name,
        "source_code": source_code,
        "signature": signature,
    }
    payload_bytes = json.dumps(payload).encode("utf-8")
    return [
        pack_header(OpCode.DEPLOY, 0, len(payload_bytes), content_type=ContentType.JSON),
        payload_bytes,
    ]
