# tests/test_architect.py
"""
Tests for ArchitectService deployment workflow.

Tests the full plugin deployment lifecycle: signature verification,
safety validation, persistence, sandbox testing, and promotion to ACTIVE.
"""
import pytest
import json
from unittest.mock import Mock, patch
from pathlib import Path

from casparian_flow.services.architect import (
    ArchitectService,
    DeploymentResult,
    handle_deploy_message,
)
from casparian_flow.db.models import PluginManifest, PluginStatusEnum
from casparian_flow.security.gatekeeper import (
    generate_signature,
    compute_source_hash,
)


class TestDeploymentWorkflow:
    """Test full deployment lifecycle."""

    def test_deploy_plugin_success(
        self, test_db_engine, test_db_session, sample_plugin_code
    ):
        """Full happy path: signature → validation → staging → active."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        # Deploy without sandbox test
        result = architect.deploy_plugin(
            plugin_name="test_plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=signature,
            sample_input=None,  # Skip sandbox
        )

        assert result.success is True
        assert result.plugin_name == "test_plugin"
        assert result.version == "1.0.0"
        assert result.error_message is None
        assert result.manifest_id is not None

        # Verify manifest in database
        manifest = test_db_session.get(PluginManifest, result.manifest_id)
        assert manifest is not None
        assert manifest.plugin_name == "test_plugin"
        assert manifest.status == PluginStatusEnum.ACTIVE
        assert manifest.source_code == sample_plugin_code
        assert manifest.deployed_at is not None

    def test_deploy_invalid_signature(self, test_db_engine, sample_plugin_code):
        """Reject if HMAC fails."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        wrong_signature = "0" * 64  # Invalid signature

        result = architect.deploy_plugin(
            plugin_name="bad_plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=wrong_signature,
        )

        assert result.success is False
        assert "signature" in result.error_message.lower()
        assert result.manifest_id is None

    def test_deploy_unsafe_code(
        self, test_db_engine, test_db_session, dangerous_plugin_code
    ):
        """Reject dangerous imports, save as REJECTED."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(dangerous_plugin_code, "test-secret-key")

        result = architect.deploy_plugin(
            plugin_name="dangerous_plugin",
            version="1.0.0",
            source_code=dangerous_plugin_code,
            signature=signature,
        )

        assert result.success is False
        assert result.error_message is not None

        # Verify REJECTED status in database
        manifests = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="dangerous_plugin")
            .all()
        )
        assert len(manifests) == 1
        assert manifests[0].status == PluginStatusEnum.REJECTED
        assert manifests[0].validation_error is not None

    def test_deploy_duplicate_hash(
        self, test_db_engine, test_db_session, sample_plugin_code
    ):
        """Reject duplicate source_hash."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        # Deploy first time
        result1 = architect.deploy_plugin(
            plugin_name="plugin1",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=signature,
        )
        assert result1.success is True

        # Try to deploy same code again (different name)
        result2 = architect.deploy_plugin(
            plugin_name="plugin2",
            version="1.0.0",
            source_code=sample_plugin_code,  # Same code!
            signature=signature,
        )

        assert result2.success is False
        assert "duplicate" in result2.error_message.lower()

    def test_deploy_without_sandbox(
        self, test_db_engine, test_db_session, sample_plugin_code
    ):
        """Skip sandbox if no sample_input provided."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        result = architect.deploy_plugin(
            plugin_name="plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=signature,
            sample_input=None,  # No sandbox test
        )

        assert result.success is True
        assert result.manifest_id is not None

        manifest = test_db_session.get(PluginManifest, result.manifest_id)
        assert manifest.status == PluginStatusEnum.ACTIVE


class TestSandboxTesting:
    """Test sandbox test functionality."""

    @patch("casparian_flow.services.architect.subprocess.Popen")
    def test_sandbox_test_success(
        self, mock_popen, test_db_engine, sample_plugin_code
    ):
        """Verify temp sidecar spawns and completes."""
        # Mock successful sidecar execution
        mock_proc = Mock()
        mock_proc.poll.return_value = None  # Still running
        mock_proc.terminate.return_value = None
        mock_proc.wait.return_value = None
        mock_popen.return_value = mock_proc

        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        result = architect.deploy_plugin(
            plugin_name="plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=signature,
            sample_input="/tmp/test.csv",  # Trigger sandbox
        )

        # Verify subprocess was called
        assert mock_popen.called
        assert result.success is True

    @patch("casparian_flow.services.architect.subprocess.Popen")
    def test_sandbox_test_crash(
        self, mock_popen, test_db_engine, test_db_session, sample_plugin_code
    ):
        """Detect immediate sidecar crash."""
        # Mock crashed sidecar
        mock_proc = Mock()
        mock_proc.poll.return_value = 1  # Crashed immediately
        mock_proc.communicate.return_value = (b"", b"ImportError: module not found")
        mock_popen.return_value = mock_proc

        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        result = architect.deploy_plugin(
            plugin_name="crash_plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=signature,
            sample_input="/tmp/test.csv",
        )

        assert result.success is False
        assert "crashed" in result.error_message.lower()

        # Verify REJECTED in database
        manifest = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="crash_plugin")
            .first()
        )
        assert manifest.status == PluginStatusEnum.REJECTED

    @patch("casparian_flow.services.architect.tempfile.NamedTemporaryFile")
    @patch("casparian_flow.services.architect.subprocess.Popen")
    def test_sandbox_test_cleanup(
        self, mock_popen, mock_tempfile, test_db_engine, sample_plugin_code
    ):
        """Temp files deleted after test."""
        # Mock temp file
        mock_file = Mock()
        mock_file.name = "/tmp/plugin_xyz.py"
        mock_file.__enter__ = Mock(return_value=mock_file)
        mock_file.__exit__ = Mock(return_value=False)
        mock_tempfile.return_value = mock_file

        # Mock successful sidecar
        mock_proc = Mock()
        mock_proc.poll.return_value = None
        mock_proc.terminate.return_value = None
        mock_proc.wait.return_value = None
        mock_popen.return_value = mock_proc

        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        with patch("pathlib.Path.unlink") as mock_unlink:
            architect.deploy_plugin(
                plugin_name="plugin",
                version="1.0.0",
                source_code=sample_plugin_code,
                signature=signature,
                sample_input="/tmp/test.csv",
            )

            # Verify cleanup was called
            assert mock_unlink.called


class TestHandleDeployMessage:
    """Test handle_deploy_message function."""

    def test_handle_deploy_message(self, test_db_engine, sample_plugin_code):
        """Parse JSON payload from OpCode.DEPLOY."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        signature = generate_signature(sample_plugin_code, "test-secret-key")

        payload = {
            "plugin_name": "json_plugin",
            "version": "1.0.0",
            "source_code": sample_plugin_code,
            "signature": signature,
        }
        payload_bytes = json.dumps(payload).encode("utf-8")

        result = handle_deploy_message(architect, payload_bytes)

        assert isinstance(result, DeploymentResult)
        assert result.success is True
        assert result.plugin_name == "json_plugin"

    def test_handle_deploy_message_invalid_json(self, test_db_engine):
        """Handle malformed JSON gracefully."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        bad_payload = b"not json at all"

        result = handle_deploy_message(architect, bad_payload)

        assert result.success is False
        assert "parsing error" in result.error_message.lower()

    def test_handle_deploy_message_missing_fields(self, test_db_engine):
        """Handle missing required fields."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        incomplete_payload = {
            "plugin_name": "incomplete",
            # Missing source_code and signature
        }
        payload_bytes = json.dumps(incomplete_payload).encode("utf-8")

        result = handle_deploy_message(architect, payload_bytes)

        assert result.success is False


