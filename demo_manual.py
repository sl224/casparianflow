"""
Casparian Flow Manual ETL Demo

This script demonstrates the ETL pipeline using pre-written (manual) plugins
instead of AI-generated ones. Useful for development and testing.

Usage:
    python demo_manual.py <folder_path> --plugins <plugin1.py> <plugin2.py> ...
    python demo_manual.py <folder_path> --plugin-dir <directory>
"""

import argparse
import logging
import sys
import time
from pathlib import Path
from threading import Thread
from typing import List

from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    PluginConfig, TopicConfig, RoutingRule, ProcessingJob,
    StatusEnum, SourceRoot
)
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.engine.brokerimport ZmqWorker
from casparian_flow.services.scout import Scout
from casparian_flow.services.architect import ArchitectService

# Configuration
DEMO_DIR = Path("demo_output_manual")
DB_PATH = DEMO_DIR / "demo.db"
PLUGINS_DIR = DEMO_DIR / "plugins"
SQLITE_OUTPUT = DEMO_DIR / "parsed_data.db"
SECRET_KEY = "demo-secret-key"
ZMQ_ADDR = "tcp://127.0.0.1:7778"  # Different port from AI demo

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] %(levelname)s: %(message)s",
    datefmt="%H:%M:%S"
)
logger = logging.getLogger("demo-manual")

