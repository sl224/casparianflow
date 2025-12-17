"""
Casparian Flow AI-Powered ETL Demo

This script demonstrates the complete end-to-end pipeline:
1. Scan a folder for files
2. Use Claude CLI to automatically generate parsing plugins
3. Process files through the ETL pipeline
4. Output parsed data to SQLite database

Usage:
    python demo.py <folder_path> [--generate-samples] [--max-file-types 3]
"""

import argparse
import logging
import sys
import time
import shutil
from pathlib import Path
from threading import Thread
from collections import defaultdict
from typing import List, Tuple, Optional

from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    PluginConfig, TopicConfig, RoutingRule, ProcessingJob,
    StatusEnum, SourceRoot, PluginManifest
)
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.engine.brokerimport ZmqWorker
from casparian_flow.services.scout import Scout
from casparian_flow.services.inspector import profile_file
from casparian_flow.services.llm_generator import LLMGenerator
from casparian_flow.services.llm_provider import get_provider
from casparian_flow.services.architect import ArchitectService
from casparian_flow.security.gatekeeper import generate_signature

# Configuration
DEMO_DIR = Path("demo_output")
DB_PATH = DEMO_DIR / "demo.db"
PLUGINS_DIR = DEMO_DIR / "plugins"
SQLITE_OUTPUT = DEMO_DIR / "parsed_data.db"
SECRET_KEY = "demo-secret-key"
ZMQ_ADDR = "tcp://127.0.0.1:7777"

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] %(levelname)s: %(message)s",
    datefmt="%H:%M:%S"
)
logger = logging.getLogger("demo")

# Suppress noisy libraries
logging.getLogger("sqlalchemy.engine").setLevel(logging.WARNING)
logging.getLogger("urllib3").setLevel(logging.WARNING)


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Casparian Flow AI-Powered ETL Demo",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Example usage:
    python demo.py ./my_data
    python demo.py ./my_data --generate-samples
    python demo.py ./my_data --max-file-types 5
        """
    )
    parser.add_argument(
        "folder",
        type=str,
        help="Path to folder containing files to process"
    )
    parser.add_argument(
        "--generate-samples",
        action="store_true",
        help="Auto-generate sample CSV/JSON files if folder is empty"
    )
    parser.add_argument(
        "--max-file-types",
        type=int,
        default=3,
        help="Maximum number of different file types to process (default: 3)"
    )
    return parser.parse_args()


def check_claude_cli():
    """Verify that the 'claude' command is available."""
    if not shutil.which("claude"):
        logger.error("'claude' command not found!")
        logger.error("Please ensure Claude Code CLI is installed and in PATH")
        logger.info("Visit https://claude.ai/download for installation instructions")
        sys.exit(1)
    logger.info("✓ Claude CLI detected")


def generate_sample_data(folder: Path):
    """
    Create sample CSV, JSON, and TXT files for demonstration.

    Args:
        folder: Directory to create sample files in
    """
    logger.info("Generating sample data files...")

    # CSV: Sales data
    csv_file = folder / "sales_2025.csv"
    csv_content = """date,product,quantity,price,region
2025-01-15,Widget A,100,29.99,North
2025-01-16,Widget B,150,39.99,South
2025-01-17,Widget C,200,19.99,East
2025-01-18,Widget A,120,29.99,West
2025-01-19,Widget B,180,39.99,North
2025-01-20,Widget C,90,19.99,South
"""
    csv_file.write_text(csv_content.strip(), encoding="utf-8")
    logger.info(f"  Created: {csv_file.name}")

    # JSON: User events
    json_file = folder / "events.json"
    json_content = """[
  {"timestamp": "2025-01-15T10:30:00", "user_id": 1001, "event": "login", "duration_seconds": 3600},
  {"timestamp": "2025-01-15T10:35:00", "user_id": 1001, "event": "purchase", "amount": 49.99},
  {"timestamp": "2025-01-15T11:00:00", "user_id": 1002, "event": "login", "duration_seconds": 1800},
  {"timestamp": "2025-01-15T11:15:00", "user_id": 1003, "event": "signup", "source": "email"},
  {"timestamp": "2025-01-15T11:30:00", "user_id": 1002, "event": "logout", "duration_seconds": 1800}
]"""
    json_file.write_text(json_content, encoding="utf-8")
    logger.info(f"  Created: {json_file.name}")

    # TXT: System logs
    txt_file = folder / "system.log"
    txt_content = """2025-01-15 10:00:00 INFO System started