class TestDeploymentResult:
    """Test DeploymentResult dataclass."""

    def test_deployment_result_success(self):
        """Test successful deployment result."""
        result = DeploymentResult(
            success=True,
            plugin_name="test",
            version="1.0.0",
            manifest_id=123,
        )

        assert result.success is True
        assert result.plugin_name == "test"
        assert result.version == "1.0.0"
        assert result.manifest_id == 123
        assert result.error_message is None

    def test_deployment_result_failure(self):
        """Test failed deployment result."""
        result = DeploymentResult(
            success=False,
            plugin_name="bad",
            version="1.0.0",
            error_message="Validation failed",
        )

        assert result.success is False
        assert result.error_message == "Validation failed"
        assert result.manifest_id is None


class TestArchitectServiceInit:
    """Test ArchitectService initialization."""

    def test_architect_service_init(self, test_db_engine):
        """Test service initialization."""
        architect = ArchitectService(test_db_engine, "my-secret-key")

        assert architect.engine == test_db_engine
        assert architect.secret_key == "my-secret-key"


class TestComplexScenarios:
    """Test complex deployment scenarios."""

    def test_multiple_deployments_same_plugin(
        self, test_db_engine, test_db_session, sample_plugin_code
    ):
        """Deploy multiple versions of same plugin."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        versions = ["1.0.0", "1.1.0", "2.0.0"]

        for version in versions:
            # Modify code slightly to get different hash
            code = sample_plugin_code + f"\n# Version {version}"
            signature = generate_signature(code, "test-secret-key")

            result = architect.deploy_plugin(
                plugin_name="multi_version_plugin",
                version=version,
                source_code=code,
                signature=signature,
            )

            assert result.success is True

        # Verify all versions exist
        all_versions = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="multi_version_plugin")
            .all()
        )

        assert len(all_versions) == 3
        assert {v.version for v in all_versions} == set(versions)

    def test_deploy_after_rejection(
        self, test_db_engine, test_db_session, dangerous_plugin_code, sample_plugin_code
    ):
        """Deploy safe version after rejection."""
        architect = ArchitectService(test_db_engine, "test-secret-key")

        # First attempt: dangerous code
        bad_sig = generate_signature(dangerous_plugin_code, "test-secret-key")
        result1 = architect.deploy_plugin(
            plugin_name="fixed_plugin",
            version="0.1.0",
            source_code=dangerous_plugin_code,
            signature=bad_sig,
        )
        assert result1.success is False

        # Second attempt: fixed safe code
        good_sig = generate_signature(sample_plugin_code, "test-secret-key")
        result2 = architect.deploy_plugin(
            plugin_name="fixed_plugin",
            version="1.0.0",
            source_code=sample_plugin_code,
            signature=good_sig,
        )
        assert result2.success is True

        # Verify both versions exist
        all_versions = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="fixed_plugin")
            .all()
        )

        assert len(all_versions) == 2
        assert any(v.status == PluginStatusEnum.REJECTED for v in all_versions)
        assert any(v.status == PluginStatusEnum.ACTIVE for v in all_versions)
