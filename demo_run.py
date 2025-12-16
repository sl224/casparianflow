
import time
import sys
import logging
from pathlib import Path
from sqlalchemy import create_engine, select, text
from sqlalchemy.orm import Session

# Add src to path
sys.path.append(str(Path.cwd() / "src"))

from casparian_flow.db.base_session import Base
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    PluginConfig, TopicConfig, RoutingRule, ProcessingJob, StatusEnum, PluginManifest
)
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.engine.zmq_worker import ZmqWorker
from casparian_flow.services.scout import Scout
from casparian_flow.services.fs_engine import ParallelFileScanner

# Setup Logging
logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(name)s: %(message)s")
logger = logging.getLogger("demo")

# CONFIG
TARGET_DIR = Path(r"C:\Users\shan\workspace\E2D_ETL\tests\static_assets\zips\169069_20250203_004745_025_TransportRSM.fpkg.e2d")
DB_PATH = Path.cwd() / "demo.db"
PARQUET_ROOT = Path.cwd() / "demo_output"
PLUGINS_DIR = Path.cwd() / "plugins"

def setup_db(engine):
    initialize_database(engine, reset_tables=True)
    
    with Session(engine) as session:
        # 1. Register SourceRoot
        root_id = get_or_create_sourceroot(engine, str(TARGET_DIR))
        
        # 2. Register Plugin Config
        p_conf = PluginConfig(plugin_name="csv_processor")
        session.add(p_conf)
        
        # 3. Register Topic Config (Output)
        # We output to a local parquet dataset
        t_conf = TopicConfig(
            plugin_name="csv_processor",
            topic_name="output",
            uri=f"parquet://e2d_data",
            mode="infer"
        )
        session.add(t_conf)
        
        # 4. Routing Rule: *.csv -> csv_processor
        # User said "text files", and listing showed .csv. 
        # We'll match *.* to be safe or just specific patterns if needed.
        # Let's match *.csv for now based on file listing.
        rule = RoutingRule(
            pattern="*.csv",
            tag="csv_processor",
            priority=100
        )
        session.add(rule)
        session.commit()
    
    logger.info("Database initialized and configured.")

def create_plugin_code():
    return """
import pandas as pd
import pyarrow as pa
from casparian_flow.sdk import BasePlugin

class Handler(BasePlugin):
    def execute(self, file_path: str):
        print(f"Processing {file_path}...")
        try:
            # Attempt to read as CSV
            # backend='pyarrow' is faster and zero-copy friendly
            df = pd.read_csv(file_path, engine="pyarrow")
            
            # Convert to Arrow Table explicitly
            table = pa.Table.from_pandas(df)
            
            # Publish
            self.publish("output", table)
        except Exception as e:
            print(f"Failed to process {file_path}: {e}")
            raise
"""

def run_demo():
    # 1. Setup Environment
    if DB_PATH.exists():
        DB_PATH.unlink()
    
    engine = create_engine(f"sqlite:///{DB_PATH}")
    setup_db(engine)
    
    # 2. Start Worker
    w_config = WorkerConfig(
        database=DatabaseConfig(connection_string=f"sqlite:///{DB_PATH}"),
        storage=StorageConfig(parquet_root=PARQUET_ROOT),
        plugins=PluginsConfig(directory=PLUGINS_DIR)
    )
    
    # Use a generic port
    worker = ZmqWorker(w_config, zmq_addr="tcp://127.0.0.1:6000")
    
    # Run worker in background thread? Or just start it non-blocking if possible?
    # ZmqWorker.run is blocking. We should run it in a thread.
    import threading
    t = threading.Thread(target=worker.run, args=(100,), daemon=True)
    t.start()
    
    time.sleep(1) # Wait for startup
    
    # 3. Deploy Plugin via AI Workflow
    logger.info("Generating Plugin via AI...")
    
    # Simulate CLI call
    from casparian_flow.services.inspector import profile_file
    from casparian_flow.services.ai_hook import MockGenerator
    from casparian_flow.services.ai_types import PluginCode
    
    # 3a. Inspect
    # Create dummy file if not exists
    dummy_csv = TARGET_DIR / "ConditionFaultCodeList.csv"
    if not dummy_csv.exists():
        # Fallback for demo if E2D path not valid
        dummy_csv = Path("demo_data.csv")
        with open(dummy_csv, "w") as f:
            f.write("id,code,desc\n1,E001,Fault")
            
    profile = profile_file(str(dummy_csv))
    
    # 3b. Generate
    gen = MockGenerator()
    proposal = gen.propose_schema(profile)
    logger.info(f"AI Proposal: {proposal}")
    
    code = gen.generate_plugin(proposal)
    
    # 3c. Write to Disk (Plugin-as-Data)
    # The SystemDeployer (running in worker) should pick this up
    plugin_path = PLUGINS_DIR / "generated_plugin.py"
    with open(plugin_path, "w") as f:
        f.write(code.source_code)
        
    logger.info(f"Plugin written to {plugin_path}. Waiting for SystemDeployer...")
    time.sleep(5) # Give time for watcher to trigger
    
    # Trigger hot reload manually just in case file watcher isn't instantaneous
    worker.reload_plugins()
    time.sleep(2)
    
    # 4. Run Scout to find files
    logger.info(f"Scanning {TARGET_DIR}...")
    scout = Scout(engine)
    scanner = ParallelFileScanner()
    
    # Manually flush batch for the demo
    files_found = 0
    batch = []
    # Use walk() with a simple filter (accept all for demo, Scout filters later)
    for info in scanner.walk(TARGET_DIR, lambda e: True):
        files_found += 1
        batch.append(info)
    
    if batch:
        scout._flush_batch(batch)
        
    logger.info(f"Scout found {files_found} files.")
    
    # 5. Monitor Queue
    logger.info("Waiting for processing...")
    timeout = 30
    start = time.time()
    
    while time.time() - start < timeout:
        with Session(engine) as session:
            pending = session.query(ProcessingJob).filter(
                ProcessingJob.status.in_([StatusEnum.QUEUED, StatusEnum.RUNNING])
            ).count()
            
            completed = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.COMPLETED
            ).count()
            
            failed = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.FAILED
            ).count()
            
            logger.info(f"Jobs: Pending={pending}, Completed={completed}, Failed={failed}")
            
            if pending == 0 and (completed + failed) > 0:
                break
                
            if pending == 0 and files_found == 0:
                break
                
        time.sleep(1)
        
    # 6. Stop
    worker.stop()
    t.join(timeout=2)
    
    # 7. Verify Results
    logger.info("=== VERIFICATION ===")
    
    output_path = PARQUET_ROOT / "e2d_data"
    if output_path.exists():
        import pandas as pd
        df = pd.read_parquet(output_path)
        print("\nProcessed Data Preview:")
        print(df.head())
        print(f"\nTotal Rows: {len(df)}")
        print(f"Columns: {df.columns.tolist()}")
    else:
        logger.error("No output parquet found!")

if __name__ == "__main__":
    run_demo()
