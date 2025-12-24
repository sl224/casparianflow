"""
E2E Test: Publish-to-Execute Lifecycle (Data-Oriented Design)

This test verifies the "Autonomous Wiring" behavior:
1. State Zero: File exists but no routing rules → Scout finds it but doesn't queue
2. Action: Deploy plugin with MANIFEST.pattern="*.magic"
3. Effect: RoutingRule is created in database
4. Reaction: Scout re-runs, matches file, creates ProcessingJob

Golden Path Verification:
- Code is treated as DATA (AST parsing, no execution)
- State is projected to DB (RoutingRule, PluginSubscription, TopicConfig)
- Scout autonomously reacts to new state
"""

import pytest
import tempfile
import hashlib
from pathlib import Path
from datetime import datetime
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

from casparian_flow.db.setup import initialize_database
from casparian_flow.db.models import (
    SourceRoot,
    FileLocation,
    FileVersion,
    FileHashRegistry,
    RoutingRule,
    PluginConfig,
    PluginSubscription,
    TopicConfig,
    PluginManifest,
    ProcessingJob,
    StatusEnum,
    PluginStatusEnum,
)
from casparian_flow.services.scout import TaggerService
from casparian_flow.services.architect import ArchitectService
from casparian_flow.protocol import DeployCommand
from casparian_flow.security.identity import User
from casparian_flow.security.gatekeeper import compute_source_hash


@pytest.fixture
def test_engine(tmp_path):
    """Create an in-memory SQLite database for testing."""
    db_path = tmp_path / "test_lifecycle.db"
    engine = create_engine(f"sqlite:///{db_path}")
    initialize_database(engine, reset_tables=True)
    yield engine
    engine.dispose()


@pytest.fixture
def test_data_dir(tmp_path):
    """Create a test data directory."""
    data_dir = tmp_path / "data"
    data_dir.mkdir()
    return data_dir


@pytest.fixture
def source_root(test_engine, test_data_dir):
    """Create a source root pointing to test data directory."""
    with Session(test_engine) as session:
        root = SourceRoot(
            path=str(test_data_dir),
            type="local",
            active=1,
        )
        session.add(root)
        session.commit()
        root_id = root.id

    return root_id, test_data_dir


def create_test_file(data_dir: Path, filename: str, content: str = "test data") -> Path:
    """Create a test file in the data directory."""
    file_path = data_dir / filename
    file_path.write_text(content, encoding="utf-8")
    return file_path


def register_file_location_only(
    session: Session,
    root_id: int,
    rel_path: str,
    file_path: Path,
) -> FileLocation:
    """
    Register a FileLocation WITHOUT creating a FileVersion.

    This simulates a file discovered by Scanner but not yet tagged.
    The Tagger will create the FileVersion when routing rules exist.

    Returns the FileLocation object.
    """
    # Create FileLocation only (no FileVersion yet)
    location = FileLocation(
        source_root_id=root_id,
        rel_path=rel_path,
        filename=file_path.name,
        last_known_mtime=file_path.stat().st_mtime,
        last_known_size=file_path.stat().st_size,
        current_version_id=None,  # No version yet!
    )
    session.add(location)
    session.commit()

    return location


