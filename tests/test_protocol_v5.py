# tests/test_protocol_v5.py
"""
Tests for v5.0 Bridge Mode protocol additions.

Tests:
- New OpCodes (PREPARE_ENV, ENV_READY, DEPLOY)
- New payload models (PrepareEnvCommand, DeployCommand, BridgeDispatchCommand)
- Message builders and unpacking
- Backward compatibility with v4 messages
"""

import pytest
import json

from casparian_flow.protocol import (
    # OpCodes
    OpCode,
    PROTOCOL_VERSION,
    # v4 functions
    pack_header,
    unpack_header,
    unpack_msg,
    # v4 models
    SinkConfig,
    DispatchCommand,
    # v5 models
    PrepareEnvCommand,
    EnvReadyPayload,
    DeployCommand,
    BridgeDispatchCommand,
    # v5 message builders
    msg_prepare_env,
    msg_env_ready,
    msg_deploy,
    msg_bridge_dispatch,
)


class TestNewOpCodes:
    """Tests for v5.0 OpCode additions."""

    def test_prepare_env_opcode_value(self):
        """PREPARE_ENV has expected value."""
        assert OpCode.PREPARE_ENV == 8

    def test_env_ready_opcode_value(self):
        """ENV_READY has expected value."""
        assert OpCode.ENV_READY == 9

    def test_deploy_opcode_value(self):
        """DEPLOY has expected value."""
        assert OpCode.DEPLOY == 10

    def test_opcodes_are_distinct(self):
        """All opcodes have unique values."""
        values = [op.value for op in OpCode]
        assert len(values) == len(set(values))


class TestPrepareEnvMessages:
    """Tests for PREPARE_ENV message handling."""

    SAMPLE_LOCKFILE = "version = 1\n[[package]]\nname = 'pandas'\n"
    SAMPLE_ENV_HASH = "a" * 64

    def test_prepare_env_command_model(self):
        """PrepareEnvCommand validates required fields."""
        cmd = PrepareEnvCommand(
            env_hash=self.SAMPLE_ENV_HASH,
            lockfile_content=self.SAMPLE_LOCKFILE,
        )
        assert cmd.env_hash == self.SAMPLE_ENV_HASH
        assert cmd.python_version is None  # Optional

    def test_prepare_env_command_with_python_version(self):
        """PrepareEnvCommand accepts python_version."""
        cmd = PrepareEnvCommand(
            env_hash=self.SAMPLE_ENV_HASH,
            lockfile_content=self.SAMPLE_LOCKFILE,
            python_version="3.11",
        )
        assert cmd.python_version == "3.11"

    def test_msg_prepare_env_packs_correctly(self):
        """msg_prepare_env creates valid message frames."""
        frames = msg_prepare_env(
            env_hash=self.SAMPLE_ENV_HASH,
            lockfile_content=self.SAMPLE_LOCKFILE,
            python_version="3.12",
        )

        assert len(frames) == 2
        header, payload = frames

        # Verify header
        opcode, job_id, payload_len = unpack_header(header)
        assert opcode == OpCode.PREPARE_ENV
        assert job_id == 0
        assert payload_len == len(payload)

        # Verify payload
        data = json.loads(payload.decode("utf-8"))
        assert data["env_hash"] == self.SAMPLE_ENV_HASH
        assert data["lockfile_content"] == self.SAMPLE_LOCKFILE
        assert data["python_version"] == "3.12"


class TestEnvReadyMessages:
    """Tests for ENV_READY message handling."""

    def test_env_ready_payload_model(self):
        """EnvReadyPayload validates fields."""
        payload = EnvReadyPayload(
            env_hash="b" * 64,
            interpreter_path="/home/user/.venvs/abc/bin/python",
            cached=True,
        )
        assert payload.cached is True

    def test_msg_env_ready_packs_correctly(self):
        """msg_env_ready creates valid message frames."""
        frames = msg_env_ready(
            env_hash="c" * 64,
            interpreter_path="/path/to/python",
            cached=False,
        )

        opcode, job_id, _ = unpack_header(frames[0])
        assert opcode == OpCode.ENV_READY

        data = json.loads(frames[1].decode("utf-8"))
        assert data["cached"] is False


