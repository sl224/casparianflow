#!/usr/bin/env python3
"""
Database Inspection Tool for E2E Tests

This script runs the full lifecycle test and keeps the database
available for manual inspection.

Usage:
    python tests/e2e/inspect_db.py

Then in another terminal:
    sqlite3 /tmp/casparian_e2e_inspect/test.db
"""

import sys
import time
import hashlib
from pathlib import Path
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "src"))

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    SourceRoot,
    ProcessingJob,
    StatusEnum,
    RoutingRule,
    PluginConfig,
    TopicConfig,
    PluginManifest,
    FileLocation,
    FileVersion,
    Publisher,
)
from casparian_flow.services.scout import Scout
from casparian_flow.services.architect import ArchitectService
from casparian_flow.protocol import DeployCommand
from casparian_flow.security.identity import User


def setup_environment():
    """Create persistent test environment."""
    # Use fixed path instead of tmp_path
    base_dir = Path("/tmp/casparian_e2e_inspect")
    base_dir.mkdir(exist_ok=True)

    # Clean up old runs
    for item in base_dir.iterdir():
        if item.is_file():
            item.unlink()
        elif item.is_dir():
            import shutil
            shutil.rmtree(item)

    source_dir = base_dir / "source"
    source_dir.mkdir()

    output_dir = base_dir / "output"
    output_dir.mkdir()

    plugins_dir = base_dir / "plugins"
    plugins_dir.mkdir()

    # Database
    db_path = base_dir / "test.db"
    conn_str = f"sqlite:///{db_path}"
    engine = create_engine(conn_str)
    initialize_database(engine, reset_tables=True)

    # Source root
    source_root_id = get_or_create_sourceroot(engine, str(source_dir))

    return {
        "base_dir": base_dir,
        "source_dir": source_dir,
        "output_dir": output_dir,
        "plugins_dir": plugins_dir,
        "db_path": db_path,
        "engine": engine,
        "source_root_id": source_root_id,
    }


def deploy_plugin(env):
    """Deploy test plugin using DOD approach."""
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
        df = pd.read_csv(file_path)
        df['processed_by'] = 'INSPECTION_PLUGIN'
        df['row_count'] = len(df)
        self.publish("processed_output", pa.Table.from_pandas(df))
