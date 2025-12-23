#!/usr/bin/env python
"""Setup script to create test SourceRoot and PluginConfig for import testing."""

from pathlib import Path
from sqlalchemy.orm import Session
from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.models import SourceRoot, PluginConfig, RoutingRule

# Get database connection
engine = get_engine(settings.database)
db = Session(engine)

try:
    # 1. Create test SourceRoot
    test_path = str(Path("C:/Users/shan/workspace/casparianflow/test_data").resolve())

    source_root = db.query(SourceRoot).filter_by(path=test_path).first()
    if not source_root:
        source_root = SourceRoot(
            path=test_path,
            type="local",
            active=1
        )
        db.add(source_root)
        db.commit()
        print(f"Created SourceRoot: {test_path} (ID: {source_root.id})")
    else:
        print(f"SourceRoot already exists: {test_path} (ID: {source_root.id})")

    # 2. Create test plugins
    plugins = [
        ("csv_processor", "csv,data", "Process CSV files"),
        ("text_analyzer", "txt,doc", "Analyze text documents"),
        ("json_validator", "json,config", "Validate JSON files")
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
            print(f"Created PluginConfig: {plugin_name} (tags: {sub_tags})")
        else:
            print(f"PluginConfig already exists: {plugin_name}")

    db.commit()

    # 3. Create test routing rules
    rules = [
        ("*.csv", "csv", 100),
        ("*.txt", "txt", 90),
        ("*.json", "json", 85),
        ("data/*", "data", 50),
    ]

    for pattern, tag, priority in rules:
        rule = db.query(RoutingRule).filter_by(pattern=pattern, tag=tag).first()
        if not rule:
            rule = RoutingRule(pattern=pattern, tag=tag, priority=priority)
            db.add(rule)
            print(f"Created RoutingRule: {pattern} -> {tag} (priority: {priority})")
        else:
            print(f"RoutingRule already exists: {pattern} -> {tag}")

    db.commit()

    print("\nTest setup complete!")
    print(f"\nSourceRoot ID: {source_root.id}")
    print("You can now test the import feature at http://localhost:5000/import")

except Exception as e:
    print(f"Error: {e}")
    db.rollback()
    raise
finally:
    db.close()
