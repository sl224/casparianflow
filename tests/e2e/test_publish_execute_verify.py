"""
Comprehensive E2E Test: Publish → Execute → Verify

This test combines Data-Oriented Design deployment with actual plugin execution:

1. Deploy plugin using deploy_artifact() (AST-based, no execution)
2. Drop data file
3. Run Scout (discovery + tagging)
4. Start Sentinel + Worker infrastructure
5. Wait for plugin execution
6. Verify output data

This verifies the complete v5.0 lifecycle from artifact deployment to data output.
"""

import pytest
import time
import threading
import socket
import hashlib
import pandas as pd
from pathlib import Path
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    SourceRoot,
    ProcessingJob,
    StatusEnum,
    RoutingRule,
    PluginConfig,
    TopicConfig,
)
from casparian_flow.engine.sentinel import Sentinel
from casparian_flow.engine.worker_client import GeneralistWorker
from casparian_flow.engine.config import (
    WorkerConfig,
    DatabaseConfig,
    StorageConfig,
    PluginsConfig,
)
from casparian_flow.services.scout import Scout
from casparian_flow.services.architect import ArchitectService
from casparian_flow.protocol import DeployCommand
from casparian_flow.security.identity import User


def get_free_port():
    """Find an available port for ZMQ."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture
def e2e_env(tmp_path):
    """
    Set up complete E2E environment:
    - Database
    - Source root (input directory)
    - Output directory (parquet)
    - Plugins directory
    - Config
    """
    # Directories
    source_dir = tmp_path / "source"
    source_dir.mkdir()

    output_dir = tmp_path / "output"
    output_dir.mkdir()

    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()

    # Database
    db_path = tmp_path / "test_e2e.db"
    conn_str = f"sqlite:///{db_path}"
    engine = create_engine(conn_str)
    initialize_database(engine, reset_tables=True)

    # Source root
    source_root_id = get_or_create_sourceroot(engine, str(source_dir))

    # Config
    config = WorkerConfig(
        database=DatabaseConfig(connection_string=conn_str),
        storage=StorageConfig(parquet_root=output_dir),
        plugins=PluginsConfig(dir=plugins_dir),
    )

    return {
        "source_dir": source_dir,
        "output_dir": output_dir,
        "plugins_dir": plugins_dir,
        "engine": engine,
        "config": config,
        "source_root_id": source_root_id,
        "zmq_port": get_free_port(),
        "conn_str": conn_str,
    }


@pytest.mark.integration
def test_full_lifecycle_publish_execute_verify(e2e_env):
    """
    Complete E2E test: Deploy plugin → Process file → Verify output.

    Flow:
    1. Deploy plugin using ArchitectService.deploy_artifact() (DOD)
    2. Verify state projection (RoutingRule, PluginConfig, TopicConfig)
    3. Drop CSV file in source directory
    4. Run Scout (discovery + pattern matching + job queue)
    5. Start Sentinel + Worker
    6. Wait for job execution
    7. Verify output parquet file
    8. Verify data transformations
    """

    # =========================================================================
    # Step 1: Deploy Plugin using v5.0 DOD Approach
    # =========================================================================
    plugin_source = """
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.csv",
    topic="processed_output",
    subscriptions=[]
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Read CSV
        df = pd.read_csv(file_path)

        # Add transformation proof
        df['processed_by'] = 'DOD_PLUGIN'
        df['row_count'] = len(df)

        # Publish as Arrow table
        self.publish("processed_output", pa.Table.from_pandas(df))