# Suppress noisy libraries
logging.getLogger("sqlalchemy.engine").setLevel(logging.WARNING)
logging.getLogger("urllib3").setLevel(logging.WARNING)


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Casparian Flow Manual ETL Demo (No AI)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Example usage:
    python demo_manual.py ./my_data --plugins plugin1.py plugin2.py
    python demo_manual.py ./my_data --plugin-dir ./my_plugins/
        """
    )
    parser.add_argument(
        "folder",
        type=str,
        help="Path to folder containing files to process"
    )

    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--plugins",
        nargs="+",
        help="Paths to plugin files to use"
    )
    group.add_argument(
        "--plugin-dir",
        type=str,
        help="Directory containing plugin files (*.py)"
    )

    parser.add_argument(
        "--unsafe",
        action="store_true",
        help="Use unsafe mode (skip signature validation) - DEV ONLY"
    )

    return parser.parse_args()


def setup_demo_environment(folder: Path):
    """
    Setup the demo environment.

    Args:
        folder: Input folder to process

    Returns:
        Validated folder path
    """
    logger.info("Setting up demo environment...")

    # Create demo output directory
    DEMO_DIR.mkdir(exist_ok=True)
    PLUGINS_DIR.mkdir(parents=True, exist_ok=True)
    logger.info(f"  Output directory: {DEMO_DIR.absolute()}")

    # Validate input folder
    if not folder.exists():
        logger.error(f"Folder does not exist: {folder}")
        sys.exit(1)

    # Check if folder has files
    files = list(folder.rglob("*"))
    file_count = sum(1 for f in files if f.is_file())

    if file_count == 0:
        logger.error(f"Folder is empty: {folder}")
        sys.exit(1)

    logger.info(f"  Input folder: {folder.absolute()}")
    logger.info(f"  Found {file_count} files")

    return folder


def load_plugin_files(args) -> List[Path]:
    """
    Load plugin files from arguments.

    Args:
        args: Parsed command-line arguments

    Returns:
        List of plugin file paths
    """
    plugin_files = []

    if args.plugins:
        # Individual plugin files specified
        for plugin_path in args.plugins:
            p = Path(plugin_path)
            if not p.exists():
                logger.error(f"Plugin file not found: {p}")
                sys.exit(1)
            if not p.suffix == ".py":
                logger.error(f"Plugin file must be .py: {p}")
                sys.exit(1)
            plugin_files.append(p)

    elif args.plugin_dir:
        # Plugin directory specified
        plugin_dir = Path(args.plugin_dir)
        if not plugin_dir.exists():
            logger.error(f"Plugin directory not found: {plugin_dir}")
            sys.exit(1)

        plugin_files = list(plugin_dir.glob("*.py"))
        # Filter out __init__.py and __pycache__
        plugin_files = [p for p in plugin_files if p.name != "__init__.py"]

        if not plugin_files:
            logger.error(f"No plugin files found in: {plugin_dir}")
            sys.exit(1)

    logger.info(f"Loaded {len(plugin_files)} plugin(s):")
    for p in plugin_files:
        logger.info(f"  - {p.name}")

    return plugin_files


def deploy_manual_plugin(
    plugin_file: Path,
    architect: ArchitectService,
    db_session: Session,
    unsafe: bool = False
) -> bool:
    """
    Deploy a manually-written plugin.

    Args:
        plugin_file: Path to plugin Python file
        architect: Architect service for deployment
        db_session: Database session
        unsafe: Use unsafe mode (skip signature validation)

    Returns:
        True if successful, False otherwise
    """
    try:
        logger.info(f"Deploying plugin: {plugin_file.name}")

        # Read plugin source code
        source_code = plugin_file.read_text(encoding="utf-8")

        # Extract plugin name from filename (remove .py)
        plugin_name = plugin_file.stem

        # Generate dummy signature (only matters if not unsafe)
        from casparian_flow.security.gatekeeper import generate_signature
        signature = generate_signature(source_code, SECRET_KEY) if not unsafe else "unsigned"

        # Deploy plugin
        result = architect.deploy_plugin(
            plugin_name=plugin_name,
            version="1.0.0",
            source_code=source_code,
            signature=signature,
            sample_input=None,
            unsafe=unsafe  # Use unsafe mode for unsigned plugins
        )

        if not result.success:
            logger.error(f"  ✗ Deployment failed: {result.error_message}")
            return False

        logger.info(f"  ✓ Deployed: {plugin_name}")
        return True

    except Exception as e:
        logger.error(f"  ✗ Failed to deploy {plugin_file.name}: {e}")
        return False


def extract_plugin_metadata(plugin_file: Path):
    """
    Extract PATTERN and TOPIC from plugin comments or docstrings.

    Args:
        plugin_file: Path to plugin file

    Returns:
        Dict with 'pattern' and 'topic', or None if not found
    """
    import re

    content = plugin_file.read_text(encoding='utf-8')

    # Try to match in comments first (# PATTERN:)
    pattern_match = re.search(r'#\s*PATTERN:\s*(.+)', content, re.IGNORECASE)
    topic_match = re.search(r'#\s*TOPIC:\s*(.+)', content, re.IGNORECASE)

    # If not found, try docstring format (just PATTERN: without #)
    if not pattern_match:
        pattern_match = re.search(r'^\s*PATTERN:\s*(.+)$', content, re.IGNORECASE | re.MULTILINE)
    if not topic_match:
        topic_match = re.search(r'^\s*TOPIC:\s*(.+)$', content, re.IGNORECASE | re.MULTILINE)

    if pattern_match and topic_match:
        return {
            'plugin_name': plugin_file.stem,
            'pattern': pattern_match.group(1).strip(),
            'topic': topic_match.group(1).strip()
        }

    return None


def configure_routing_from_plugins(engine, plugin_files: List[Path]):
    """
    Configure routing rules based on plugin metadata comments.

    Args:
        engine: SQLAlchemy engine
        plugin_files: List of plugin file paths
    """
    configured = 0

    with Session(engine) as session:
        for plugin_file in plugin_files:
            metadata = extract_plugin_metadata(plugin_file)

            if not metadata:
                logger.warning(f"  ⚠ Skipping {plugin_file.name} - no PATTERN/TOPIC found")
                continue

            plugin_name = metadata['plugin_name']
            pattern = metadata['pattern']
            topic = metadata['topic']

            # Create tag
            tag = f"manual_{plugin_name}"

            # Create routing rule
            routing_rule = RoutingRule(
                pattern=pattern,
                tag=tag,
                priority=100
            )
            session.add(routing_rule)

            # Configure plugin subscription
            plugin_config = PluginConfig(
                plugin_name=plugin_name,
                subscription_tags=tag
            )
            session.add(plugin_config)

            # Configure SQLite output topic
            topic_config = TopicConfig(
                plugin_name=plugin_name,
                topic_name="output",
                uri=f"sqlite://{SQLITE_OUTPUT.name}/{topic}",
                mode="append"
            )
            session.add(topic_config)

            logger.info(f"  Configured: {plugin_name} -> {pattern} -> {topic}")
            configured += 1

        session.commit()

    if configured == 0:
        logger.warning("  ⚠ No plugins configured - add PATTERN and TOPIC comments to your plugins")


def start_worker(db_path: Path):
    """
    Start the ZMQ worker in a background thread.

    Args:
        db_path: Path to metadata database

    Returns:
        Tuple of (worker, thread)
    """
    logger.info(f"Starting ZMQ Worker on {ZMQ_ADDR}...")

    config = WorkerConfig(
        database=DatabaseConfig(connection_string=f"sqlite:///{db_path}"),
        storage=StorageConfig(parquet_root=DEMO_DIR / "temp"),
        plugins=PluginsConfig(directory=PLUGINS_DIR)
    )

    worker = ZmqWorker(config, zmq_addr=ZMQ_ADDR, architect_secret_key=SECRET_KEY)
    thread = Thread(target=worker.run, daemon=True)
    thread.start()

    time.sleep(2)  # Wait for worker startup
    logger.info("  ✓ Worker started")

    return worker, thread


def monitor_jobs(engine, timeout: int = 120):
    """
    Monitor job processing progress.

    Args:
        engine: SQLAlchemy engine
        timeout: Maximum time to wait in seconds

    Returns:
        Final job statistics dict
    """
    logger.info("Monitoring job processing...")
    start = time.time()

    while time.time() - start < timeout:
        with Session(engine) as session:
            queued = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.QUEUED
            ).count()

            running = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.RUNNING
            ).count()

            completed = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.COMPLETED
            ).count()

            failed = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.FAILED
            ).count()

            stats = {
                "queued": queued,
                "running": running,
                "completed": completed,
                "failed": failed
            }

            logger.info(f"  Jobs: queued={queued}, running={running}, completed={completed}, failed={failed}")

            # Break if all jobs are done
            if queued == 0 and running == 0 and (completed + failed) > 0:
                logger.info("  ✓ All jobs completed!")
                return stats

        time.sleep(2)

    logger.warning(f"  ⚠ Timeout reached after {timeout}s")
    return stats


def display_results(output_db: Path):
    """
    Display results from the output database.

    Args:
        output_db: Path to output SQLite database
    """
    logger.info("=" * 60)
    logger.info("RESULTS")
    logger.info("=" * 60)

    if not output_db.exists():
        logger.error("✗ Output database not created!")
        logger.info(f"  Expected: {output_db}")
        return

    output_engine = create_engine(f"sqlite:///{output_db}")

    # Get list of tables
    with output_engine.connect() as conn:
        result = conn.execute(text("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"))
        tables = [row[0] for row in result]

    if not tables:
        logger.warning("No tables found in output database")
        return

    total_rows = 0
    for table in tables:
        try:
            with output_engine.connect() as conn:
                # Count rows
                result = conn.execute(text(f"SELECT COUNT(*) FROM {table}"))
                count = result.scalar()
                total_rows += count
                logger.info(f"  ✓ Table '{table}': {count} rows")

                # Show preview
                if count > 0:
                    preview = conn.execute(text(f"SELECT * FROM {table} LIMIT 3"))
                    rows = preview.fetchall()
                    if rows:
                        logger.info(f"    Preview (first {min(3, len(rows))} rows):")
                        for i, row in enumerate(rows, 1):
                            row_dict = dict(row._mapping)
                            # Truncate long values
                            row_str = {k: (str(v)[:50] + '...' if isinstance(v, str) and len(str(v)) > 50 else v)
                                      for k, v in row_dict.items()}
                            logger.info(f"      {i}. {row_str}")
        except Exception as e:
            logger.warning(f"  ⚠ Table '{table}': {str(e)}")

    logger.info("=" * 60)
    logger.info(f"Total rows across all tables: {total_rows}")
    logger.info("")
    logger.info("Demo complete! Inspect results:")
    logger.info(f"  Database: {output_db.absolute()}")
    logger.info(f"  Plugins:  {PLUGINS_DIR.absolute()}")
    logger.info("=" * 60)


def run_demo(args):
    """
    Main demo workflow.

    Args:
        args: Parsed command-line arguments
    """
    logger.info("=" * 60)
    logger.info("CASPARIAN FLOW MANUAL ETL DEMO")
    if args.unsafe:
        logger.warning("⚠ UNSAFE MODE ENABLED - Signature validation disabled")
    logger.info("=" * 60)

    # Setup environment
    folder = Path(args.folder).resolve()
    folder = setup_demo_environment(folder)

    # Load plugin files
    plugin_files = load_plugin_files(args)

    # Initialize databases
    logger.info("Initializing databases...")
    if DB_PATH.exists():
        DB_PATH.unlink()
    if SQLITE_OUTPUT.exists():
        SQLITE_OUTPUT.unlink()

    engine = create_engine(f"sqlite:///{DB_PATH}")
    initialize_database(engine, reset_tables=True)
    logger.info("  ✓ Databases initialized")

    # Register source root
    with Session(engine) as session:
        source_root_id = get_or_create_sourceroot(engine, str(folder))
        source_root = session.get(SourceRoot, source_root_id)

    # Initialize services
    logger.info("Initializing services...")
    architect = ArchitectService(engine, SECRET_KEY)
    logger.info("  ✓ Services initialized")

    # Start worker
    worker, worker_thread = start_worker(DB_PATH)

    # Deploy plugins
    logger.info("")
    logger.info(f"Deploying {len(plugin_files)} manual plugin(s)...")

    deployed_plugins = []
    with Session(engine) as session:
        for plugin_file in plugin_files:
            success = deploy_manual_plugin(plugin_file, architect, db_session=session, unsafe=args.unsafe)
            if success:
                deployed_plugins.append(plugin_file.stem)

    if not deployed_plugins:
        logger.error("✗ No plugins were successfully deployed!")
        worker.stop()
        worker_thread.join(timeout=5)
        sys.exit(1)

    logger.info(f"✓ Successfully deployed {len(deployed_plugins)} plugin(s)")

    # Auto-configure routing based on plugin comments
    logger.info("")
    logger.info("Configuring routing rules from plugin metadata...")
    configure_routing_from_plugins(engine, plugin_files)
    logger.info("  ✓ Routing configured")

    # Hot reload plugins
    logger.info("")
    logger.info("Hot-reloading plugins in worker...")
    worker.reload_plugins()
    time.sleep(3)
    logger.info("  ✓ Plugins reloaded")

    # Scan and queue jobs
    logger.info("Scanning files and queuing processing jobs...")
    with Session(engine) as session:
        scout = Scout(session)
        scout.scan_source(source_root)

    with Session(engine) as session:
        job_count = session.query(ProcessingJob).count()
        logger.info(f"  ✓ Queued {job_count} job(s)")

    if job_count == 0:
        logger.warning("⚠ No jobs were queued - check routing rules")
        logger.info("  Make sure routing rules are configured for your plugins")
    else:
        # Monitor processing
        logger.info("")
        stats = monitor_jobs(engine, timeout=120)

    # Stop worker
    logger.info("")
    logger.info("Stopping worker...")
    worker.stop()
    worker_thread.join(timeout=5)
    logger.info("  ✓ Worker stopped")

    # Display results
    logger.info("")
    display_results(SQLITE_OUTPUT)


def main():
    """Entry point."""
    try:
        args = parse_args()
        run_demo(args)
    except KeyboardInterrupt:
        logger.info("")
        logger.info("Demo interrupted by user")
        sys.exit(0)
    except Exception as e:
        logger.exception(f"Demo failed with error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
