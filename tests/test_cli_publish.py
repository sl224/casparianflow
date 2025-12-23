# tests/test_cli_publish.py
"""
Tests for v5.0 Bridge Mode CLI publish command.

Tests:
- Plugin validation during publish
- Lockfile discovery and generation
- Artifact hashing
- Plugin name extraction
- Dry run mode

Note: Network calls to Sentinel are mocked.
"""

import pytest
from pathlib import Path
from unittest.mock import patch, MagicMock

from casparian_flow.cli.publish import (
    publish_plugin,
    find_lockfile,
    extract_plugin_name,
    PublishError,
)
from casparian_flow.security import compute_artifact_hash, compute_env_hash


VALID_PLUGIN_CODE = '''
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    """A valid test plugin."""

    def execute(self, file_path):
        import pandas as pd
        return pd.read_csv(file_path)
'''

UNSAFE_PLUGIN_CODE = '''
import os  # Banned import
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        os.system("rm -rf /")  # Dangerous!
'''

PLUGIN_WITH_MANIFEST = '''
from casparian_flow.sdk import BasePlugin, PluginMetadata

MANIFEST = PluginMetadata(
    name="custom_name_plugin",
    pattern="*.csv",
    topic="output",
)

class Handler(BasePlugin):
    def execute(self, file_path):
        pass
'''

SAMPLE_LOCKFILE = '''
version = 1
requires-python = ">=3.11"

[[package]]
name = "pandas"
version = "2.2.0"
'''


class TestPublishValidation:
    """Tests for plugin validation during publish."""

    def test_rejects_unsafe_plugin(self, tmp_path: Path):
        """Publish fails for plugins with banned imports."""
        plugin_file = tmp_path / "bad_plugin.py"
        plugin_file.write_text(UNSAFE_PLUGIN_CODE)

        with pytest.raises(PublishError, match="validation failed"):
            publish_plugin(plugin_file, dry_run=True)

    def test_accepts_safe_plugin(self, tmp_path: Path):
        """Publish succeeds for safe plugins."""
        plugin_file = tmp_path / "good_plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        # Mock the identity provider with proper User and SignedArtifact objects
        from casparian_flow.security.identity import User, SignedArtifact

        with patch("casparian_flow.cli.publish.get_identity_provider") as mock_provider:
            mock_instance = MagicMock()
            mock_instance.authenticate.return_value = User(
                id=1, name="test_user", email="test@example.com", azure_oid=None
            )
            mock_instance.sign_artifact.return_value = SignedArtifact(
                artifact_hash="a" * 64, signature="b" * 128, public_key="c" * 64
            )
            mock_provider.return_value = mock_instance

            result = publish_plugin(plugin_file, dry_run=True)
            assert result["status"] == "DRY_RUN"

    def test_rejects_non_python_file(self, tmp_path: Path):
        """Publish fails for non-.py files."""
        txt_file = tmp_path / "not_python.txt"
        txt_file.write_text("hello")

        with pytest.raises(PublishError, match="must be a .py file"):
            publish_plugin(txt_file, dry_run=True)

    def test_rejects_missing_file(self, tmp_path: Path):
        """Publish fails for non-existent files."""
        with pytest.raises(PublishError, match="not found"):
            publish_plugin(tmp_path / "missing.py", dry_run=True)


class TestLockfileDiscovery:
    """Tests for lockfile discovery logic."""

    def test_finds_lockfile_in_same_directory(self, tmp_path: Path):
        """Finds uv.lock in plugin directory."""
        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        lockfile = tmp_path / "uv.lock"
        lockfile.write_text(SAMPLE_LOCKFILE)

        path, content = find_lockfile(plugin_file)
        assert path == lockfile
        assert content == SAMPLE_LOCKFILE

    def test_finds_lockfile_in_parent(self, tmp_path: Path):
        """Finds uv.lock in parent directory."""
        subdir = tmp_path / "subdir"
        subdir.mkdir()
        plugin_file = subdir / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        lockfile = tmp_path / "uv.lock"
        lockfile.write_text(SAMPLE_LOCKFILE)

        path, content = find_lockfile(plugin_file)
        assert path == lockfile

    def test_returns_empty_for_legacy_mode(self, tmp_path: Path):
        """Returns empty content when no lockfile found."""
        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        path, content = find_lockfile(plugin_file)
        assert path is None
        assert content == ""

    @patch("subprocess.run")
    def test_generates_lockfile_from_pyproject(self, mock_run, tmp_path: Path):
        """Runs uv lock when pyproject.toml found."""
        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        pyproject = tmp_path / "pyproject.toml"
        pyproject.write_text('[project]\nname = "test"\n')

        # Mock uv lock creating the lockfile
        def create_lockfile(*args, **kwargs):
            (tmp_path / "uv.lock").write_text(SAMPLE_LOCKFILE)
            return MagicMock(returncode=0, stdout="", stderr="")

        mock_run.side_effect = create_lockfile

        path, content = find_lockfile(plugin_file)
        assert path is not None
        assert content == SAMPLE_LOCKFILE


