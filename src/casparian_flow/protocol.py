# src/casparian_flow/protocol.py
"""
Binary Protocol v4: The Split Plane Protocol.

Wire format for Sentinel <-> Worker communication.
Control Plane only - Data flows directly from Worker to Storage.

DEPRECATED: This protocol has been ported to Rust.

The Rust implementation is in: crates/cf_protocol/

This file is kept for:
- Reference implementation
- Python tools that need to communicate with Rust Sentinel/Worker
- CLI publish workflow

Migration: Operation Iron Core (2025-12-23)
"""

import struct
import json
from enum import IntEnum
from typing import Optional
from pydantic import BaseModel

PROTOCOL_VERSION = 0x04


class OpCode(IntEnum):
    """
    Split Plane Protocol OpCodes.
    """

    UNKNOWN = 0

    # Worker -> Sentinel (Handshake)
    IDENTIFY = 1  # "I am here. My capabilities are [A, B, C]."

    # Sentinel -> Worker (Command)
    DISPATCH = 2  # "Process this file. Here is your sink configuration."

    # Sentinel -> Worker (Abort)
    ABORT = 3  # "Cancel this job."

    # Worker -> Sentinel (Keep-alive)
    HEARTBEAT = 4  # "Still alive, working on job X."

    # Worker -> Sentinel (Completion)
    CONCLUDE = 5  # "Job finished. Here is the receipt."

    # Bidirectional (Error)
    ERR = 6  # "Something went wrong."

    # Sentinel -> Worker (Config Refresh)
    RELOAD = 7  # "Reload configuration / plugins."

    # v5.0 Bridge Mode: Environment Provisioning
    PREPARE_ENV = 8  # "Provision this environment (lockfile) before execution."
    ENV_READY = 9  # "Environment is ready for use."

    # v5.0 Bridge Mode: Artifact Deployment
    DEPLOY = 10  # "Deploy this artifact (source + lockfile + signature)."


# Header: !BBHQI (16 bytes)
# [VER:1][OP:1][RES:2][JOB_ID:8][LEN:4]
# RES = Reserved for future use
# CRITICAL: Q (8 bytes) for JOB_ID, I (4 bytes) for LEN
# This gives ~18 quintillion possible job IDs before overflow
HEADER_FORMAT = "!BBHQI"
HEADER_SIZE = 16


def pack_header(
    op: OpCode,
    job_id: int,
    payload_len: int,
    version: int = PROTOCOL_VERSION,
) -> bytes:
    """
    Pack a protocol header.

    Args:
        op: OpCode for this message
        job_id: Job ID (0 for non-job messages like IDENTIFY)
        payload_len: Length of the JSON payload in bytes
        version: Protocol version (default: 0x04)

    Returns:
        16-byte header
    """
    reserved = 0  # Reserved field for future use
    return struct.pack(HEADER_FORMAT, version, op, reserved, job_id, payload_len)


def unpack_header(data: bytes) -> tuple[OpCode, int, int]:
    """
    Unpack a protocol header.

    Args:
        data: Header bytes (must be at least 16 bytes)

    Returns:
        Tuple of (opcode, job_id, payload_len)

    Raises:
        ValueError: If header is malformed or version mismatch
    """
    if len(data) < HEADER_SIZE:
        raise ValueError(f"Header too short: expected {HEADER_SIZE}, got {len(data)}")

    version, op_raw, reserved, job_id, payload_len = struct.unpack(
        HEADER_FORMAT, data[:HEADER_SIZE]
    )

    if version != PROTOCOL_VERSION:
        raise ValueError(f"Version mismatch: {version} != {PROTOCOL_VERSION}")

    return OpCode(op_raw), job_id, payload_len


# --- Pydantic Models ---


class SinkConfig(BaseModel):
    """
    Configuration for a single data sink.
    Worker will use this to instantiate the appropriate sink.
    """

    topic: str
    uri: str
    mode: str = "append"  # "append" | "replace" | "error"
    schema_def: Optional[str] = None  # Renamed from schema_json to avoid BaseModel conflict


class DispatchCommand(BaseModel):
    """
    Payload for OpCode.DISPATCH.
    Sentinel -> Worker: "Process this file in isolated venv with Bridge Mode."

    v5.0: Bridge Mode is now mandatory. All execution happens in isolated subprocesses.
    """

    plugin_name: str
    file_path: str
    sinks: list[SinkConfig]
    file_version_id: int  # Required for lineage restoration

    # Bridge Mode fields (now required)
    env_hash: str  # SHA256 of lockfile - links to PluginEnvironment
    source_code: str  # Plugin source code for subprocess execution
    artifact_hash: Optional[str] = None  # For signature verification (optional for legacy manifests)


class JobReceipt(BaseModel):
    """
    Payload for OpCode.CONCLUDE.
    Worker -> Sentinel: "Job finished. Here are the results."
    """

    status: str  # "SUCCESS" | "FAILED"
    metrics: dict[str, int]  # e.g., {"rows": 1500, "size_bytes": 42000}
    artifacts: list[dict[str, str]]  # e.g., [{"topic": "output", "uri": "s3://..."}]
    error_message: Optional[str] = None  # Populated if status == "FAILED"


class IdentifyPayload(BaseModel):
    """
    Payload for OpCode.IDENTIFY.
    Worker -> Sentinel: Handshake with capabilities.
    """

    capabilities: list[str]  # List of plugin names this worker can execute
    worker_id: Optional[str] = None  # Optional stable worker ID


class HeartbeatPayload(BaseModel):
    """
    Payload for OpCode.HEARTBEAT.
    Worker -> Sentinel: Status update.
    """

    status: str  # "IDLE" | "BUSY"
    current_job_id: Optional[int] = None


