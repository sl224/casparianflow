# tests/test_plugin_manifest.py
"""
Tests for PluginManifest database model and lifecycle.

Tests the PluginManifest table, status transitions, unique constraints,
and query performance.
"""
import pytest
from datetime import datetime
from sqlalchemy.exc import IntegrityError
from sqlalchemy import select

from casparian_flow.db.models import PluginManifest, PluginStatusEnum
from casparian_flow.security.gatekeeper import compute_source_hash


SAMPLE_PLUGIN_CODE = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        pass
"""

DIFFERENT_PLUGIN_CODE = """
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path):
        print("different")
"""


class TestManifestCreation:
    """Test creating PluginManifest records."""

    def test_manifest_creation(self, test_db_session):
        """Create manifest with all fields."""
        source_hash = compute_source_hash(SAMPLE_PLUGIN_CODE)

        manifest = PluginManifest(
            plugin_name="test_plugin",
            version="1.0.0",
            source_code=SAMPLE_PLUGIN_CODE,
            source_hash=source_hash,
            status=PluginStatusEnum.PENDING,
            signature="test_signature",
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        assert manifest.id is not None
        assert manifest.plugin_name == "test_plugin"
        assert manifest.version == "1.0.0"
        assert manifest.source_code == SAMPLE_PLUGIN_CODE
        assert manifest.source_hash == source_hash
        assert manifest.status == PluginStatusEnum.PENDING
        assert manifest.signature == "test_signature"
        assert manifest.created_at is not None
        assert manifest.deployed_at is None
        assert manifest.validation_error is None

    def test_manifest_defaults(self, test_db_session):
        """Test default values."""
        manifest = PluginManifest(
            plugin_name="minimal_plugin",
            version="0.1.0",
            source_code="code",
            source_hash="abc123",
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        assert manifest.status == PluginStatusEnum.PENDING  # Default status
        assert manifest.created_at is not None  # Auto-generated
        assert manifest.deployed_at is None
        assert manifest.signature is None
        assert manifest.validation_error is None


class TestStatusTransitions:
    """Test PluginManifest status lifecycle."""

    def test_pending_to_staging_transition(self, test_db_session):
        """PENDING → STAGING."""
        manifest = PluginManifest(
            plugin_name="plugin",
            version="1.0.0",
            source_code="code",
            source_hash="hash1",
            status=PluginStatusEnum.PENDING,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        # Transition to STAGING
        manifest.status = PluginStatusEnum.STAGING
        test_db_session.commit()

        assert manifest.status == PluginStatusEnum.STAGING

    def test_staging_to_active_transition(self, test_db_session):
        """STAGING → ACTIVE."""
        manifest = PluginManifest(
            plugin_name="plugin",
            version="1.0.0",
            source_code="code",
            source_hash="hash2",
            status=PluginStatusEnum.STAGING,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        # Transition to ACTIVE
        manifest.status = PluginStatusEnum.ACTIVE
        manifest.deployed_at = datetime.now()
        test_db_session.commit()

        assert manifest.status == PluginStatusEnum.ACTIVE
        assert manifest.deployed_at is not None

    def test_pending_to_rejected_transition(self, test_db_session):
        """PENDING → REJECTED with error message."""
        manifest = PluginManifest(
            plugin_name="bad_plugin",
            version="1.0.0",
            source_code="bad code",
            source_hash="hash3",
            status=PluginStatusEnum.PENDING,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        # Reject with error message
        manifest.status = PluginStatusEnum.REJECTED
        manifest.validation_error = "Dangerous import detected: os"
        test_db_session.commit()

        assert manifest.status == PluginStatusEnum.REJECTED
        assert manifest.validation_error == "Dangerous import detected: os"
        assert manifest.deployed_at is None


class TestSourceHashUniqueness:
    """Test source_hash unique constraint."""

    def test_source_hash_uniqueness(self, test_db_session):
        """Duplicate hash should fail."""
        hash_val = compute_source_hash(SAMPLE_PLUGIN_CODE)

        # Create first manifest
        manifest1 = PluginManifest(
            plugin_name="plugin1",
            version="1.0.0",
            source_code=SAMPLE_PLUGIN_CODE,
            source_hash=hash_val,
        )
        test_db_session.add(manifest1)
        test_db_session.commit()

        # Try to create second manifest with same hash
        manifest2 = PluginManifest(
            plugin_name="plugin2",
            version="2.0.0",
            source_code=SAMPLE_PLUGIN_CODE,
            source_hash=hash_val,  # Same hash!
        )
        test_db_session.add(manifest2)

        with pytest.raises(IntegrityError):
            test_db_session.commit()

    def test_different_hashes_allowed(self, test_db_session):
        """Different hashes can coexist."""
        hash1 = compute_source_hash(SAMPLE_PLUGIN_CODE)
        hash2 = compute_source_hash(DIFFERENT_PLUGIN_CODE)

        manifest1 = PluginManifest(
            plugin_name="plugin1",
            version="1.0.0",
            source_code=SAMPLE_PLUGIN_CODE,
            source_hash=hash1,
        )
        manifest2 = PluginManifest(
            plugin_name="plugin2",
            version="1.0.0",
            source_code=DIFFERENT_PLUGIN_CODE,
            source_hash=hash2,
        )

        test_db_session.add_all([manifest1, manifest2])
        test_db_session.commit()

        assert manifest1.id != manifest2.id
        assert manifest1.source_hash != manifest2.source_hash


class TestQuerying:
    """Test querying PluginManifest records."""

    def test_query_by_status(self, test_db_session):
        """Filter by PluginStatusEnum.ACTIVE."""
        # Create manifests with different statuses
        manifests = [
            PluginManifest(
                plugin_name="pending_plugin",
                version="1.0.0",
                source_code="code1",
                source_hash="hash_pending",
                status=PluginStatusEnum.PENDING,
            ),
            PluginManifest(
                plugin_name="active_plugin",
                version="1.0.0",
                source_code="code2",
                source_hash="hash_active",
                status=PluginStatusEnum.ACTIVE,
            ),
            PluginManifest(
                plugin_name="rejected_plugin",
                version="1.0.0",
                source_code="code3",
                source_hash="hash_rejected",
                status=PluginStatusEnum.REJECTED,
            ),
        ]

        test_db_session.add_all(manifests)
        test_db_session.commit()

        # Query only ACTIVE plugins
        active_plugins = (
            test_db_session.query(PluginManifest)
            .filter_by(status=PluginStatusEnum.ACTIVE)
            .all()
        )

        assert len(active_plugins) == 1
        assert active_plugins[0].plugin_name == "active_plugin"

    def test_query_by_plugin_name(self, test_db_session):
        """Query by plugin_name (indexed field)."""
        manifest1 = PluginManifest(
            plugin_name="my_plugin",
            version="1.0.0",
            source_code="v1",
            source_hash="hash_v1",
        )
        manifest2 = PluginManifest(
            plugin_name="my_plugin",
            version="2.0.0",
            source_code="v2",
            source_hash="hash_v2",
        )

        test_db_session.add_all([manifest1, manifest2])
        test_db_session.commit()

        results = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="my_plugin")
            .all()
        )

        assert len(results) == 2

    def test_query_active_by_name(self, test_db_session):
        """Query using composite index (plugin_name + status)."""
        manifests = [
            PluginManifest(
                plugin_name="plugin_a",
                version="1.0.0",
                source_code="code1",
                source_hash="hash1",
                status=PluginStatusEnum.PENDING,
            ),
            PluginManifest(
                plugin_name="plugin_a",
                version="2.0.0",
                source_code="code2",
                source_hash="hash2",
                status=PluginStatusEnum.ACTIVE,
            ),
        ]

        test_db_session.add_all(manifests)
        test_db_session.commit()

        # Query ACTIVE version of plugin_a
        active_version = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="plugin_a", status=PluginStatusEnum.ACTIVE)
            .first()
        )

        assert active_version is not None
        assert active_version.version == "2.0.0"


class TestIndexes:
    """Test that indexes exist (performance check)."""

    def test_plugin_name_index_exists(self, test_db_engine):
        """Verify plugin_name is indexed."""
        from sqlalchemy import inspect

        inspector = inspect(test_db_engine)
        indexes = inspector.get_indexes("cf_plugin_manifest")

        # Check that at least one index includes plugin_name
        index_columns = [idx["column_names"] for idx in indexes]
        assert any("plugin_name" in cols for cols in index_columns)

    def test_status_index_exists(self, test_db_engine):
        """Verify status is indexed."""
        from sqlalchemy import inspect

        inspector = inspect(test_db_engine)
        indexes = inspector.get_indexes("cf_plugin_manifest")

        # Check that at least one index includes status
        index_columns = [idx["column_names"] for idx in indexes]
        assert any("status" in cols for cols in index_columns)


class TestDeployedAtTimestamp:
    """Test deployed_at timestamp field."""

    def test_deployed_at_null_initially(self, test_db_session):
        """deployed_at is NULL for non-ACTIVE plugins."""
        manifest = PluginManifest(
            plugin_name="plugin",
            version="1.0.0",
            source_code="code",
            source_hash="hash",
            status=PluginStatusEnum.PENDING,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        assert manifest.deployed_at is None

    def test_deployed_at_set_on_active(self, test_db_session):
        """deployed_at should be set when status becomes ACTIVE."""
        manifest = PluginManifest(
            plugin_name="plugin",
            version="1.0.0",
            source_code="code",
            source_hash="hash_deploy",
            status=PluginStatusEnum.STAGING,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        # Promote to ACTIVE
        now = datetime.now()
        manifest.status = PluginStatusEnum.ACTIVE
        manifest.deployed_at = now
        test_db_session.commit()

        assert manifest.deployed_at is not None
        assert manifest.deployed_at == now


class TestVersioning:
    """Test multiple versions of same plugin."""

    def test_multiple_versions_same_plugin(self, test_db_session):
        """Multiple versions of same plugin can exist."""
        versions = ["1.0.0", "1.1.0", "2.0.0"]

        for i, version in enumerate(versions):
            manifest = PluginManifest(
                plugin_name="my_plugin",
                version=version,
                source_code=f"code_v{i}",
                source_hash=f"hash_{i}",
            )
            test_db_session.add(manifest)

        test_db_session.commit()

        all_versions = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="my_plugin")
            .all()
        )

        assert len(all_versions) == 3
        assert {v.version for v in all_versions} == set(versions)

    def test_query_latest_active_version(self, test_db_session):
        """Get the latest ACTIVE version of a plugin."""
        manifests = [
            PluginManifest(
                plugin_name="plugin",
                version="1.0.0",
                source_code="v1",
                source_hash="hash1",
                status=PluginStatusEnum.ACTIVE,
                deployed_at=datetime(2024, 1, 1),
            ),
            PluginManifest(
                plugin_name="plugin",
                version="2.0.0",
                source_code="v2",
                source_hash="hash2",
                status=PluginStatusEnum.ACTIVE,
                deployed_at=datetime(2024, 2, 1),
            ),
        ]

        test_db_session.add_all(manifests)
        test_db_session.commit()

        # Get latest by deployed_at
        latest = (
            test_db_session.query(PluginManifest)
            .filter_by(plugin_name="plugin", status=PluginStatusEnum.ACTIVE)
            .order_by(PluginManifest.deployed_at.desc())
            .first()
        )

        assert latest.version == "2.0.0"


class TestValidationErrors:
    """Test validation_error field."""

    def test_validation_error_stored(self, test_db_session):
        """Validation errors are stored in the field."""
        error_msg = "Banned import: os; Banned import: subprocess"

        manifest = PluginManifest(
            plugin_name="bad_plugin",
            version="1.0.0",
            source_code="bad code",
            source_hash="bad_hash",
            status=PluginStatusEnum.REJECTED,
            validation_error=error_msg,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        retrieved = test_db_session.get(PluginManifest, manifest.id)
        assert retrieved.validation_error == error_msg

    def test_validation_error_nullable(self, test_db_session):
        """validation_error can be NULL."""
        manifest = PluginManifest(
            plugin_name="good_plugin",
            version="1.0.0",
            source_code="good code",
            source_hash="good_hash",
            status=PluginStatusEnum.ACTIVE,
        )

        test_db_session.add(manifest)
        test_db_session.commit()

        assert manifest.validation_error is None