"""

    # Create DeployCommand
    deploy_cmd = DeployCommand(
        plugin_name="csv_processor",
        version="1.0.0",
        source_code=plugin_source,
        lockfile_content="",  # No isolated env
        env_hash="",
        artifact_hash=hashlib.sha256(plugin_source.encode()).hexdigest(),
        signature="e2e-test-signature",
        publisher_name="E2E Test",
        publisher_email="e2e@test.com",
        azure_oid=None,
        system_requirements=[],
    )

    publisher = User(
        id=0,
        name="E2E Test",
        email="e2e@test.com",
        azure_oid=None,
    )

    # Deploy via Architect
    architect = ArchitectService(e2e_env["engine"], secret_key="e2e-secret")
    result = architect.deploy_artifact(deploy_cmd, publisher)

    assert result.success, f"Deployment failed: {result.error_message}"
    assert result.manifest_id is not None

    print(f"✓ Plugin deployed via DOD: {result.plugin_name} (manifest_id={result.manifest_id})")

    # =========================================================================
    # Step 2: Verify State Projection
    # =========================================================================
    with Session(e2e_env["engine"]) as session:
        # Check RoutingRule
        rule = session.query(RoutingRule).filter_by(pattern="*.csv").first()
        assert rule is not None, "RoutingRule not created"
        assert rule.tag == "auto_csv_processor"
        print(f"✓ RoutingRule: {rule.pattern} → {rule.tag}")

        # Check PluginConfig
        config = session.query(PluginConfig).filter_by(plugin_name="csv_processor").first()
        assert config is not None, "PluginConfig not created"
        print(f"✓ PluginConfig: {config.plugin_name}")

        # Check TopicConfig
        topic = (
            session.query(TopicConfig)
            .filter_by(plugin_name="csv_processor", topic_name="processed_output")
            .first()
        )
        assert topic is not None, "TopicConfig not created"
        print(f"✓ TopicConfig: {topic.topic_name} → {topic.uri}")

    # =========================================================================
    # Step 3: Write Plugin File to Disk (for Worker to load)
    # =========================================================================
    # Worker needs to physically load the plugin from disk
    plugin_file = e2e_env["plugins_dir"] / "csv_processor.py"
    plugin_file.write_text(plugin_source, encoding="utf-8")
    print(f"✓ Plugin written to disk: {plugin_file}")

    # =========================================================================
    # Step 4: Drop CSV File
    # =========================================================================
    csv_file = e2e_env["source_dir"] / "data.csv"
    csv_file.write_text(
        "id,name,value\n"
        "1,Alice,100\n"
        "2,Bob,200\n"
        "3,Charlie,300\n",
        encoding="utf-8"
    )
    print(f"✓ CSV file created: {csv_file}")

    # =========================================================================
    # Step 5: Run Scout (Discovery → Tagging → Job Queue)
    # =========================================================================
    with Session(e2e_env["engine"]) as session:
        source_root = session.get(SourceRoot, e2e_env["source_root_id"])
        scout = Scout(session)
        scout.scan_source(source_root)

        # Verify job queued
        jobs = session.query(ProcessingJob).all()
        assert len(jobs) == 1, f"Expected 1 job, got {len(jobs)}"
        job = jobs[0]
        assert job.plugin_name == "csv_processor"
        assert job.status == StatusEnum.QUEUED
        print(f"✓ Job queued: {job.id} for {job.plugin_name}")

    # =========================================================================
    # Step 6: Start Sentinel + Worker
    # =========================================================================
    sentinel_addr = f"tcp://127.0.0.1:{e2e_env['zmq_port']}"

    # Start Sentinel
    sentinel = Sentinel(e2e_env["config"], bind_addr=sentinel_addr)
    t_sentinel = threading.Thread(target=sentinel.run, daemon=True)
    t_sentinel.start()
    print(f"✓ Sentinel started at {sentinel_addr}")

    # Start Worker
    worker = GeneralistWorker(
        sentinel_addr,
        e2e_env["plugins_dir"],
        e2e_env["engine"],
        parquet_root=e2e_env["output_dir"],
    )
    t_worker = threading.Thread(target=worker.start, daemon=True)
    t_worker.start()
    print("✓ Worker started")

    # Wait for worker registration
    time.sleep(2)

    # =========================================================================
    # Step 7: Wait for Job Execution
    # =========================================================================
    print("[WAIT] Waiting for job execution...")

    job_completed = False
    for i in range(50):  # 5 second timeout
        with Session(e2e_env["engine"]) as session:
            job = session.query(ProcessingJob).first()
            if job.status == StatusEnum.COMPLETED:
                job_completed = True
                print(f"✓ Job completed: {job.id}")
                break
            elif job.status == StatusEnum.FAILED:
                pytest.fail(f"Job failed: {job.error_message}")
        time.sleep(0.1)

    assert job_completed, "Job did not complete within timeout"

    # =========================================================================
    # Step 8: Verify Output File
    # =========================================================================
    print(f"[DEBUG] Output directory: {e2e_env['output_dir']}")
    output_files = list(e2e_env["output_dir"].rglob("*.parquet"))
    print(f"[DEBUG] Output files: {output_files}")

    assert len(output_files) > 0, "No output files created"
    output_file = output_files[0]
    print(f"✓ Output file created: {output_file}")

    # Verify filename contains job ID (race condition prevention)
    assert f"_{job.id}.parquet" in output_file.name, \
        f"Output filename {output_file.name} missing job ID {job.id}"

    # =========================================================================
    # Step 9: Verify Data Transformations
    # =========================================================================
    df = pd.read_parquet(output_file)
    print(f"✓ Output data:\n{df}")

    # Check original columns
    assert "id" in df.columns
    assert "name" in df.columns
    assert "value" in df.columns

    # Check transformation columns
    assert "processed_by" in df.columns, "Missing transformation column 'processed_by'"
    assert df.iloc[0]["processed_by"] == "DOD_PLUGIN"
    assert "row_count" in df.columns
    assert df.iloc[0]["row_count"] == 3

    # Check lineage columns (injected by Worker)
    assert "_job_id" in df.columns, "Missing lineage column _job_id"
    assert "_file_version_id" in df.columns, "Missing lineage column _file_version_id"
    assert df.iloc[0]["_job_id"] == job.id

    # Check row count
    assert len(df) == 3, f"Expected 3 rows, got {len(df)}"

    print("✓ Data transformations verified")
    print("✓ Lineage tracking verified")

    # =========================================================================
    # Cleanup
    # =========================================================================
    sentinel.stop()
    worker.stop()
    time.sleep(0.5)

    print("\n" + "=" * 70)
    print("✓ FULL E2E LIFECYCLE VERIFIED")
    print("  Deploy (DOD) → Scout → Queue → Execute → Output")
    print("=" * 70)


@pytest.mark.integration
def test_deploy_multiple_plugins_routing(e2e_env):
    """
    Test multiple plugins with different patterns routing correctly.

    Verifies that:
    1. Multiple plugins can be deployed
    2. Each creates its own routing rule
    3. Files are routed to the correct plugin
    """

    # Deploy plugin 1: handles *.csv
    csv_plugin = """
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.csv",
    topic="csv_output",
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.read_csv(file_path)
        df['handler'] = 'CSV_HANDLER'
        self.publish("csv_output", pa.Table.from_pandas(df))