2025-01-15 10:01:23 INFO User authentication successful
2025-01-15 10:05:45 WARNING High memory usage detected
2025-01-15 10:10:12 INFO Database backup completed
2025-01-15 10:15:00 ERROR Connection timeout to external service
2025-01-15 10:15:30 INFO Retrying connection
2025-01-15 10:15:35 INFO Connection restored
"""
    txt_file.write_text(txt_content.strip(), encoding="utf-8")
    logger.info(f"  Created: {txt_file.name}")

    logger.info(f"✓ Generated 3 sample files in {folder}")


def setup_demo_environment(folder: Path, generate_samples: bool):
    """
    Setup the demo environment and validate inputs.

    Args:
        folder: Input folder to process
        generate_samples: Whether to generate sample data if folder is empty

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
        if generate_samples:
            logger.info(f"Creating input folder: {folder}")
            folder.mkdir(parents=True, exist_ok=True)
            generate_sample_data(folder)
        else:
            logger.error(f"Folder does not exist: {folder}")
            logger.info("Tip: Use --generate-samples to create sample data")
            sys.exit(1)

    # Check if folder has files
    files = list(folder.rglob("*"))
    file_count = sum(1 for f in files if f.is_file())

    if file_count == 0:
        if generate_samples:
            generate_sample_data(folder)
        else:
            logger.error(f"Folder is empty: {folder}")
            logger.info("Tip: Use --generate-samples to create sample data")
            sys.exit(1)

    logger.info(f"  Input folder: {folder.absolute()}")
    logger.info(f"  Found {file_count} files")

    return folder


def discover_representative_files(folder: Path, max_types: int) -> List[Path]:
    """
    Discover representative files for each unique file extension.

    Args:
        folder: Folder to scan
        max_types: Maximum number of file types to process

    Returns:
        List of representative files (one per extension)
    """
    logger.info("Discovering file types...")

    # Group files by extension
    files_by_ext = defaultdict(list)
    for file_path in folder.rglob("*"):
        if file_path.is_file():
            ext = file_path.suffix.lower() or ".txt"  # Default to .txt for no extension
            files_by_ext[ext].append(file_path)

    # Prioritize common extensions
    priority_exts = [".csv", ".json", ".txt", ".xml", ".log", ".parquet"]

    # Sort extensions by priority, then alphabetically
    sorted_exts = sorted(
        files_by_ext.keys(),
        key=lambda e: (e not in priority_exts, priority_exts.index(e) if e in priority_exts else 999, e)
    )

    # Select one representative file per extension
    representatives = []
    for ext in sorted_exts[:max_types]:
        # Pick smallest file as representative for faster profiling
        files = files_by_ext[ext]
        representative = min(files, key=lambda f: f.stat().st_size)
        representatives.append(representative)

    logger.info(f"  Found {len(files_by_ext)} file types: {', '.join(sorted_exts)}")
    logger.info(f"  Processing {len(representatives)} types (limit: {max_types})")

    return representatives


def generate_plugin_for_file(
    file_path: Path,
    provider,
    architect: ArchitectService,
    db_session: Session
) -> Optional[Tuple[str, str]]:
    """
    Generate a plugin for a specific file type using AI.

    Args:
        file_path: Sample file to analyze
        provider: LLM provider instance
        architect: Architect service for deployment
        db_session: Database session

    Returns:
        Tuple of (plugin_name, topic_name) or None if failed
    """
    try:
        logger.info(f"Processing: {file_path.name}")

        # Step 1: Profile the file
        logger.info("  → Profiling file...")
        profile = profile_file(str(file_path))

        # Step 2: AI generates schema proposal
        logger.info("  → AI analyzing schema...")
        generator = LLMGenerator(provider)
        proposal = generator.propose_schema(profile)
        logger.info(f"  → AI Schema Proposal: {proposal.target_topic}")

        # Step 3: AI generates plugin code
        logger.info("  → AI generating plugin code...")
        plugin_code = generator.generate_plugin(proposal)

        # Step 4: Validate and deploy plugin
        plugin_name = f"demo_{proposal.target_topic}"
        logger.info(f"  → Deploying plugin: {plugin_name}")

        signature = generate_signature(plugin_code.source_code, SECRET_KEY)
        result = architect.deploy_plugin(
            plugin_name=plugin_name,
            version="1.0.0",
            source_code=plugin_code.source_code,
            signature=signature,
            sample_input=None  # Skip sandbox for demo speed
        )

        if not result.success:
            logger.error(f"  ✗ Plugin deployment failed: {result.error_message}")
            return None

        # Step 5: Configure SQLite output topic
        topic_name = proposal.target_topic
        topic_config = TopicConfig(
            plugin_name=plugin_name,
            topic_name="output",
            uri=f"sqlite://{SQLITE_OUTPUT.name}/{topic_name}",
            mode="append"
        )
        db_session.add(topic_config)

        # Step 6: Configure routing rule
        ext = file_path.suffix.lower() or ".txt"
        pattern = f"*{ext}"
        tag = f"demo_{ext[1:] if ext else 'data'}"

        routing_rule = RoutingRule(
            pattern=pattern,
            tag=tag,
            priority=100
        )
        db_session.add(routing_rule)

        # Step 7: Configure plugin subscription
        plugin_config = PluginConfig(
            plugin_name=plugin_name,
            subscription_tags=tag
        )
        db_session.add(plugin_config)

        db_session.commit()

        logger.info(f"  ✓ Generated: {plugin_name}")
        return (plugin_name, topic_name)

    except Exception as e:
        logger.error(f"  ✗ Failed: {str(e)}")
        return None