@pytest.mark.integration
def test_lifecycle_publish_to_execute(test_engine, source_root):
    """
    The Golden Path: Autonomous wiring of deployed plugin.

    Flow:
    1. Create file "data.magic"
    2. Register in DB
    3. Run Tagger → Verify NO jobs created (no rules exist)
    4. Deploy plugin with MANIFEST.pattern="*.magic"
    5. Verify RoutingRule created
    6. Run Tagger again → Verify job IS created
    """
    root_id, data_dir = source_root

    # =========================================================================
    # Step 1: Create test file
    # =========================================================================
    test_file = create_test_file(data_dir, "data.magic", "magic content here")

    with Session(test_engine) as session:
        # =========================================================================
        # Step 2: Register FileLocation only (no FileVersion yet)
        # =========================================================================
        location = register_file_location_only(session, root_id, "data.magic", test_file)

        # =========================================================================
        # Step 3: Run Tagger (STATE ZERO - no rules exist)
        # =========================================================================
        root = session.get(SourceRoot, root_id)
        tagger = TaggerService(session)
        tagger.run(root)

        # Verify NO jobs were created (no routing rules)
        job_count = session.query(ProcessingJob).count()
        assert job_count == 0, "Expected 0 jobs before plugin deployment"

        # Verify NO FileVersion created (no routing rules matched)
        version_count = session.query(FileVersion).count()
        assert version_count == 0, "Expected 0 versions before plugin deployment"

        print("✓ State Zero: File found, no routing rules, 0 jobs queued")

    # =========================================================================
    # Step 4: Deploy Plugin with MANIFEST
    # =========================================================================
    plugin_source = """
from casparian_flow.sdk import BasePlugin

class PluginMetadata:
    def __init__(self, pattern, topic, subscriptions=None):
        self.pattern = pattern
        self.topic = topic
        self.subscriptions = subscriptions or []

MANIFEST = PluginMetadata(
    pattern="*.magic",
    topic="magic_output",
    subscriptions=[]
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Process magic files
        self.publish("magic_output", {"result": "processed"})
"""

    deploy_cmd = DeployCommand(
        plugin_name="magic_processor",
        version="1.0.0",
        source_code=plugin_source,
        lockfile_content="",  # No isolated env for this test
        env_hash="",
        artifact_hash=hashlib.sha256(plugin_source.encode()).hexdigest(),
        signature="test-signature",
        publisher_name="Test Publisher",
        publisher_email="test@example.com",
        azure_oid=None,
        system_requirements=[],
    )

    publisher = User(
        id=0,
        name="Test Publisher",
        email="test@example.com",
        azure_oid=None,
    )

    architect = ArchitectService(test_engine, secret_key="test-secret")
    result = architect.deploy_artifact(deploy_cmd, publisher)

    assert result.success, f"Deployment failed: {result.error_message}"
    assert result.manifest_id is not None

    print(f"✓ Plugin deployed: {result.plugin_name} (manifest_id={result.manifest_id})")

    # =========================================================================
    # Step 5: Verify State Projection (Routing Rules Created)
    # =========================================================================
    with Session(test_engine) as session:
        # Check RoutingRule
        rule = session.query(RoutingRule).filter_by(pattern="*.magic").first()
        assert rule is not None, "RoutingRule not created"
        assert rule.tag == "auto_magic_processor"

        # Check PluginConfig
        config = session.query(PluginConfig).filter_by(plugin_name="magic_processor").first()
        assert config is not None, "PluginConfig not created"

        # Check TopicConfig
        topic = (
            session.query(TopicConfig)
            .filter_by(plugin_name="magic_processor", topic_name="magic_output")
            .first()
        )
        assert topic is not None, "TopicConfig not created"
        assert "parquet" in topic.uri

        # Check PluginManifest
        manifest = session.query(PluginManifest).filter_by(plugin_name="magic_processor").first()
        assert manifest is not None
        assert manifest.status == PluginStatusEnum.ACTIVE

        print("✓ State projected: RoutingRule, PluginConfig, TopicConfig created")

    # =========================================================================
    # Step 6: Run Tagger Again (Autonomous Reaction)
    # =========================================================================
    with Session(test_engine) as session:
        root = session.get(SourceRoot, root_id)
        tagger = TaggerService(session)
        tagger.run(root)

        # Verify FileVersion WAS created this time
        versions = session.query(FileVersion).all()
        assert len(versions) == 1, f"Expected 1 version, got {len(versions)}"

        # Verify job WAS created this time
        jobs = session.query(ProcessingJob).all()
        assert len(jobs) == 1, f"Expected 1 job, got {len(jobs)}"

        job = jobs[0]
        assert job.plugin_name == "magic_processor"
        assert job.status == StatusEnum.QUEUED
        assert job.file_version_id == versions[0].id

        print(f"✓ Autonomous reaction: Job {job.id} queued for magic_processor")

    print("\n" + "=" * 70)
    print("✓ GOLDEN PATH VERIFIED: Publish-to-Execute Lifecycle Complete")
    print("=" * 70)


@pytest.mark.unit
def test_extract_metadata_safely_valid():
    """Test AST metadata extraction with valid MANIFEST."""
    source = """
class PluginMetadata:
    pass

MANIFEST = PluginMetadata(
    pattern="*.csv",
    topic="output_topic",
    subscriptions=["upstream_a", "upstream_b"]
)
"""
    metadata = ArchitectService._extract_metadata_safely(source)

    assert metadata["pattern"] == "*.csv"
    assert metadata["topic"] == "output_topic"
    assert metadata["subscriptions"] == ["upstream_a", "upstream_b"]


@pytest.mark.unit
def test_extract_metadata_safely_missing_manifest():
    """Test that missing MANIFEST raises ValueError."""
    source = """
class Handler:
    pass
"""
    with pytest.raises(ValueError, match="MANIFEST not found"):
        ArchitectService._extract_metadata_safely(source)


@pytest.mark.unit
def test_extract_metadata_safely_dynamic_code():
    """Test that dynamic code in MANIFEST is rejected."""
    source = """
def get_pattern():
    return "*.csv"

class PluginMetadata:
    pass

MANIFEST = PluginMetadata(
    pattern=get_pattern(),  # Dynamic code!
    topic="output"
)
"""
    with pytest.raises(ValueError, match="dynamic code"):
        ArchitectService._extract_metadata_safely(source)


@pytest.mark.unit
def test_extract_metadata_safely_missing_required_field():
    """Test that missing required fields raise ValueError."""
    source = """
class PluginMetadata:
    pass

MANIFEST = PluginMetadata(
    pattern="*.csv"
    # Missing topic!
)
"""
    with pytest.raises(ValueError, match="missing required field"):
        ArchitectService._extract_metadata_safely(source)


@pytest.mark.unit
def test_extract_metadata_safely_syntax_error():
    """Test that invalid Python syntax raises SyntaxError."""
    source = """
MANIFEST = PluginMetadata(
    pattern="*.csv"
    topic=  # Syntax error!
)
"""
    with pytest.raises(SyntaxError):
        ArchitectService._extract_metadata_safely(source)