class ErrorPayload(BaseModel):
    """
    Payload for OpCode.ERR.
    Bidirectional: Error notification.
    """

    message: str
    traceback: Optional[str] = None


# --- v5.0 Bridge Mode Payloads ---


class PrepareEnvCommand(BaseModel):
    """
    Payload for OpCode.PREPARE_ENV.
    Sentinel -> Worker: "Provision this environment before execution."

    Enables Eager Provisioning to avoid network blocking during job execution.
    """

    env_hash: str  # SHA256 of lockfile content
    lockfile_content: str  # Raw TOML content (uv.lock)
    python_version: Optional[str] = None  # e.g., "3.11"


class EnvReadyPayload(BaseModel):
    """
    Payload for OpCode.ENV_READY.
    Worker -> Sentinel: "Environment is ready."
    """

    env_hash: str
    interpreter_path: str  # Path to Python interpreter in venv
    cached: bool = False  # True if environment was already cached


class DeployCommand(BaseModel):
    """
    Payload for OpCode.DEPLOY.
    CLI -> Sentinel: "Deploy this artifact to the registry."

    Part of the Publisher workflow.
    """

    plugin_name: str
    version: str
    source_code: str
    lockfile_content: str  # uv.lock content (empty string for legacy mode)
    env_hash: str  # SHA256(lockfile_content)
    artifact_hash: str  # SHA256(source_code + lockfile_content)
    signature: str  # Ed25519 signature of artifact_hash
    publisher_name: str
    publisher_email: Optional[str] = None
    azure_oid: Optional[str] = None  # For enterprise mode
    system_requirements: Optional[list[str]] = None  # e.g., ["glibc_2.31"]


# --- Message Builders ---


def msg_identify(capabilities: list[str], worker_id: Optional[str] = None) -> list:
    """Build an IDENTIFY message (Worker -> Sentinel)."""
    payload_obj = IdentifyPayload(capabilities=capabilities, worker_id=worker_id)
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.IDENTIFY, 0, len(payload)), payload]


def msg_dispatch(
    job_id: int,
    plugin_name: str,
    file_path: str,
    sinks: list[SinkConfig],
    file_version_id: int,
    env_hash: str,
    source_code: str,
    artifact_hash: Optional[str] = None,
) -> list:
    """
    Build a DISPATCH message (Sentinel -> Worker).

    v5.0: Bridge Mode is mandatory - env_hash and source_code are required.
    """
    payload_obj = DispatchCommand(
        plugin_name=plugin_name,
        file_path=file_path,
        sinks=sinks,
        file_version_id=file_version_id,
        env_hash=env_hash,
        source_code=source_code,
        artifact_hash=artifact_hash,
    )
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.DISPATCH, job_id, len(payload)), payload]


def msg_abort(job_id: int) -> list:
    """Build an ABORT message (Sentinel -> Worker)."""
    # ABORT has no payload - just the header
    return [pack_header(OpCode.ABORT, job_id, 0), b""]


def msg_heartbeat(
    job_id: int, status: str, current_job_id: Optional[int] = None
) -> list:
    """Build a HEARTBEAT message (Worker -> Sentinel)."""
    payload_obj = HeartbeatPayload(status=status, current_job_id=current_job_id)
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.HEARTBEAT, job_id, len(payload)), payload]


def msg_conclude(job_id: int, receipt: JobReceipt) -> list:
    """Build a CONCLUDE message (Worker -> Sentinel)."""
    payload = receipt.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.CONCLUDE, job_id, len(payload)), payload]


def msg_err(job_id: int, message: str, traceback: Optional[str] = None) -> list:
    """Build an ERR message (Bidirectional)."""
    payload_obj = ErrorPayload(message=message, traceback=traceback)
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.ERR, job_id, len(payload)), payload]


# --- v5.0 Bridge Mode Message Builders ---


def msg_prepare_env(
    env_hash: str,
    lockfile_content: str,
    python_version: Optional[str] = None,
) -> list:
    """Build a PREPARE_ENV message (Sentinel -> Worker)."""
    payload_obj = PrepareEnvCommand(
        env_hash=env_hash,
        lockfile_content=lockfile_content,
        python_version=python_version,
    )
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.PREPARE_ENV, 0, len(payload)), payload]


def msg_env_ready(
    env_hash: str,
    interpreter_path: str,
    cached: bool = False,
) -> list:
    """Build an ENV_READY message (Worker -> Sentinel)."""
    payload_obj = EnvReadyPayload(
        env_hash=env_hash,
        interpreter_path=interpreter_path,
        cached=cached,
    )
    payload = payload_obj.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.ENV_READY, 0, len(payload)), payload]


def msg_deploy(cmd: DeployCommand) -> list:
    """Build a DEPLOY message (CLI -> Sentinel)."""
    payload = cmd.model_dump_json().encode("utf-8")
    return [pack_header(OpCode.DEPLOY, 0, len(payload)), payload]


# --- Message Unpacking ---


def unpack_msg(frames: list[bytes]) -> tuple[OpCode, int, dict]:
    """
    Unpack a protocol message.

    Args:
        frames: List of ZMQ frames (must have at least 2: header + payload)

    Returns:
        Tuple of (opcode, job_id, payload_dict)
        payload_dict is the parsed JSON payload as a Python dict

    Raises:
        ValueError: If message is malformed
    """
    if len(frames) < 2:
        raise ValueError(f"Expected at least 2 frames, got {len(frames)}")

    header = frames[0]
    payload = frames[1]

    opcode, job_id, payload_len = unpack_header(header)

    # Parse JSON payload (if present)
    payload_dict = {}
    if payload_len > 0:
        try:
            payload_dict = json.loads(payload.decode("utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            raise ValueError(f"Failed to decode JSON payload: {e}")

    return opcode, job_id, payload_dict