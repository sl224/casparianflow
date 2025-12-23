# src/casparian_flow/engine/venv_manager.py
"""
v5.0 Bridge Mode: Virtual Environment Manager.

Responsible for the "Physical Layer" of dependencies:
- Creates isolated venvs per environment hash
- Uses `uv` for fast, reproducible installs
- Implements LRU eviction for disk space management
- Supports hardlinking for deduplication

Design Principles:
- Eager Provisioning via OpCode.PREPARE_ENV to avoid network blocking
- Content-addressable storage: ~/.casparian_flow/venvs/{env_hash}/
- Lockfile-based determinism: `uv sync --frozen --no-dev`
"""

import os
import shutil
import subprocess
import logging
from pathlib import Path
from typing import Optional, Tuple
from datetime import datetime
import json

logger = logging.getLogger(__name__)


class VenvManagerError(Exception):
    """Raised when venv operations fail."""
    pass


class VenvManager:
    """
    Manages isolated virtual environments for Bridge Mode execution.

    Features:
    - Content-addressable venv storage by env_hash
    - Eager provisioning for zero-latency execution
    - LRU-based eviction for disk management
    - uv-based installation for speed and reproducibility
    """

    DEFAULT_VENVS_DIR = Path.home() / ".casparian_flow" / "venvs"
    UV_TIMEOUT_SECONDS = 300  # 5 minutes for dependency resolution

    def __init__(
        self,
        venvs_dir: Optional[Path] = None,
        max_cache_size_gb: float = 10.0,
        uv_path: Optional[str] = None,
    ):
        """
        Initialize the VenvManager.

        Args:
            venvs_dir: Base directory for venv storage
            max_cache_size_gb: Maximum cache size before LRU eviction
            uv_path: Path to uv binary (auto-detected if not provided)
        """
        self.venvs_dir = venvs_dir or self.DEFAULT_VENVS_DIR
        self.venvs_dir.mkdir(parents=True, exist_ok=True)

        self.max_cache_size_bytes = int(max_cache_size_gb * 1024 * 1024 * 1024)
        self.uv_path = uv_path or self._find_uv()

        # Metadata file for tracking venv usage
        self.metadata_path = self.venvs_dir / ".metadata.json"
        self._metadata = self._load_metadata()

    def _find_uv(self) -> str:
        """Find the uv binary in PATH or common locations."""
        # Check PATH
        uv_in_path = shutil.which("uv")
        if uv_in_path:
            return uv_in_path

        # Check common locations
        common_paths = [
            Path.home() / ".cargo" / "bin" / "uv",
            Path.home() / ".local" / "bin" / "uv",
            Path("/usr/local/bin/uv"),
        ]

        for path in common_paths:
            if path.exists():
                return str(path)

        raise VenvManagerError(
            "uv binary not found. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh"
        )

    def _load_metadata(self) -> dict:
        """Load venv metadata from disk."""
        if self.metadata_path.exists():
            try:
                return json.loads(self.metadata_path.read_text())
            except (json.JSONDecodeError, IOError):
                logger.warning("Failed to load venv metadata, starting fresh")
        return {"venvs": {}}

    def _save_metadata(self):
        """Persist venv metadata to disk."""
        try:
            self.metadata_path.write_text(json.dumps(self._metadata, indent=2))
        except IOError as e:
            logger.warning(f"Failed to save venv metadata: {e}")

    def get_venv_path(self, env_hash: str) -> Path:
        """Get the path for a venv by its environment hash."""
        return self.venvs_dir / env_hash

    def get_interpreter_path(self, env_hash: str) -> Path:
        """Get the Python interpreter path for a venv."""
        venv_path = self.get_venv_path(env_hash)
        if os.name == "nt":  # Windows
            return venv_path / "Scripts" / "python.exe"
        return venv_path / "bin" / "python"

    def exists(self, env_hash: str) -> bool:
        """Check if a venv exists for the given environment hash."""
        interpreter = self.get_interpreter_path(env_hash)
        return interpreter.exists()

    def get_or_create_env(
        self,
        env_hash: str,
        lockfile_content: str,
        python_version: Optional[str] = None,
    ) -> Path:
        """
        Get or create a virtual environment for the given lockfile.

        This is the main entry point for venv provisioning.

        Args:
            env_hash: SHA256 hash of the lockfile content
            lockfile_content: Raw TOML content of uv.lock
            python_version: Optional Python version constraint (e.g., "3.11")

        Returns:
            Path to the Python interpreter in the venv

        Raises:
            VenvManagerError: If venv creation fails
        """
        venv_path = self.get_venv_path(env_hash)
        interpreter_path = self.get_interpreter_path(env_hash)

        # Cache hit
        if interpreter_path.exists():
            logger.info(f"VenvManager: Cache hit for env {env_hash[:12]}")
            self._update_last_used(env_hash)
            return interpreter_path

        # Cache miss - create new venv
        logger.info(f"VenvManager: Cache miss for env {env_hash[:12]}, creating...")

        try:
            # Create the venv directory
            venv_path.mkdir(parents=True, exist_ok=True)

            # Write the lockfile
            lockfile_path = venv_path / "uv.lock"
            lockfile_path.write_text(lockfile_content)

            # Create a minimal pyproject.toml for uv
            pyproject_content = self._generate_pyproject(python_version)
            pyproject_path = venv_path / "pyproject.toml"
            pyproject_path.write_text(pyproject_content)

            # Create venv with uv
            self._create_venv(venv_path, python_version)

            # Install dependencies
            self._sync_dependencies(venv_path)

            # Update metadata
            self._record_venv(env_hash, lockfile_content)

            # Check if eviction is needed
            self._maybe_evict()

            logger.info(f"VenvManager: Created venv for env {env_hash[:12]}")
            return interpreter_path

        except Exception as e:
            # Clean up failed venv
            if venv_path.exists():
                shutil.rmtree(venv_path, ignore_errors=True)
            raise VenvManagerError(f"Failed to create venv: {e}") from e

    def _generate_pyproject(self, python_version: Optional[str] = None) -> str:
        """Generate a minimal pyproject.toml for uv sync."""
        python_constraint = f">={python_version}" if python_version else ">=3.10"
        return f'''[project]
name = "casparian-bridge-env"
version = "0.0.1"
requires-python = "{python_constraint}"
dependencies = []

[tool.uv]
# Dependencies are fully specified in uv.lock
'''

    def _create_venv(self, venv_path: Path, python_version: Optional[str] = None):
        """Create a new virtual environment using uv."""
        cmd = [self.uv_path, "venv", str(venv_path / ".venv")]

        if python_version:
            cmd.extend(["--python", python_version])

        result = subprocess.run(
            cmd,
            cwd=venv_path,
            capture_output=True,
            text=True,
            timeout=60,
        )

        if result.returncode != 0:
            raise VenvManagerError(
                f"uv venv failed: {result.stderr or result.stdout}"
            )

        # Move .venv contents up to venv_path for cleaner structure
        dot_venv = venv_path / ".venv"
        if dot_venv.exists():
            for item in dot_venv.iterdir():
                shutil.move(str(item), str(venv_path / item.name))
            dot_venv.rmdir()

    def _sync_dependencies(self, venv_path: Path):
        """Install dependencies using uv sync with frozen lockfile."""
        cmd = [
            self.uv_path,
            "sync",
            "--frozen",  # Use exact versions from lockfile
            "--no-dev",  # Skip dev dependencies
        ]

        # Set environment to use our venv
        env = os.environ.copy()
        env["VIRTUAL_ENV"] = str(venv_path)

        result = subprocess.run(
            cmd,
            cwd=venv_path,
            capture_output=True,
            text=True,
            timeout=self.UV_TIMEOUT_SECONDS,
            env=env,
        )

        if result.returncode != 0:
            raise VenvManagerError(
                f"uv sync failed: {result.stderr or result.stdout}"
            )

    def _record_venv(self, env_hash: str, lockfile_content: str):
        """Record venv creation in metadata."""
        venv_path = self.get_venv_path(env_hash)
        size_bytes = self._get_dir_size(venv_path)

        self._metadata["venvs"][env_hash] = {
            "created_at": datetime.now().isoformat(),
            "last_used": datetime.now().isoformat(),
            "size_bytes": size_bytes,
            "lockfile_hash": env_hash,
        }
        self._save_metadata()

    def _update_last_used(self, env_hash: str):
        """Update the last_used timestamp for a venv."""
        if env_hash in self._metadata["venvs"]:
            self._metadata["venvs"][env_hash]["last_used"] = datetime.now().isoformat()
            self._save_metadata()

    def _get_dir_size(self, path: Path) -> int:
        """Calculate the total size of a directory."""
        total = 0
        for entry in path.rglob("*"):
            if entry.is_file():
                total += entry.stat().st_size
        return total

    def _maybe_evict(self):
        """Evict oldest venvs if cache size exceeds limit."""
        total_size = sum(
            v.get("size_bytes", 0) for v in self._metadata["venvs"].values()
        )

        if total_size <= self.max_cache_size_bytes:
            return

        logger.info(
            f"VenvManager: Cache size ({total_size / 1e9:.2f} GB) exceeds limit, evicting..."
        )

        # Sort by last_used (oldest first)
        sorted_venvs = sorted(
            self._metadata["venvs"].items(),
            key=lambda x: x[1].get("last_used", ""),
        )

        for env_hash, info in sorted_venvs:
            if total_size <= self.max_cache_size_bytes * 0.8:  # Evict to 80%
                break

            venv_path = self.get_venv_path(env_hash)
            if venv_path.exists():
                size = info.get("size_bytes", 0)
                shutil.rmtree(venv_path, ignore_errors=True)
                del self._metadata["venvs"][env_hash]
                total_size -= size
                logger.info(f"VenvManager: Evicted env {env_hash[:12]}")

        self._save_metadata()

    def delete_env(self, env_hash: str):
        """Delete a specific venv."""
        venv_path = self.get_venv_path(env_hash)
        if venv_path.exists():
            shutil.rmtree(venv_path, ignore_errors=True)
        if env_hash in self._metadata["venvs"]:
            del self._metadata["venvs"][env_hash]
            self._save_metadata()
        logger.info(f"VenvManager: Deleted env {env_hash[:12]}")

    def list_envs(self) -> list[dict]:
        """List all cached venvs with metadata."""
        return [
            {"env_hash": k, **v}
            for k, v in self._metadata["venvs"].items()
        ]

    def get_cache_stats(self) -> dict:
        """Get cache statistics."""
        venvs = self._metadata["venvs"]
        total_size = sum(v.get("size_bytes", 0) for v in venvs.values())
        return {
            "count": len(venvs),
            "total_size_bytes": total_size,
            "total_size_gb": total_size / (1024 ** 3),
            "max_size_gb": self.max_cache_size_bytes / (1024 ** 3),
            "utilization_percent": (total_size / self.max_cache_size_bytes) * 100,
        }


# Singleton instance
_venv_manager: Optional[VenvManager] = None


def get_venv_manager() -> VenvManager:
    """Get the singleton VenvManager instance."""
    global _venv_manager
    if _venv_manager is None:
        _venv_manager = VenvManager()
    return _venv_manager