def start_worker(db_path: Path) -> Tuple[ZmqWorker, Thread]:
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


def display_results(output_db: Path, plugins: List[Tuple[str, str]]):
    """
    Display results from the output database.

    Args:
        output_db: Path to output SQLite database
        plugins: List of (plugin_name, topic_name) tuples
    """
    logger.info("=" * 60)
    logger.info("RESULTS")
    logger.info("=" * 60)

    if not output_db.exists():
        logger.error("✗ Output database not created!")
        logger.info(f"  Expected: {output_db}")
        return

    output_engine = create_engine(f"sqlite:///{output_db}")

    total_rows = 0
    for plugin_name, topic in plugins:
        try:
            with output_engine.connect() as conn:
                # Count rows
                result = conn.execute(text(f"SELECT COUNT(*) FROM {topic}"))
                count = result.scalar()
                total_rows += count
                logger.info(f"  ✓ Table '{topic}': {count} rows")

                # Show preview
                if count > 0:
                    preview = conn.execute(text(f"SELECT * FROM {topic} LIMIT 3"))
                    rows = preview.fetchall()
                    if rows:
                        logger.info(f"    Preview (first {min(3, len(rows))} rows):")
                        for i, row in enumerate(rows, 1):
                            # Format row nicely
                            row_dict = dict(row._mapping)
                            logger.info(f"      {i}. {row_dict}")
        except Exception as e:
            logger.warning(f"  ⚠ Table '{topic}': {str(e)}")

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
    logger.info("CASPARIAN FLOW AI-POWERED ETL DEMO")
    logger.info("=" * 60)

    # Check Claude CLI availability
    check_claude_cli()

    # Setup environment
    folder = Path(args.folder).resolve()
    folder = setup_demo_environment(folder, args.generate_samples)

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
    logger.info("Initializing AI services...")
    provider = get_provider("claude-cli")
    architect = ArchitectService(engine, SECRET_KEY)
    logger.info("  ✓ Services initialized")

    # Start worker
    worker, worker_thread = start_worker(DB_PATH)

    # Discover files
    representative_files = discover_representative_files(folder, args.max_file_types)

    if not representative_files:
        logger.error("No files found to process!")
        worker.stop()
        worker_thread.join(timeout=5)
        sys.exit(1)

    # AI Plugin Generation
    logger.info("")
    logger.info(f"Generating plugins for {len(representative_files)} file types...")
    logger.info("(This may take a few minutes...)")
    logger.info("")

    generated_plugins = []
    with Session(engine) as session:
        for sample_file in representative_files:
            result = generate_plugin_for_file(sample_file, provider, architect, session)
            if result:
                generated_plugins.append(result)
            logger.info("")  # Blank line between files

    if not generated_plugins:
        logger.error("✗ No plugins were successfully generated!")
        worker.stop()
        worker_thread.join(timeout=5)
        sys.exit(1)

    logger.info(f"✓ Successfully generated {len(generated_plugins)} plugin(s)")
    logger.info("")

    # Hot reload plugins
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
        logger.warning("⚠ No jobs were queued (check routing rules)")
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
    display_results(SQLITE_OUTPUT, generated_plugins)


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
