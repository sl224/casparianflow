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

def prompt_yes_no(question, default="y"):
    """Helper for yes/no prompts"""
    valid = {"yes": True, "y": True, "ye": True, "no": False, "n": False}
    prompt = " [Y/n] " if default == "y" else " [y/N] "
    while True:
        sys.stdout.write(question + prompt)
        choice = input().lower().strip()
        if choice == "":
            return True if default == "y" else False
        if choice in valid:
            return valid[choice]
        sys.stdout.write("Please respond with 'yes' or 'no' (or 'y'/'n').\n")

def main():
    # 1. Config
    RAW_SOURCE_FILE = Path(r"tests/static_assets/zips/169069_20250203_004745_025_TransportRSM.fpkg.e2d/169069_20250203_004745_025_MCData")
    
    if not RAW_SOURCE_FILE.exists():
        logger.error(f"File not found: {RAW_SOURCE_FILE}")
        return

    DEMO_ROOT = Path(".demo_output/subscription")
    if DEMO_ROOT.exists(): shutil.rmtree(DEMO_ROOT)
    DEMO_ROOT.mkdir(parents=True)

    TEST_ROOT = DEMO_ROOT / "files"
    TEST_ROOT.mkdir()

    PLUGINS_DIR = DEMO_ROOT / "plugins"
    PLUGINS_DIR.mkdir()

    DB_PATH = DEMO_ROOT / "subscription_test.db"
    TARGET_FILE = TEST_ROOT / RAW_SOURCE_FILE.name
    shutil.copy(RAW_SOURCE_FILE, TARGET_FILE)
    
    # 2. Init DB
    engine = create_engine(f"sqlite:///{DB_PATH}")
    initialize_database(engine, reset_tables=True)
    
    # 3. Create Routing Rule
    # This connects the File Pattern (*MCData*) to the Topic (raw_mcdata_events)
    with Session(engine) as session:
        rule = RoutingRule(pattern="*MCData*", tag="raw_mcdata_events", priority=100)
        session.add(rule)
        session.commit()
        logger.info("Routing Rule Created: *MCData* -> raw_mcdata_events")

    # Initialize Generator
    profile = profile_file(str(TARGET_FILE))
    provider = get_provider("claude-cli") 
    generator = LLMGenerator(provider)
    
    # ==========================================
    # 4a. Schema Proposal
    # ==========================================
    proposal = None
    schema_feedback = "Only create schemas for pfc_db: type data if possible."
    
    while True:
        try:
            print("\nGenerating Schema Proposal...")
            proposal = generator.propose_schema(profile, user_feedback=schema_feedback)
            
            print("\n" + "="*60)
            print(f"Strategy: {proposal.read_strategy}")
            print(f"Reasoning: {proposal.reasoning}")
            print("-" * 60)
            
            if proposal.tables:
                for t in proposal.tables:
                    print(f"Table: {t.topic_name}")
                    print(f"Desc:  {t.description}")
                    print("Columns:")
                    for c in t.columns[:5]: # Show first 5 cols
                        print(f"  - {c.name} ({c.target_type})")
                    if len(t.columns) > 5: print("  ... (more)")
                    print("-" * 30)
            else:
                print("No tables proposed.")
            print("=" * 60)
            
            choice = input("Approve Schema? [Y/n/r] (r=Refine) > ").strip().lower()
            if choice in ['y', 'yes', '']:
                break
            elif choice == 'r':
                schema_feedback = input("Enter hint > ").strip()
            else:
                logger.info("Skipped.")
                return
        except Exception as e:
            logger.error(f"Schema Gen Failed: {e}")
            return

    # ==========================================
    # 4b. Plugin Generation
    # ==========================================
    code = None
    # Pre-seed instruction to ensure subscription matches Routing Rule
    code_feedback = "The input topic is 'raw_mcdata_events'. Subscribe to that."
    
    while True:
        try:
            print("\nGenerating Plugin Code...")
            code = generator.generate_plugin(proposal, user_feedback=code_feedback, example_path=str(TARGET_FILE))
            
            print("\n" + "="*40)
            print("GENERATED CODE")
            print("="*40)
            print(code.source_code)
            print("-" * 40)
            
            choice = input("Approve Code? [Y/n/r] (r=Refine) > ").strip().lower()
            if choice in ['y', 'yes', '']:
                break
            elif choice == 'r':
                code_feedback = input("Enter hint > ").strip()
            else:
                logger.info("Skipped.")
                return
        except Exception as e:
            logger.error(f"Code Gen Failed: {e}")
            return

    if code:
        plugin_path = PLUGINS_DIR / code.filename
        plugin_path.write_text(code.source_code, encoding="utf-8")
        logger.info(f"Plugin written to {plugin_path}")

    # ==========================================
    # 5. Start Infrastructure
    # ==========================================
    print("\nStarting Engine...")
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
    max_wait = 15
    
    for i in range(max_wait):
        with Session(engine) as session:
            job = session.query(ProcessingJob).first()
            if job and job.status in [StatusEnum.COMPLETED, StatusEnum.FAILED]:
                logger.info(f"Job Status: {job.status}")
                if job.status == StatusEnum.FAILED:
                    logger.error(f"Error: {job.error_message}")
                break
        time.sleep(1)
        print(".", end="", flush=True)
    
    # Check Output
    out_dir = Path("sub_output")
    if out_dir.exists() and list(out_dir.rglob("*.parquet")):
        print("\nSUCCESS: Output files generated!")
        for f in out_dir.rglob("*.parquet"):
            print(f" - {f}")
    else:
        print("\nFAILURE: No output files found.")

    sentinel.stop()
    worker.stop()
    st.join(timeout=1)
    wt.join(timeout=1)

if __name__ == "__main__":
    main()