# tests/test_venv_manager.py
"""
Tests for v5.0 Bridge Mode VenvManager.

Tests:
- Venv path generation and interpreter lookup
- Cache hit/miss logic
- Metadata tracking and LRU eviction
- Environment hash-based storage

Note: Subprocess calls to `uv` are mocked to avoid actual venv creation.
"""

import pytest
import json
import os
import shutil
from pathlib import Path
from unittest.mock import patch, MagicMock
from datetime import datetime

from casparian_flow.engine.venv_manager import (
    VenvManager,
    VenvManagerError,
    get_venv_manager,
)
from casparian_flow.security import compute_env_hash


class TestVenvManagerPaths:
    """Tests for path generation and lookup."""

    def test_venv_path_uses_env_hash(self, tmp_path: Path):
        """Venv directory is named by environment hash."""
        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = "a" * 64

        path = manager.get_venv_path(env_hash)
        assert path == tmp_path / env_hash

    def test_interpreter_path_unix(self, tmp_path: Path):
        """Interpreter path on Unix is bin/python."""
        if os.name == "nt":
            pytest.skip("Unix-specific test")

        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = "b" * 64

        interp = manager.get_interpreter_path(env_hash)
        assert interp == tmp_path / env_hash / "bin" / "python"

    def test_interpreter_path_windows(self, tmp_path: Path):
        """Interpreter path on Windows is Scripts/python.exe."""
        if os.name != "nt":
            pytest.skip("Windows-specific test")

        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = "c" * 64

        interp = manager.get_interpreter_path(env_hash)
        assert interp == tmp_path / env_hash / "Scripts" / "python.exe"

    def test_exists_returns_false_for_missing_venv(self, tmp_path: Path):
        """exists() returns False when venv doesn't exist."""
        manager = VenvManager(venvs_dir=tmp_path)
        assert manager.exists("nonexistent" + "0" * 55) is False

    def test_exists_returns_true_for_existing_interpreter(self, tmp_path: Path):
        """exists() returns True when interpreter file exists."""
        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = "d" * 64

        # Create fake interpreter
        interp_path = manager.get_interpreter_path(env_hash)
        interp_path.parent.mkdir(parents=True)
        interp_path.touch()

        assert manager.exists(env_hash) is True


class TestVenvManagerCaching:
    """Tests for cache behavior and metadata."""

    SAMPLE_LOCKFILE = "version = 1\n[[package]]\nname = 'test'\n"

    def test_cache_hit_returns_existing_interpreter(self, tmp_path: Path):
        """When venv exists, get_or_create returns path without calling uv."""
        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = compute_env_hash(self.SAMPLE_LOCKFILE)

        # Pre-create the venv structure
        interp_path = manager.get_interpreter_path(env_hash)
        interp_path.parent.mkdir(parents=True)
        interp_path.touch()

        # Should return immediately without subprocess
        with patch("subprocess.run") as mock_run:
            result = manager.get_or_create_env(env_hash, self.SAMPLE_LOCKFILE)
            mock_run.assert_not_called()

        assert result == interp_path

    @patch("subprocess.run")
    def test_cache_miss_calls_uv(self, mock_run, tmp_path: Path):
        """When venv doesn't exist, calls uv to create it."""
        mock_run.return_value = MagicMock(returncode=0, stdout="", stderr="")

        # Mock uv binary location
        with patch.object(VenvManager, "_find_uv", return_value="/usr/bin/uv"):
            manager = VenvManager(venvs_dir=tmp_path)
            env_hash = compute_env_hash(self.SAMPLE_LOCKFILE)

            # This will fail because uv isn't actually creating files,
            # but we can verify uv was called
            try:
                manager.get_or_create_env(env_hash, self.SAMPLE_LOCKFILE)
            except VenvManagerError:
                pass  # Expected - interpreter won't exist

            # Verify uv venv was called
            calls = [str(c) for c in mock_run.call_args_list]
            assert any("uv" in c and "venv" in c for c in calls)

    def test_metadata_file_created(self, tmp_path: Path):
        """Metadata file is created in venvs directory."""
        manager = VenvManager(venvs_dir=tmp_path)
        assert manager.metadata_path == tmp_path / ".metadata.json"

    def test_metadata_persisted_on_save(self, tmp_path: Path):
        """Metadata is written to disk."""
        manager = VenvManager(venvs_dir=tmp_path)
        manager._metadata = {"venvs": {"test": {"size_bytes": 100}}}
        manager._save_metadata()

        content = json.loads(manager.metadata_path.read_text())
        assert content["venvs"]["test"]["size_bytes"] == 100

    def test_metadata_loaded_on_init(self, tmp_path: Path):
        """Existing metadata is loaded on initialization."""
        metadata = {"venvs": {"existing": {"size_bytes": 500}}}
        (tmp_path / ".metadata.json").write_text(json.dumps(metadata))

        manager = VenvManager(venvs_dir=tmp_path)
        assert "existing" in manager._metadata["venvs"]