class TestDeployMessages:
    """Tests for DEPLOY message handling."""

    def test_deploy_command_model_required_fields(self):
        """DeployCommand requires core artifact fields."""
        cmd = DeployCommand(
            plugin_name="my_plugin",
            version="1.0.0",
            source_code="# code",
            lockfile_content="version = 1",
            env_hash="d" * 64,
            artifact_hash="e" * 64,
            signature="f" * 128,
            publisher_name="test_user",
        )
        assert cmd.plugin_name == "my_plugin"
        assert cmd.azure_oid is None  # Optional

    def test_deploy_command_with_enterprise_fields(self):
        """DeployCommand accepts enterprise mode fields."""
        cmd = DeployCommand(
            plugin_name="enterprise_plugin",
            version="2.0.0",
            source_code="# code",
            lockfile_content="version = 1",
            env_hash="g" * 64,
            artifact_hash="h" * 64,
            signature="i" * 128,
            publisher_name="enterprise_user",
            publisher_email="user@company.com",
            azure_oid="12345678-1234-1234-1234-123456789012",
            system_requirements=["glibc_2.31", "cuda_11.8"],
        )
        assert cmd.azure_oid is not None
        assert len(cmd.system_requirements) == 2

    def test_msg_deploy_packs_correctly(self):
        """msg_deploy creates valid message frames."""
        cmd = DeployCommand(
            plugin_name="test",
            version="1.0.0",
            source_code="# code",
            lockfile_content="lock",
            env_hash="x" * 64,
            artifact_hash="y" * 64,
            signature="z" * 128,
            publisher_name="user",
        )
        frames = msg_deploy(cmd)

        opcode, job_id, _ = unpack_header(frames[0])
        assert opcode == OpCode.DEPLOY

        data = json.loads(frames[1].decode("utf-8"))
        assert data["plugin_name"] == "test"
        assert data["artifact_hash"] == "y" * 64


class TestBridgeDispatchMessages:
    """Tests for Bridge Mode DISPATCH messages."""

    def test_bridge_dispatch_extends_dispatch(self):
        """BridgeDispatchCommand has all DispatchCommand fields."""
        cmd = BridgeDispatchCommand(
            plugin_name="plugin",
            file_path="/data/input.csv",
            sinks=[SinkConfig(topic="output", uri="parquet://out.parquet")],
            file_version_id=123,
        )
        # Has base fields
        assert cmd.plugin_name == "plugin"
        assert cmd.file_version_id == 123
        # Bridge fields are optional
        assert cmd.env_hash is None
        assert cmd.source_code is None

    def test_bridge_dispatch_with_bridge_fields(self):
        """BridgeDispatchCommand accepts bridge mode fields."""
        cmd = BridgeDispatchCommand(
            plugin_name="bridge_plugin",
            file_path="/data/input.csv",
            sinks=[SinkConfig(topic="output", uri="parquet://out.parquet")],
            file_version_id=456,
            env_hash="a" * 64,
            artifact_hash="b" * 64,
            source_code="from sdk import BasePlugin\nclass Handler(BasePlugin): pass",
        )
        assert cmd.env_hash == "a" * 64
        assert cmd.source_code is not None

    def test_msg_bridge_dispatch_packs_correctly(self):
        """msg_bridge_dispatch creates valid message frames."""
        sinks = [SinkConfig(topic="out", uri="parquet://test.parquet")]
        frames = msg_bridge_dispatch(
            job_id=999,
            plugin_name="test_plugin",
            file_path="/input.csv",
            sinks=sinks,
            file_version_id=100,
            env_hash="c" * 64,
            source_code="# plugin code",
        )

        opcode, job_id, _ = unpack_header(frames[0])
        assert opcode == OpCode.DISPATCH  # Uses same opcode as legacy
        assert job_id == 999

        data = json.loads(frames[1].decode("utf-8"))
        assert data["env_hash"] == "c" * 64
        assert data["source_code"] == "# plugin code"

    def test_bridge_dispatch_backward_compatible(self):
        """BridgeDispatchCommand can parse legacy DispatchCommand."""
        # Legacy payload (no bridge fields)
        legacy_payload = {
            "plugin_name": "legacy",
            "file_path": "/file.csv",
            "sinks": [{"topic": "out", "uri": "parquet://x.parquet", "mode": "append"}],
            "file_version_id": 1,
        }

        # Should parse without error
        cmd = BridgeDispatchCommand(**legacy_payload)
        assert cmd.env_hash is None
        assert cmd.source_code is None


class TestMessageUnpacking:
    """Tests for unpacking v5 messages."""

    def test_unpack_prepare_env(self):
        """unpack_msg correctly parses PREPARE_ENV."""
        frames = msg_prepare_env("a" * 64, "lockfile content")
        opcode, job_id, payload = unpack_msg(frames)

        assert opcode == OpCode.PREPARE_ENV
        assert payload["env_hash"] == "a" * 64
        assert payload["lockfile_content"] == "lockfile content"

    def test_unpack_env_ready(self):
        """unpack_msg correctly parses ENV_READY."""
        frames = msg_env_ready("b" * 64, "/path/python", cached=True)
        opcode, job_id, payload = unpack_msg(frames)

        assert opcode == OpCode.ENV_READY
        assert payload["cached"] is True

    def test_protocol_version_unchanged(self):
        """Protocol version remains 0x04 for compatibility."""
        assert PROTOCOL_VERSION == 0x04
