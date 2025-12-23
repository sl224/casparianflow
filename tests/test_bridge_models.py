# tests/test_bridge_models.py
"""
Tests for v5.0 Bridge Mode database models.

Tests:
- PluginEnvironment table and content-addressable storage
- Publisher table and identity management
- PluginManifest v5.0 fields (env_hash, artifact_hash, publisher_id)
- Relationships between models
"""

import pytest
from datetime import datetime
from sqlalchemy.orm import Session

from casparian_flow.db.models import (
    PluginEnvironment,
    Publisher,
    PluginManifest,
    PluginStatusEnum,
)
from casparian_flow.security import compute_artifact_hash, compute_env_hash


class TestPluginEnvironment:
    """Tests for the PluginEnvironment model (uv.lock storage)."""

    SAMPLE_LOCKFILE = """
version = 1
requires-python = ">=3.11"

[[package]]
name = "pandas"
version = "2.2.0"
"""

    def test_create_environment_with_hash_pk(self, test_db_session: Session):
        """Environment uses SHA256 hash as primary key."""
        env_hash = compute_env_hash(self.SAMPLE_LOCKFILE)

        env = PluginEnvironment(
            hash=env_hash,
            lockfile_content=self.SAMPLE_LOCKFILE,
            size_mb=50.0,
        )
        test_db_session.add(env)
        test_db_session.commit()

        # Query by hash
        result = test_db_session.get(PluginEnvironment, env_hash)
        assert result is not None
        assert result.lockfile_content == self.SAMPLE_LOCKFILE
        assert result.size_mb == 50.0

    def test_environment_hash_is_deterministic(self):
        """Same lockfile content produces same hash."""
        hash1 = compute_env_hash(self.SAMPLE_LOCKFILE)
        hash2 = compute_env_hash(self.SAMPLE_LOCKFILE)
        assert hash1 == hash2
        assert len(hash1) == 64  # SHA256 hex

    def test_different_lockfiles_produce_different_hashes(self):
        """Different content produces different hashes."""
        lockfile2 = self.SAMPLE_LOCKFILE.replace("2.2.0", "2.3.0")
        hash1 = compute_env_hash(self.SAMPLE_LOCKFILE)
        hash2 = compute_env_hash(lockfile2)
        assert hash1 != hash2

    def test_environment_deduplication(self, test_db_session: Session):
        """Multiple manifests can share the same environment."""
        env_hash = compute_env_hash(self.SAMPLE_LOCKFILE)

        # Create one environment
        env = PluginEnvironment(
            hash=env_hash,
            lockfile_content=self.SAMPLE_LOCKFILE,
        )
        test_db_session.add(env)
        test_db_session.flush()

        # Create two manifests referencing same environment
        manifest1 = PluginManifest(
            plugin_name="plugin_a",
            version="1.0.0",
            source_code="# plugin a",
            source_hash="a" * 64,
            env_hash=env_hash,
        )
        manifest2 = PluginManifest(
            plugin_name="plugin_b",
            version="1.0.0",
            source_code="# plugin b",
            source_hash="b" * 64,
            env_hash=env_hash,
        )
        test_db_session.add_all([manifest1, manifest2])
        test_db_session.commit()

        # Both manifests reference same environment
        assert manifest1.environment is manifest2.environment
        assert len(env.manifests) == 2


