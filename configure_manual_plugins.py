"""
Helper script to configure routing rules for manual plugins.

Reads PATTERN and TOPIC from plugin comments and configures database.

Usage:
    python configure_manual_plugins.py --plugin-dir manual_plugins --db demo_output_manual/demo.db
"""

import argparse
import re
from pathlib import Path
from sqlalchemy import create_engine
from sqlalchemy.orm import Session
import sys

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from casparian_flow.db.models import RoutingRule, PluginConfig, TopicConfig


def extract_plugin_metadata(plugin_file: Path):
    """
    Extract PATTERN and TOPIC from plugin comments.

    Args:
        plugin_file: Path to plugin file

    Returns:
        Dict with 'pattern' and 'topic', or None if not found
    """
    content = plugin_file.read_text(encoding='utf-8')

    pattern_match = re.search(r'#\s*PATTERN:\s*(.+)', content, re.IGNORECASE)
    topic_match = re.search(r'#\s*TOPIC:\s*(.+)', content, re.IGNORECASE)

    if pattern_match and topic_match:
        return {
            'plugin_name': plugin_file.stem,
            'pattern': pattern_match.group(1).strip(),
            'topic': topic_match.group(1).strip()
        }

    return None


def configure_plugin_routing(db_path: Path, plugin_dir: Path):
    """
    Configure routing rules for all plugins in directory.

    Args:
        db_path: Path to database
        plugin_dir: Directory containing plugins
    """
    engine = create_engine(f"sqlite:///{db_path}")

    plugin_files = list(plugin_dir.glob("*.py"))
    plugin_files = [p for p in plugin_files if p.name != "__init__.py"]

    configured = 0

    with Session(engine) as session:
        for plugin_file in plugin_files:
            metadata = extract_plugin_metadata(plugin_file)

            if not metadata:
                print(f"⚠ Skipping {plugin_file.name} - no PATTERN/TOPIC found")
                continue

            plugin_name = metadata['plugin_name']
            pattern = metadata['pattern']
            topic = metadata['topic']

            # Create tag
            tag = f"manual_{plugin_name}"

            # Check if routing rule already exists
            existing_rule = session.query(RoutingRule).filter_by(pattern=pattern).first()
            if not existing_rule:
                routing_rule = RoutingRule(
                    pattern=pattern,
                    tag=tag,
                    priority=100
                )
                session.add(routing_rule)
                print(f"✓ Added routing rule: {pattern} -> {tag}")

            # Check if plugin config already exists
            existing_config = session.query(PluginConfig).filter_by(plugin_name=plugin_name).first()
            if not existing_config:
                plugin_config = PluginConfig(
                    plugin_name=plugin_name,
                    subscription_tags=tag
                )
                session.add(plugin_config)
                print(f"✓ Added plugin config: {plugin_name} -> {tag}")

            # Check if topic config already exists
            existing_topic = session.query(TopicConfig).filter_by(plugin_name=plugin_name).first()
            if not existing_topic:
                topic_config = TopicConfig(
                    plugin_name=plugin_name,
                    topic_name="output",
                    uri=f"sqlite://parsed_data.db/{topic}",
                    mode="append"
                )
                session.add(topic_config)
                print(f"✓ Added topic config: {plugin_name} -> {topic}")

            configured += 1

        session.commit()

    print(f"\n✓ Configured {configured} plugin(s)")


def main():
    parser = argparse.ArgumentParser(description="Configure routing rules for manual plugins")
    parser.add_argument("--plugin-dir", required=True, help="Directory containing plugins")
    parser.add_argument("--db", required=True, help="Path to database")

    args = parser.parse_args()

    plugin_dir = Path(args.plugin_dir)
    db_path = Path(args.db)

    if not plugin_dir.exists():
        print(f"Error: Plugin directory not found: {plugin_dir}")
        sys.exit(1)

    if not db_path.exists():
        print(f"Error: Database not found: {db_path}")
        sys.exit(1)

    configure_plugin_routing(db_path, plugin_dir)


if __name__ == "__main__":
    main()
