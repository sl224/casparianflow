# src/casparian_flow/protocol.py
"""
Binary Protocol for ZMQ-based Plugin Isolation.

This module defines the wire format for communication between the
Worker (Router) and Sidecar (Plugin) processes.

Design Principles (Data-Oriented):
- Fixed-width binary headers (no JSON parsing overhead)
- Zero-copy payloads via Arrow IPC
- Network byte order for cross-platform compatibility

Header Format: 16 bytes, packed as '!BxHQI'
┌──────────┬──────────┬──────────┬──────────────┬──────────────┐
│ OP_CODE  │ FLAGS    │ PAD      │ JOB_ID       │ META_LEN     │
│ uint8    │ uint8    │ uint16   │ uint64       │ uint32       │
│ 1 byte   │ 1 byte   │ 2 bytes  │ 8 bytes      │ 4 bytes      │
└──────────┴──────────┴──────────┴──────────────┴──────────────┘
"""
import struct
from enum import IntEnum
from typing import Tuple, Optional


class OpCode(IntEnum):
    """Message operation codes."""
    REG = 1    # Plugin registration (Sidecar → Router)
    EXEC = 2   # Execute job (Router → Sidecar)
    DATA = 3   # Data payload with Arrow IPC (Sidecar → Router)
    ERR = 4    # Error with traceback (Sidecar → Router)
    DONE = 5   # Job complete (Sidecar → Router)


# Header format: Network byte order (!), B=uint8, x=pad, H=uint16, Q=uint64, I=uint32
HEADER_FORMAT = '!BxHQI'
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)  # Should be 16

# Validate header size at module load
assert HEADER_SIZE == 16, f"Header size mismatch: expected 16, got {HEADER_SIZE}"


def pack_header(op: OpCode, job_id: int, meta_len: int, flags: int = 0) -> bytes:
    """
    Pack a header into 16 bytes.
    
    Args:
        op: Operation code
        job_id: The job ID (lineage key)
        meta_len: Length of variable tail (filename, error string, etc.)
        flags: Reserved flags byte (default 0)
    
    Returns:
        16 bytes representing the header
    """
    return struct.pack(HEADER_FORMAT, op, 0, job_id, meta_len)


def unpack_header(data: bytes) -> Tuple[OpCode, int, int]:
    """
    Unpack a header from bytes.
    
    Args:
        data: At least 16 bytes
    
    Returns:
        Tuple of (op_code, job_id, meta_len)
    
    Raises:
        ValueError: If data is too short or op_code is invalid
    """
    if len(data) < HEADER_SIZE:
        raise ValueError(f"Header too short: expected {HEADER_SIZE}, got {len(data)}")
    
    op_raw, pad, job_id, meta_len = struct.unpack(HEADER_FORMAT, data[:HEADER_SIZE])
    
    try:
        op = OpCode(op_raw)
    except ValueError:
        raise ValueError(f"Invalid op_code: {op_raw}")
    
    return op, job_id, meta_len


def validate_header(data: bytes) -> Optional[str]:
    """
    Validate header bytes without raising exceptions.
    Returns error message or None if valid.
    """
    if len(data) < HEADER_SIZE:
        return f"Header too short: {len(data)} bytes"
    
    op_raw = data[0]
    if op_raw < 1 or op_raw > 5:
        return f"Invalid op_code: {op_raw}"
    
    # Check padding is zero (reserved)
    if data[2:4] != b'\x00\x00':
        return "Non-zero padding bytes"
    
    return None


# Convenience message builders
def msg_register(plugin_name: str) -> list:
    """Build REG message frames."""
    name_bytes = plugin_name.encode('utf-8')
    return [pack_header(OpCode.REG, 0, len(name_bytes)), name_bytes]


def msg_execute(job_id: int, filepath: str) -> list:
    """Build EXEC message frames."""
    path_bytes = filepath.encode('utf-8')
    return [pack_header(OpCode.EXEC, job_id, len(path_bytes)), path_bytes]


def msg_data(job_id: int, payload: bytes) -> list:
    """Build DATA message frames with Arrow IPC payload."""
    return [pack_header(OpCode.DATA, job_id, 0), b'', payload]


def msg_done(job_id: int) -> list:
    """Build DONE message frames."""
    return [pack_header(OpCode.DONE, job_id, 0), b'']


def msg_error(job_id: int, error: str) -> list:
    """Build ERR message frames."""
    err_bytes = error.encode('utf-8')
    return [pack_header(OpCode.ERR, job_id, len(err_bytes)), err_bytes]