class TestPluginNameExtraction:
    """Tests for plugin name extraction."""

    def test_extracts_from_manifest(self, tmp_path: Path):
        """Extracts name from MANIFEST attribute."""
        plugin_file = tmp_path / "my_plugin.py"
        name = extract_plugin_name(PLUGIN_WITH_MANIFEST, plugin_file)
        assert name == "custom_name_plugin"

    def test_falls_back_to_filename(self, tmp_path: Path):
        """Uses filename when no MANIFEST."""
        plugin_file = tmp_path / "fallback_plugin.py"
        name = extract_plugin_name(VALID_PLUGIN_CODE, plugin_file)
        assert name == "fallback_plugin"


class TestArtifactHashing:
    """Tests for artifact hash computation during publish."""

    def test_hash_computed_for_bridge_mode(self, tmp_path: Path):
        """Artifact hash includes lockfile for bridge mode."""
        from casparian_flow.security.identity import User, SignedArtifact

        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        lockfile = tmp_path / "uv.lock"
        lockfile.write_text(SAMPLE_LOCKFILE)

        with patch("casparian_flow.cli.publish.get_identity_provider") as mock_provider:
            mock_instance = MagicMock()
            mock_instance.authenticate.return_value = User(
                id=0, name="user", email=None, azure_oid=None
            )
            mock_instance.sign_artifact.return_value = SignedArtifact(
                artifact_hash="a" * 64, signature="x" * 128, public_key="y" * 64
            )
            mock_provider.return_value = mock_instance

            result = publish_plugin(plugin_file, dry_run=True)

            # Env hash should be set (bridge mode)
            assert result["env_hash"] is not None
            assert len(result["env_hash"]) == 64

    def test_hash_empty_lockfile_for_legacy(self, tmp_path: Path):
        """Legacy mode has null env_hash."""
        from casparian_flow.security.identity import User, SignedArtifact

        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)
        # No lockfile = legacy mode

        with patch("casparian_flow.cli.publish.get_identity_provider") as mock_provider:
            mock_instance = MagicMock()
            mock_instance.authenticate.return_value = User(
                id=0, name="user", email=None, azure_oid=None
            )
            mock_instance.sign_artifact.return_value = SignedArtifact(
                artifact_hash="a" * 64, signature="x" * 128, public_key="y" * 64
            )
            mock_provider.return_value = mock_instance

            result = publish_plugin(plugin_file, dry_run=True)
            assert result["env_hash"] is None


class TestDryRun:
    """Tests for dry run mode."""

    def test_dry_run_skips_network(self, tmp_path: Path):
        """Dry run doesn't connect to Sentinel."""
        from casparian_flow.security.identity import User, SignedArtifact

        plugin_file = tmp_path / "plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        with patch("casparian_flow.cli.publish.get_identity_provider") as mock_provider:
            mock_instance = MagicMock()
            mock_instance.authenticate.return_value = User(
                id=0, name="user", email=None, azure_oid=None
            )
            mock_instance.sign_artifact.return_value = SignedArtifact(
                artifact_hash="a" * 64, signature="b" * 128, public_key="c" * 64
            )
            mock_provider.return_value = mock_instance

            with patch("zmq.Context") as mock_zmq:
                result = publish_plugin(plugin_file, dry_run=True)

                assert result["status"] == "DRY_RUN"
                mock_zmq.assert_not_called()

    def test_dry_run_returns_artifact_info(self, tmp_path: Path):
        """Dry run returns computed artifact info."""
        from casparian_flow.security.identity import User, SignedArtifact

        plugin_file = tmp_path / "test_plugin.py"
        plugin_file.write_text(VALID_PLUGIN_CODE)

        with patch("casparian_flow.cli.publish.get_identity_provider") as mock_provider:
            mock_instance = MagicMock()
            mock_instance.authenticate.return_value = User(
                id=0, name="dry_run_user", email=None, azure_oid=None
            )
            mock_instance.sign_artifact.return_value = SignedArtifact(
                artifact_hash="a" * 64, signature="b" * 128, public_key="c" * 64
            )
            mock_provider.return_value = mock_instance

            result = publish_plugin(plugin_file, version="2.0.0", dry_run=True)

            assert result["plugin_name"] == "test_plugin"
            assert result["version"] == "2.0.0"
            assert result["publisher"] == "dry_run_user"
            assert "artifact_hash" in result
