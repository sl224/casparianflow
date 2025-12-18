# src/casparian_flow/protocol.py
"""
Binary Protocol v3: The Generalist Protocol.

Wire format for Sentinel <-> Generalist Worker communication.
"""

import struct
from enum import IntEnum, IntFlag
from typing import Tuple, Optional

PROTOCOL_VERSION = 0x03


class OpCode(IntEnum):
    """
    Generalist Worker OpCodes.
    """

    UNKNOWN = 0

    # Worker -> Sentinel (Handshake)
    HELLO = 1  # "I am here. My capabilities are [A, B, C]."

    # Sentinel -> Worker (Command)
    EXEC = 2  # "Process this file using Plugin X."

    # Worker -> Sentinel (Payload)
    DATA = 3  # "Here is a chunk of data (Arrow IPC)."

    # Worker -> Sentinel (State)
    READY = 4  # "I am finished/idle. Give me work."

    # Bidirectional
    ERR = 5  # "Something went wrong."
    HEARTBEAT = 6  # Keep-alive


class ContentType(IntEnum):
    """Payload content type."""

    UNKNOWN = 0
    JSON = 1  # Used for HELLO caps, EXEC metadata
    ARROW = 2  # Used for DATA frames
    UTF8 = 3  # Used for ERR messages
    PARQUET = 4


class HeaderFlags(IntFlag):
    NONE = 0
    COMPRESSED = 1 << 0


# Header: !BBHIQ (16 bytes)
# [VER:1][OP:1][FLAGS:2][META_LEN:4][JOB_ID:8]
HEADER_FORMAT = "!BBHIQ"
HEADER_SIZE = 16


def pack_header(
    op: OpCode,
    job_id: int,
    meta_len: int,
    content_type: ContentType = ContentType.UNKNOWN,
    compressed: bool = False,
    version: int = PROTOCOL_VERSION,
) -> bytes:
    flags = HeaderFlags.NONE
    if compressed:
        flags |= HeaderFlags.COMPRESSED

    # Encode content type in flags (bits 1-4)
    flags |= (content_type & 0x0F) << 1

    return struct.pack(HEADER_FORMAT, version, op, flags, meta_len, job_id)


def unpack_header(data: bytes) -> Tuple[OpCode, int, int, ContentType, bool]:
    if len(data) < HEADER_SIZE:
        raise ValueError(f"Header too short: expected {HEADER_SIZE}, got {len(data)}")

    version, op_raw, flags, meta_len, job_id = struct.unpack(
        HEADER_FORMAT, data[:HEADER_SIZE]
    )

    if version != PROTOCOL_VERSION:
        raise ValueError(f"Version mismatch: {version} != {PROTOCOL_VERSION}")

    compressed = bool(flags & HeaderFlags.COMPRESSED)
    content_type = ContentType((flags >> 1) & 0x0F)

    return OpCode(op_raw), job_id, meta_len, content_type, compressed


# --- Message Builders ---


def msg_hello(capabilities: list) -> list:
    import json

    payload = json.dumps(capabilities).encode("utf-8")
    return [pack_header(OpCode.HELLO, 0, len(payload), ContentType.JSON), payload]


def msg_exec(job_id: int, plugin_name: str, file_path: str) -> list:
    import json

    payload = json.dumps({"plugin": plugin_name, "path": file_path}).encode("utf-8")
    return [pack_header(OpCode.EXEC, job_id, len(payload), ContentType.JSON), payload]


def msg_ready() -> list:
    return [pack_header(OpCode.READY, 0, 0), b""]


def msg_data(job_id: int, topic: str, arrow_bytes: bytes) -> list:
    """
    Constructs a multi-frame data message.
    Frames: [Header, Topic (bytes), Payload (Arrow bytes)]
    """
    topic_bytes = topic.encode("utf-8")
    return [
        pack_header(OpCode.DATA, job_id, len(arrow_bytes), ContentType.ARROW),
        topic_bytes,
        arrow_bytes,
    ]


def msg_err(job_id: int, message: str) -> list:
    payload = message.encode("utf-8")
    return [pack_header(OpCode.ERR, job_id, len(payload), ContentType.UTF8), payload]