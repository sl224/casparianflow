# scripts/run_subscription_demo.py
import sys
import shutil
import logging
from pathlib import Path
from sqlalchemy import create_engine
from sqlalchemy.orm import Session
import threading
import time

sys.path.insert(0, str(Path.cwd() / "src"))

from casparian_flow.services.inspector import profile_file
from casparian_flow.services.llm_generator import LLMGenerator
from casparian_flow.services.llm_provider import get_provider
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import SourceRoot, ProcessingJob, StatusEnum, RoutingRule
from casparian_flow.services.scout import Scout
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.engine.sentinel import Sentinel
from casparian_flow.engine.worker_client import GeneralistWorker

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("demo")

def main():
    # 1. Config
    RAW_SOURCE_FILE = Path(r"tests/static_assets/zips/169069_20250203_004745_025_TransportRSM.fpkg.e2d/169069_20250203_004745_025_MCData")
    TEST_ROOT = Path("test_env_subscription")
    if TEST_ROOT.exists(): shutil.rmtree(TEST_ROOT)
    TEST_ROOT.mkdir()
    PLUGINS_DIR = Path("plugins_sub")
    PLUGINS_DIR.mkdir(exist_ok=True)
    DB_PATH = Path("subscription_test.db")
    
    TARGET_FILE = TEST_ROOT / RAW_SOURCE_FILE.name
    shutil.copy(RAW_SOURCE_FILE, TARGET_FILE)
    
    # 2. Init DB
    engine = create_engine(f"sqlite:///{DB_PATH}")
    initialize_database(engine, reset_tables=True)
    
    # 3. Create Routing Rule (The Binding)
    # This says: "Files named *MCData* generate 'raw_mcdata_events'"
    with Session(engine) as session:
        rule = RoutingRule(pattern="*MCData*", tag="raw_mcdata_events", priority=100)
        session.add(rule)
        session.commit()
        logger.info("Routing Rule Created: *MCData* -> raw_mcdata_events")

    # 4. Generate Plugin (AI)
    logger.info("Generating Plugin...")
    profile = profile_file(str(TARGET_FILE))
    provider = get_provider("claude-cli")
    generator = LLMGenerator(provider)
    
    proposal = generator.propose_schema(profile)
    # Hint AI to use the correct subscription
    code = generator.generate_plugin(proposal, user_feedback="Subscribe to 'raw_mcdata_events'")
    
    plugin_path = PLUGINS_DIR / code.filename
    plugin_path.write_text(code.source_code, encoding="utf-8")
    logger.info(f"Plugin written to {plugin_path}")

    # 5. Start Infrastructure
    w_config = WorkerConfig(
        database=DatabaseConfig(connection_string=f"sqlite:///{DB_PATH}"),
        storage=StorageConfig(parquet_root=Path("sub_output")),
        plugins=PluginsConfig(dir=PLUGINS_DIR)
    )
    
    sentinel = Sentinel(w_config, bind_addr="tcp://127.0.0.1:9100")
    st = threading.Thread(target=sentinel.run, daemon=True)
    st.start()
    
    worker = GeneralistWorker("tcp://127.0.0.1:9100", PLUGINS_DIR, engine)
    wt = threading.Thread(target=worker.start, daemon=True)
    wt.start()
    
    time.sleep(3)

    # 6. Run Scout
    with Session(engine) as session:
        sid = get_or_create_sourceroot(engine, str(TEST_ROOT))
        sr = session.get(SourceRoot, sid)
        Scout(session).scan_source(sr)

    # 7. Wait & Verify
    logger.info("Waiting for processing...")
    time.sleep(5)
    
    # Check Output
    out_dir = Path("sub_output")
    if out_dir.exists() and list(out_dir.rglob("*.parquet")):
        print("\nSUCCESS: Output files generated!")
        for f in out_dir.rglob("*.parquet"):
            print(f" - {f}")
    else:
        print("\nFAILURE: No output files.")

    sentinel.stop()
    worker.stop()
    st.join(timeout=1)
    wt.join(timeout=1)

if __name__ == "__main__":
    main()