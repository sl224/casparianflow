#!/usr/bin/env python
"""
End-to-End Test: Import file → Queue job → Execute plugin → Verify output

This test demonstrates the complete workflow:
1. Register test plugin
2. Import a CSV file
3. Verify job was created
4. Execute the job with the plugin
5. Verify output was generated
"""

import sys
from pathlib import Path
from sqlalchemy.orm import Session

from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import (
    SourceRoot, PluginConfig, TopicConfig, RoutingRule,
    FileLocation, FileVersion, ProcessingJob, StatusEnum
)
from casparian_flow.services.import_service import ImportService
from casparian_flow.engine.queue import JobQueue

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

print("=" * 80)
print("END-TO-END TEST: Import -> Queue -> Execute -> Verify")
print("=" * 80)

try:
    # ========================================
    # STEP 1: Register Test Plugin
    # ========================================
    print("\n[STEP 1] Registering test_csv_parser plugin...")

    plugin = db.query(PluginConfig).filter_by(plugin_name="test_csv_parser").first()
    if not plugin:
        plugin = PluginConfig(
            plugin_name="test_csv_parser",
            subscription_tags="csv,data",
            default_parameters='{"description": "Test CSV parser"}'
        )
        db.add(plugin)
        db.commit()
        print(f"  Created plugin: test_csv_parser")
    else:
        print(f"  Plugin already exists: test_csv_parser")

    # Register topics/sinks for the plugin
    topics = [
        ("output", "parquet://data/output/csv_parsed.parquet", "append"),
        ("summary", "parquet://data/output/csv_summary.parquet", "append")
    ]

    for topic_name, uri, mode in topics:
        topic = db.query(TopicConfig).filter_by(
            plugin_name="test_csv_parser",
            topic_name=topic_name
        ).first()

        if not topic:
            topic = TopicConfig(
                plugin_name="test_csv_parser",
                topic_name=topic_name,
                uri=uri,
                mode=mode
            )
            db.add(topic)
            print(f"  Created topic: {topic_name} -> {uri}")
        else:
            print(f"  Topic already exists: {topic_name}")

    db.commit()

    # ========================================
    # STEP 2: Import Test File
    # ========================================
    print("\n[STEP 2] Importing test CSV file...")

    # Check if already imported
    existing_csv = db.query(FileLocation).filter(
        FileLocation.filename == "sample_data.csv"
    ).first()

    if existing_csv:
        print(f"  File already imported: ID {existing_csv.id}")
        csv_file = existing_csv
    else:
        # Import the file
        import_service = ImportService(db)
        imported = import_service.import_files(
            source_root_id=1,  # test_data source root
            rel_paths=["sample_data.csv"],
            manual_tags={"e2e_test"},
            manual_plugins={"test_csv_parser"}
        )

        if imported:
            csv_file = imported[0]
            print(f"  Imported file: ID {csv_file.id}, Version: {csv_file.current_version_id}")
        else:
            print("  ERROR: Failed to import file!")
            sys.exit(1)

    # ========================================
    # STEP 3: Verify/Create Job
    # ========================================
    print("\n[STEP 3] Checking job queue...")

    version = db.query(FileVersion).filter_by(id=csv_file.current_version_id).first()
    jobs = db.query(ProcessingJob).filter_by(
        file_version_id=version.id,
        plugin_name="test_csv_parser"
    ).all()

    if jobs:
        job = jobs[0]
        # Reset to QUEUED if already completed
        if job.status != StatusEnum.QUEUED:
            print(f"  Found existing job (status: {job.status.value}), resetting to QUEUED...")
            job.status = StatusEnum.QUEUED
            job.result_summary = None
            job.error_message = None
            db.commit()
        print(f"  Job ID {job.id}: Status = {job.status.value}")
    else:
        # Create new job
        print("  No job found, creating new job...")
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_csv_parser",
            status=StatusEnum.QUEUED,
            priority=100
        )
        db.add(job)
        db.commit()
        print(f"  Created job: ID {job.id}")

    # ========================================
    # STEP 4: Execute Job (Simplified Worker Simulation)
    # ========================================
    print("\n[STEP 4] Executing job with plugin...")

    # Load plugin
    plugin_path = Path("plugins/test_csv_parser.py")
    if not plugin_path.exists():
        print(f"  ERROR: Plugin file not found: {plugin_path}")
        sys.exit(1)

    # Import plugin module
    import importlib.util
    spec = importlib.util.spec_from_file_location("test_csv_parser", plugin_path)
    plugin_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin_module)

    print(f"  Loaded plugin: {plugin_module.MANIFEST['name']}")

    # Get file location
    managed_root = db.query(SourceRoot).filter_by(type="managed").first()
    file_path = Path(managed_root.path) / csv_file.rel_path

    print(f"  File path: {file_path}")
    print(f"  File exists: {file_path.exists()}")

    if not file_path.exists():
        print("  ERROR: File not found!")
        sys.exit(1)

    # Create mock context
    class MockContext:
        def __init__(self):
            self.topics = {}
            self.published_data = {}

        def register_topic(self, name):
            if name not in self.topics:
                handle = len(self.topics)
                self.topics[name] = handle
                self.published_data[name] = []
                print(f"    Registered topic: {name} (handle {handle})")
                return handle
            return self.topics[name]

        def publish(self, handle, data):
            # Find topic name by handle
            topic_name = next((k for k, v in self.topics.items() if v == handle), None)
            if topic_name:
                self.published_data[topic_name].append(data)
                print(f"    Published to '{topic_name}': {len(data)} rows")

    # Execute plugin
    context = MockContext()
    plugin_instance = plugin_module.Plugin(config={})

    print(f"\n  Executing plugin.consume()...")
    result = plugin_instance.consume(file_path, context)

    print(f"\n  Execution result:")
    print(f"    Status: {result.get('status')}")
    print(f"    Rows processed: {result.get('rows_processed')}")
    print(f"    Columns: {result.get('columns')}")

    # ========================================
    # STEP 5: Verify Output
    # ========================================
    print("\n[STEP 5] Verifying output...")

    print(f"  Published topics: {list(context.published_data.keys())}")

    for topic_name, data_list in context.published_data.items():
        print(f"\n  Topic '{topic_name}':")
        print(f"    Number of publishes: {len(data_list)}")
        if data_list:
            df = data_list[0]
            print(f"    Shape: {df.shape}")
            print(f"    Columns: {list(df.columns)}")
            print(f"    First 3 rows:")
            print(df.head(3).to_string(index=False))

    # Update job status
    job.status = StatusEnum.COMPLETED
    job.result_summary = str(result)
    db.commit()
    print(f"\n  Updated job status to COMPLETED")

    # ========================================
    # SUMMARY
    # ========================================
    print("\n" + "=" * 80)
    print("END-TO-END TEST SUMMARY")
    print("=" * 80)
    print(f"Plugin: test_csv_parser")
    print(f"File: {csv_file.filename} (ID: {csv_file.id})")
    print(f"Job: ID {job.id}, Status: {job.status.value}")
    print(f"Result: {result.get('status')}")
    print(f"Rows processed: {result.get('rows_processed')}")
    print(f"\nAll steps completed successfully!")

except Exception as e:
    print(f"\nERROR: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)
finally:
    db.close()
