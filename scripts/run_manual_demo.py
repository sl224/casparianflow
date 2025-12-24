# scripts/run_fanout_demo.py
import sys
import shutil
import logging
import time
import threading
import pandas as pd
from pathlib import Path
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

# Add src to path
sys.path.insert(0, str(Path.cwd() / "src"))

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import SourceRoot, ProcessingJob, StatusEnum, RoutingRule, TopicConfig
from casparian_flow.services.scout import Scout
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.engine.sentinel import Sentinel
from casparian_flow.engine.worker_client import GeneralistWorker

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("fanout_demo")

# --- Configuration ---
DEMO_ROOT = Path(".demo_output/fanout")
TEST_ROOT = DEMO_ROOT / "files"
PLUGINS_DIR = Path("tests/fixtures/plugins")  # Use consolidated fixtures
OUTPUT_DIR = DEMO_ROOT / "output"
DB_PATH = DEMO_ROOT / "fanout_test.db"
TARGET_FILE_SRC = Path(r"tests/static_assets/zips/169069_20250203_004745_025_TransportRSM.fpkg.e2d/169069_20250203_004745_025_MCData")

def setup_env():
    if DEMO_ROOT.exists(): shutil.rmtree(DEMO_ROOT)
    DEMO_ROOT.mkdir(parents=True)
    TEST_ROOT.mkdir()
    OUTPUT_DIR.mkdir()
    
    # Copy target file to isolated scan root
    if not TARGET_FILE_SRC.exists():
        logger.error(f"Source file not found: {TARGET_FILE_SRC}")
        sys.exit(1)
    
    target = TEST_ROOT / TARGET_FILE_SRC.name
    shutil.copy(TARGET_FILE_SRC, target)
    return target

def main():
    target_file = setup_env()
    
    # 1. Init Database
    db_url = f"sqlite:///{DB_PATH}"
    engine = create_engine(db_url)
    initialize_database(engine, reset_tables=True)

    # 2. Configure Routing & Fan-Out
    with Session(engine) as session:
        # A. Routing Rule: Map File -> Event Tag
        rule = RoutingRule(pattern="*MCData*", tag="raw_mcdata_events", priority=100)
        session.add(rule)
        
        # B. Topic Fan-Out: Map 'pfc_db_data' -> Parquet AND SQLite
        
        # Sink 1: Parquet (Already defined in Manifest, but explicit config overrides/confirms)
        sink_parquet = TopicConfig(
            plugin_name="generated_pfc_db", # Must match filename stem
            topic_name="pfc_db_data",
            uri="parquet://pfc_db_data.parquet",
            mode="append"
        )
        session.add(sink_parquet)
        
        # Sink 2: SQLite (The extra sink)
        # Note: In a real app, you might use a dedicated DB. Here we use the metadata DB for convenience.
        sink_sqlite = TopicConfig(
            plugin_name="generated_pfc_db",
            topic_name="pfc_db_data",
            uri=f"sqlite://{DB_PATH}/pfc_db_table", 
            mode="append"
        )
        session.add(sink_sqlite)
        
        session.commit()
        logger.info("Configured Fan-Out: 'pfc_db_data' -> [Parquet, SQLite]")

    # 3. Start Engine
    logger.info("Starting Engine...")
    w_config = WorkerConfig(
        database=DatabaseConfig(connection_string=db_url),
        storage=StorageConfig(parquet_root=OUTPUT_DIR),
        plugins=PluginsConfig(dir=PLUGINS_DIR)
    )

    sentinel = Sentinel(w_config, bind_addr="tcp://127.0.0.1:9300")
    st = threading.Thread(target=sentinel.run, daemon=True)
    st.start()

    worker = GeneralistWorker("tcp://127.0.0.1:9300", PLUGINS_DIR, engine)
    wt = threading.Thread(target=worker.start, daemon=True)
    wt.start()

    time.sleep(3) # Wait for registration

    # 4. Scan & Tag
    logger.info("Running Scout...")
    with Session(engine) as session:
        sid = get_or_create_sourceroot(engine, str(TEST_ROOT))
        sr = session.get(SourceRoot, sid)
        Scout(session).scan_source(sr)

    # 5. Wait for Completion
    logger.info("Waiting for processing...")
    for _ in range(15):
        with Session(engine) as session:
            job = session.query(ProcessingJob).first()
            if job and job.status in [StatusEnum.COMPLETED, StatusEnum.FAILED]:
                logger.info(f"Job Finished: {job.status}")
                if job.status == StatusEnum.FAILED:
                    logger.error(f"Error: {job.error_message}")
                break
        time.sleep(1)
        print(".", end="", flush=True)
    
    print("")

    # 6. Verify Outputs
    print("="*40)
    print("VERIFICATION")
    print("="*40)
    
    # Check Parquet
    pq_files = list(OUTPUT_DIR.rglob("*.parquet"))
    if pq_files:
        print(f"[SUCCESS] Parquet File Created: {pq_files[0]}")
        try:
            df = pd.read_parquet(pq_files[0])
            print(f"  Rows: {len(df)}")
        except: pass
    else:
        print("[FAILURE] No Parquet file found.")

    # Check SQLite
    with engine.connect() as conn:
        try:
            result = conn.execute(text("SELECT count(*) FROM pfc_db_table")).scalar()
            print(f"[SUCCESS] SQLite Table 'pfc_db_table' has {result} rows.")
            
            # Preview
            rows = conn.execute(text("SELECT * FROM pfc_db_table LIMIT 3")).fetchall()
            print("  Preview:")
            for r in rows:
                print(f"  {r}")
        except Exception as e:
            print(f"[FAILURE] SQLite verification failed: {e}")

    # Cleanup
    sentinel.stop()
    worker.stop()
    st.join(timeout=1)
    wt.join(timeout=1)

if __name__ == "__main__":
    main()