"""

    deploy_cmd = DeployCommand(
        plugin_name="csv_processor",
        version="1.0.0",
        source_code=plugin_source,
        lockfile_content="",
        env_hash="",
        artifact_hash=hashlib.sha256(plugin_source.encode()).hexdigest(),
        signature="inspection-test",
        publisher_name="Inspection User",
        publisher_email="inspect@test.com",
        azure_oid=None,
        system_requirements=[],
    )

    publisher = User(
        id=0,
        name="Inspection User",
        email="inspect@test.com",
        azure_oid=None,
    )

    architect = ArchitectService(env["engine"], secret_key="inspect-secret")
    result = architect.deploy_artifact(deploy_cmd, publisher)

    if not result.success:
        print(f"❌ Deployment failed: {result.error_message}")
        sys.exit(1)

    print(f"✓ Plugin deployed: {result.plugin_name} (manifest_id={result.manifest_id})")

    # Write plugin to disk
    plugin_file = env["plugins_dir"] / "csv_processor.py"
    plugin_file.write_text(plugin_source, encoding="utf-8")

    return result


def create_test_file(env):
    """Create test CSV file."""
    csv_file = env["source_dir"] / "data.csv"
    csv_file.write_text(
        "id,name,value\n"
        "1,Alice,100\n"
        "2,Bob,200\n"
        "3,Charlie,300\n",
        encoding="utf-8"
    )
    print(f"✓ CSV file created: {csv_file}")
    return csv_file


def run_scout(env):
    """Run Scout to discover and tag files."""
    with Session(env["engine"]) as session:
        source_root = session.get(SourceRoot, env["source_root_id"])
        scout = Scout(session)
        scout.scan_source(source_root)

        jobs = session.query(ProcessingJob).all()
        print(f"✓ Scout completed: {len(jobs)} job(s) queued")
        return jobs


def print_database_summary(env):
    """Print summary of database contents."""
    with Session(env["engine"]) as session:
        print("\n" + "=" * 70)
        print("DATABASE SUMMARY")
        print("=" * 70)

        # Publishers
        publishers = session.query(Publisher).all()
        print(f"\n📋 Publishers: {len(publishers)}")
        for p in publishers:
            print(f"   - {p.name} ({p.email})")

        # PluginManifest
        manifests = session.query(PluginManifest).all()
        print(f"\n📦 Plugin Manifests: {len(manifests)}")
        for m in manifests:
            print(f"   - {m.plugin_name} v{m.version} [{m.status.value}]")

        # RoutingRules
        rules = session.query(RoutingRule).all()
        print(f"\n🔀 Routing Rules: {len(rules)}")
        for r in rules:
            print(f"   - {r.pattern} → {r.tag} (priority: {r.priority})")

        # PluginConfig
        configs = session.query(PluginConfig).all()
        print(f"\n⚙️  Plugin Configs: {len(configs)}")
        for c in configs:
            print(f"   - {c.plugin_name} (tags: {c.subscription_tags})")

        # TopicConfig
        topics = session.query(TopicConfig).all()
        print(f"\n📤 Topic Configs: {len(topics)}")
        for t in topics:
            print(f"   - {t.plugin_name}.{t.topic_name} → {t.uri}")

        # FileLocations
        locations = session.query(FileLocation).all()
        print(f"\n📁 File Locations: {len(locations)}")
        for loc in locations:
            print(f"   - {loc.filename} (version_id: {loc.current_version_id})")

        # FileVersions
        versions = session.query(FileVersion).all()
        print(f"\n📄 File Versions: {len(versions)}")
        for v in versions:
            print(f"   - {v.content_hash[:8]}... (tags: {v.applied_tags})")

        # ProcessingJobs
        jobs = session.query(ProcessingJob).all()
        print(f"\n⚡ Processing Jobs: {len(jobs)}")
        for j in jobs:
            print(f"   - Job {j.id}: {j.plugin_name} [{j.status.value}]")

        print("\n" + "=" * 70)


def print_useful_queries(db_path):
    """Print useful SQL queries for manual inspection."""
    print("\n" + "=" * 70)
    print("USEFUL QUERIES")
    print("=" * 70)
    print(f"\n1. Connect to database:")
    print(f"   sqlite3 {db_path}")
    print(f"\n2. List all tables:")
    print(f"   .tables")
    print(f"\n3. View routing rules:")
    print(f"   SELECT * FROM cf_routing_rule;")
    print(f"\n4. View plugin manifests:")
    print(f"   SELECT plugin_name, version, status FROM cf_plugin_manifest;")
    print(f"\n5. View processing jobs:")
    print(f"   SELECT id, plugin_name, status FROM cf_processing_queue;")
    print(f"\n6. View file versions with tags:")
    print(f"   SELECT fv.id, fv.content_hash, fv.applied_tags, fl.filename")
    print(f"   FROM cf_file_version fv")
    print(f"   JOIN cf_file_location fl ON fv.location_id = fl.id;")
    print(f"\n7. Full job details:")
    print(f"   SELECT j.id, j.plugin_name, j.status, fl.filename")
    print(f"   FROM cf_processing_queue j")
    print(f"   JOIN cf_file_version fv ON j.file_version_id = fv.id")
    print(f"   JOIN cf_file_location fl ON fv.location_id = fl.id;")
    print("\n" + "=" * 70)


def main():
    print("\n" + "=" * 70)
    print("E2E DATABASE INSPECTION TOOL")
    print("=" * 70)

    # Setup
    env = setup_environment()
    print(f"\n✓ Environment created at: {env['base_dir']}")

    # Deploy plugin
    deploy_plugin(env)

    # Create test file
    create_test_file(env)

    # Run Scout
    run_scout(env)

    # Print summary
    print_database_summary(env)

    # Print queries
    print_useful_queries(env["db_path"])

    # Keep alive
    print(f"\n🔍 Database is ready for inspection!")
    print(f"   Database: {env['db_path']}")
    print(f"   Files:    {env['source_dir']}")
    print(f"\n⏸  Press CTRL+C to exit and cleanup...")

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n\n✓ Cleanup complete")


if __name__ == "__main__":
    main()