class TestVenvManagerEviction:
    """Tests for LRU cache eviction."""

    def test_list_envs_returns_metadata(self, tmp_path: Path):
        """list_envs() returns all cached environments."""
        manager = VenvManager(venvs_dir=tmp_path)
        manager._metadata = {
            "venvs": {
                "hash1": {"size_bytes": 100, "last_used": "2024-01-01"},
                "hash2": {"size_bytes": 200, "last_used": "2024-01-02"},
            }
        }

        envs = manager.list_envs()
        assert len(envs) == 2
        assert any(e["env_hash"] == "hash1" for e in envs)

    def test_cache_stats(self, tmp_path: Path):
        """get_cache_stats() returns utilization info."""
        manager = VenvManager(venvs_dir=tmp_path, max_cache_size_gb=1.0)
        manager._metadata = {
            "venvs": {
                "hash1": {"size_bytes": 500_000_000},  # 500MB
            }
        }

        stats = manager.get_cache_stats()
        assert stats["count"] == 1
        assert stats["total_size_bytes"] == 500_000_000
        assert stats["max_size_gb"] == 1.0
        assert 45 < stats["utilization_percent"] < 55  # ~50%

    def test_delete_env_removes_directory_and_metadata(self, tmp_path: Path):
        """delete_env() removes both filesystem and metadata."""
        manager = VenvManager(venvs_dir=tmp_path)
        env_hash = "e" * 64

        # Create fake venv
        venv_path = manager.get_venv_path(env_hash)
        venv_path.mkdir()
        (venv_path / "dummy.txt").touch()

        manager._metadata = {"venvs": {env_hash: {"size_bytes": 100}}}

        manager.delete_env(env_hash)

        assert not venv_path.exists()
        assert env_hash not in manager._metadata["venvs"]


class TestVenvManagerUvIntegration:
    """Tests for uv binary detection."""

    def test_find_uv_raises_when_not_found(self, tmp_path: Path, monkeypatch):
        """Raises VenvManagerError if uv not in PATH or common locations."""
        # Mock shutil.which to return None (uv not found)
        with patch("shutil.which", return_value=None):
            # Also mock common paths to not exist
            with patch.object(Path, "exists", return_value=False):
                with pytest.raises(VenvManagerError, match="uv binary not found"):
                    VenvManager(venvs_dir=tmp_path)

    def test_find_uv_uses_provided_path(self, tmp_path: Path):
        """Uses explicitly provided uv_path."""
        fake_uv = tmp_path / "my_uv"
        fake_uv.touch()

        manager = VenvManager(venvs_dir=tmp_path, uv_path=str(fake_uv))
        assert manager.uv_path == str(fake_uv)


class TestVenvManagerSingleton:
    """Tests for singleton accessor."""

    def test_get_venv_manager_returns_instance(self):
        """get_venv_manager() returns a VenvManager instance."""
        # Note: This uses the default venvs_dir, so may fail if uv not installed
        try:
            manager = get_venv_manager()
            assert isinstance(manager, VenvManager)
        except VenvManagerError:
            pytest.skip("uv not installed")