class TestPublisher:
    """Tests for the Publisher model (identity storage)."""

    def test_create_local_publisher(self, test_db_session: Session):
        """Create publisher without Azure OID (local mode)."""
        publisher = Publisher(
            name="local_user",
            email="user@localhost",
        )
        test_db_session.add(publisher)
        test_db_session.commit()

        assert publisher.id is not None
        assert publisher.azure_oid is None
        assert publisher.name == "local_user"

    def test_create_enterprise_publisher(self, test_db_session: Session):
        """Create publisher with Azure OID (enterprise mode)."""
        azure_oid = "12345678-1234-1234-1234-123456789012"
        publisher = Publisher(
            name="enterprise_user",
            email="user@company.com",
            azure_oid=azure_oid,
        )
        test_db_session.add(publisher)
        test_db_session.commit()

        result = test_db_session.query(Publisher).filter_by(azure_oid=azure_oid).first()
        assert result is not None
        assert result.name == "enterprise_user"

    def test_azure_oid_uniqueness(self, test_db_session: Session):
        """Azure OID must be unique across publishers."""
        azure_oid = "12345678-1234-1234-1234-123456789012"

        publisher1 = Publisher(name="user1", azure_oid=azure_oid)
        test_db_session.add(publisher1)
        test_db_session.commit()

        publisher2 = Publisher(name="user2", azure_oid=azure_oid)
        test_db_session.add(publisher2)

        with pytest.raises(Exception):  # IntegrityError
            test_db_session.commit()


class TestPluginManifestBridgeFields:
    """Tests for v5.0 PluginManifest fields."""

    SAMPLE_SOURCE = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        return None
"""
    SAMPLE_LOCKFILE = "version = 1\n"

    def test_manifest_with_bridge_mode_fields(self, test_db_session: Session):
        """Create manifest with all v5.0 fields."""
        env_hash = compute_env_hash(self.SAMPLE_LOCKFILE)
        artifact_hash = compute_artifact_hash(self.SAMPLE_SOURCE, self.SAMPLE_LOCKFILE)

        # Create environment first
        env = PluginEnvironment(hash=env_hash, lockfile_content=self.SAMPLE_LOCKFILE)
        test_db_session.add(env)

        # Create publisher
        publisher = Publisher(name="test_user")
        test_db_session.add(publisher)
        test_db_session.flush()

        # Create manifest with v5.0 fields
        manifest = PluginManifest(
            plugin_name="bridge_plugin",
            version="1.0.0",
            source_code=self.SAMPLE_SOURCE,
            source_hash="x" * 64,
            env_hash=env_hash,
            artifact_hash=artifact_hash,
            publisher_id=publisher.id,
            signature="sig" * 20,
            system_requirements='["glibc_2.31"]',
        )
        test_db_session.add(manifest)
        test_db_session.commit()

        # Verify relationships
        assert manifest.environment == env
        assert manifest.publisher == publisher
        assert manifest.artifact_hash == artifact_hash

    def test_manifest_legacy_mode_null_env_hash(self, test_db_session: Session):
        """Legacy plugins have NULL env_hash (run in host process)."""
        manifest = PluginManifest(
            plugin_name="legacy_plugin",
            version="1.0.0",
            source_code=self.SAMPLE_SOURCE,
            source_hash="y" * 64,
            env_hash=None,  # Legacy mode
        )
        test_db_session.add(manifest)
        test_db_session.commit()

        assert manifest.env_hash is None
        assert manifest.environment is None

    def test_artifact_hash_combines_source_and_lockfile(self):
        """Artifact hash is SHA256(source + lockfile)."""
        hash1 = compute_artifact_hash(self.SAMPLE_SOURCE, self.SAMPLE_LOCKFILE)
        hash2 = compute_artifact_hash(self.SAMPLE_SOURCE, "different lockfile")
        hash3 = compute_artifact_hash("different source", self.SAMPLE_LOCKFILE)

        assert len(hash1) == 64
        assert hash1 != hash2  # Different lockfile
        assert hash1 != hash3  # Different source

    def test_artifact_hash_empty_lockfile_for_legacy(self):
        """Legacy mode uses empty string for lockfile in hash."""
        hash_legacy = compute_artifact_hash(self.SAMPLE_SOURCE, "")
        hash_bridge = compute_artifact_hash(self.SAMPLE_SOURCE, self.SAMPLE_LOCKFILE)

        assert hash_legacy != hash_bridge
