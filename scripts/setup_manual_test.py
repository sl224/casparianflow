#!/usr/bin/env python
"""
Setup script for manual UI testing.
Creates a clean test environment with fresh data.
"""

from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import (
    SourceRoot, PluginConfig, TopicConfig, RoutingRule,
    FileLocation, FileVersion, ProcessingJob
)

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

print("=" * 70)
print("SETTING UP MANUAL TEST ENVIRONMENT")
print("=" * 70)

try:
    # ========================================
    # Clean up previous test data
    # ========================================
    print("\n[1] Cleaning up previous test data...")

    # Delete test jobs
    deleted_jobs = db.query(ProcessingJob).filter(
        ProcessingJob.plugin_name.in_(['sales_analyzer', 'text_processor', 'test_csv_parser'])
    ).delete(synchronize_session=False)
    print(f"  Deleted {deleted_jobs} test jobs")

    # Delete test file versions and locations (managed files only)
    managed_root = db.query(SourceRoot).filter_by(type="managed").first()
    if managed_root:
        locations = db.query(FileLocation).filter_by(source_root_id=managed_root.id).all()
        for loc in locations:
            # Delete versions
            db.query(FileVersion).filter_by(location_id=loc.id).delete()
        # Delete locations
        deleted_locs = db.query(FileLocation).filter_by(source_root_id=managed_root.id).delete()
        print(f"  Deleted {deleted_locs} managed file locations")

    db.commit()

    # ========================================
    # Create test SourceRoot
    # ========================================
    print("\n[2] Creating test SourceRoot...")

    test_path = str(Path("C:/Users/shan/workspace/casparianflow/test_files").resolve())
    source_root = db.query(SourceRoot).filter_by(path=test_path).first()

    if not source_root:
        source_root = SourceRoot(
            path=test_path,
            type="local",
            active=1
        )
        db.add(source_root)
        db.commit()
        print(f"  Created SourceRoot: {test_path} (ID: {source_root.id})")
    else:
        print(f"  SourceRoot exists: {test_path} (ID: {source_root.id})")

    # ========================================
    # Create test plugins
    # ========================================
    print("\n[3] Creating test plugins...")

    plugins = [
        ("sales_analyzer", "csv,sales,data", "Analyzes sales CSV files"),
        ("text_processor", "txt,doc,report", "Processes text documents")
    ]

    for plugin_name, sub_tags, desc in plugins:
        plugin = db.query(PluginConfig).filter_by(plugin_name=plugin_name).first()
        if not plugin:
            plugin = PluginConfig(
                plugin_name=plugin_name,
                subscription_tags=sub_tags,
                default_parameters=f'{{"description": "{desc}"}}'
            )
            db.add(plugin)
            print(f"  Created: {plugin_name} (subscriptions: {sub_tags})")
        else:
            print(f"  Exists: {plugin_name}")

    db.commit()

    # ========================================
    # Create topics for plugins
    # ========================================
    print("\n[4] Creating output topics...")

    topics = [
        ("sales_analyzer", "output", "parquet://data/output/sales_analyzed.parquet", "append"),
        ("text_processor", "output", "parquet://data/output/text_processed.parquet", "append")
    ]

    for plugin_name, topic_name, uri, mode in topics:
        topic = db.query(TopicConfig).filter_by(
            plugin_name=plugin_name,
            topic_name=topic_name
        ).first()

        if not topic:
            topic = TopicConfig(
                plugin_name=plugin_name,
                topic_name=topic_name,
                uri=uri,
                mode=mode
            )
            db.add(topic)
            print(f"  Created: {plugin_name}.{topic_name} -> {uri}")
        else:
            print(f"  Exists: {plugin_name}.{topic_name}")

    db.commit()

    # ========================================
    # Create routing rules
    # ========================================
    print("\n[5] Creating routing rules...")

    rules = [
        ("*.csv", "csv", 100),
        ("sales*.csv", "sales", 110),
        ("*.txt", "txt", 90),
        ("*report*", "report", 95),
        ("data/*", "data", 50)
    ]

    for pattern, tag, priority in rules:
        rule = db.query(RoutingRule).filter_by(pattern=pattern, tag=tag).first()
        if not rule:
            rule = RoutingRule(pattern=pattern, tag=tag, priority=priority)
            db.add(rule)
            print(f"  Created: {pattern} -> {tag} (priority {priority})")
        else:
            print(f"  Exists: {pattern} -> {tag}")

    db.commit()

    # ========================================
    # Summary
    # ========================================
    print("\n" + "=" * 70)
    print("SETUP COMPLETE!")
    print("=" * 70)

    print(f"\nTest SourceRoot ID: {source_root.id}")
    print(f"Path: {source_root.path}")

    print("\nTest Files Available:")
    print("  - test_files/data/sales_2024.csv (sales data)")
    print("  - test_files/data/inventory.csv (inventory data)")
    print("  - test_files/documents/report.txt (text report)")
    print("  - test_files/documents/config.json (JSON config)")

    print("\nPlugins Configured:")
    print("  - sales_analyzer (subscribes to: csv, sales, data)")
    print("  - text_processor (subscribes to: txt, doc, report)")

    print("\nRouting Rules:")
    print("  - *.csv -> csv tag")
    print("  - sales*.csv -> sales tag")
    print("  - *.txt -> txt tag")
    print("  - *report* -> report tag")

    print("\nNext Steps:")
    print("  1. Start the UI: uv run python -m casparian_flow.main_ui --port 5000")
    print("  2. Open browser: http://localhost:5000/import")
    print("  3. Follow the manual testing guide")

except Exception as e:
    print(f"\nError: {e}")
    import traceback
    traceback.print_exc()
    db.rollback()
finally:
    db.close()