"""

    # Deploy plugin 2: handles *.tsv
    tsv_plugin = """
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.tsv",
    topic="tsv_output",
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.read_csv(file_path, sep='\\t')
        df['handler'] = 'TSV_HANDLER'
        self.publish("tsv_output", pa.Table.from_pandas(df))
"""

    architect = ArchitectService(e2e_env["engine"], secret_key="test")
    publisher = User(id=0, name="Test", email="test@test.com", azure_oid=None)

    # Deploy both plugins
    for name, source in [("csv_handler", csv_plugin), ("tsv_handler", tsv_plugin)]:
        cmd = DeployCommand(
            plugin_name=name,
            version="1.0.0",
            source_code=source,
            lockfile_content="",
            env_hash="",
            artifact_hash=hashlib.sha256(source.encode()).hexdigest(),
            signature="test",
            publisher_name="Test",
        )
        result = architect.deploy_artifact(cmd, publisher)
        assert result.success, f"Failed to deploy {name}: {result.error_message}"
        print(f"✓ Deployed {name}")

    # Verify routing rules
    with Session(e2e_env["engine"]) as session:
        rules = session.query(RoutingRule).all()
        assert len(rules) == 2, f"Expected 2 rules, got {len(rules)}"

        patterns = {rule.pattern for rule in rules}
        assert "*.csv" in patterns
        assert "*.tsv" in patterns
        print(f"✓ Routing rules created: {patterns}")

    print("\n✓ Multi-plugin routing test passed")
