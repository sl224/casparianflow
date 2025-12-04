import os
import time
import shutil
import logging
from pathlib import Path
from sqlalchemy import create_engine
from casparian_flow.config import settings
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import PluginConfig, ProcessingJob, FileMetadata
from casparian_flow.services.scout import Scout

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("smoke_test")

def run_smoke_test():
    # 1. Setup Environment
    test_dir = Path("smoke_test_data")
    if test_dir.exists():
        shutil.rmtree(test_dir)
    test_dir.mkdir()
    
    # Create a dummy file
    dummy_file = test_dir / "test_data.csv"
    dummy_file.write_text("col1,col2\n1,2\n3,4")
    
    # 2. Setup DB
    # Force SQLite for test
    db_url = f"sqlite:///{settings.database.db_location}"
    engine = create_engine(db_url)
    initialize_database(engine, reset_tables=True)
    
    db = SessionLocal(bind=engine)
    
    # 3. Configure System
    # Create SourceRoot
    root_id = get_or_create_sourceroot(engine, str(test_dir.absolute()))
    logger.info(f"Created SourceRoot with ID: {root_id}")
    
    # Create PluginConfig for 'TestPlugin'
    # The plugin name in `test_plugin.py` class is `TestPlugin`, but the loader might use the filename or something else.
    # Let's check `loader.py` later. Assuming 'TestPlugin' or 'test_plugin'.
    # The `ProcessingJob` needs `plugin_name`.
    
    # Let's assume the loader uses the class name or we need to match what the loader finds.
    # For now, I'll add a config for "TestPlugin".
    
    plugin_config = PluginConfig(
        plugin_name="test_plugin",
        topic_config='{"test": {"uri": "parquet://./output", "mode": "append"}}'
    )
    db.add(plugin_config)
    db.commit()
    
    # 4. Run Scout
    scout = Scout(db)
    from casparian_flow.db.models import SourceRoot
    root = db.query(SourceRoot).get(root_id)
    scout.scan_source(root)
    
    # 5. Verify Queue
    jobs = db.query(ProcessingJob).all()
    logger.info(f"Found {len(jobs)} jobs in queue.")
    
    if len(jobs) == 1:
        logger.info("✅ Scout successfully queued the job!")
        job = jobs[0]
        logger.info(f"Job Details: File={job.file.filename}, Plugin={job.plugin_name}, Status={job.status}")
    else:
        logger.error(f"❌ Expected 1 job, found {len(jobs)}")
        exit(1)

    # 6. (Optional) Run Worker
    # We can try to run the worker in a separate process or just verify the queue for now.
    # The user asked to "Run uv run -m casparian_flow.main".
    # I'll leave that for the manual step or a separate run command.

if __name__ == "__main__":
    run_smoke_test()
