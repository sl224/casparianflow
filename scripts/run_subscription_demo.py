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
from casparian_flow.db.models import SourceRoot, RoutingRule
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
    
    # Validation for demo safety
    if not RAW_SOURCE_FILE.exists():
        RAW_SOURCE_FILE.parent.mkdir(parents=True, exist_ok=True)
        RAW_SOURCE_FILE.write_text("Dummy,Data,123", encoding="utf-8")
        logger.warning(f"Created dummy source file at {RAW_SOURCE_FILE}")

    TEST_ROOT = Path("test_env_subscription")
    if TEST_ROOT.exists(): shutil.rmtree(TEST_ROOT)
    TEST_ROOT.mkdir()
    
    # [NOTE] If you want to persist plugins between runs, comment out the rmtree line below
    PLUGINS_DIR = Path("plugins_sub")
    if PLUGINS_DIR.exists(): shutil.rmtree(PLUGINS_DIR)
    PLUGINS_DIR.mkdir(exist_ok=True)
    
    DB_PATH = Path("subscription_test.db")
    TARGET_FILE = TEST_ROOT / RAW_SOURCE_FILE.name
    shutil.copy(RAW_SOURCE_FILE, TARGET_FILE)
    
    # 2. Init DB
    engine = create_engine(f"sqlite:///{DB_PATH}")
    initialize_database(engine, reset_tables=True)
    
    # 3. Create Routing Rule
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
    # 4a. Schema Proposal (Skippable)
    # ==========================================
    proposal = None
    if prompt_yes_no("\nRun schema proposal generation?"):
        
        # --- PRE-INPUT CONTEXT ---
        print(f"\n{'-'*60}")
        print("CONTEXT CONFIGURATION")
        print(f"{'-'*60}")
        print("Enter any specific constraints for the schema (e.g., 'Only create schemas for pfc_db: type data').")
        user_context = input("Context (press Enter to skip) > ").strip()
        
        schema_feedback = user_context if user_context else None
        
        while True:
            try:
                print("\nGenerating Schema Proposal...")
                # We pass the context/feedback here
                proposal = generator.propose_schema(profile, user_feedback=schema_feedback)
                
                print("\n" + "="*60)
                print("SCHEMA PROPOSAL")
                print("="*60)
                print(f"File Type Inferred: {proposal.file_type_inferred}")
                print(f"Read Strategy:      {proposal.read_strategy}")
                print("-" * 60)
                
                if proposal.tables:
                    for t in proposal.tables:
                        print(f"Table: {t.topic_name}")
                        print(f"Desc:  {t.description}")
                        print("Columns:")
                        for c in t.columns:
                            print(f"  - {c.name: <25} {c.target_type: <10} {c.description or ''}")
                        print("-" * 30)
                else:
                    print("No tables proposed.")
                    
                print(f"\nReasoning: {proposal.reasoning}")
                print("=" * 60)
                
                choice = input("Approve Schema? [Y/n] or enter feedback > ").strip()
                if choice.lower() in ['y', 'yes', '']:
                    break
                schema_feedback = choice # Update feedback for next loop
            except Exception as e:
                logger.error(f"Schema Gen Failed: {e}")
                schema_feedback = input("Retry hint? > ")
    else:
        logger.info("Skipping Schema Proposal generation.")

    # ==========================================
    # 4b. Plugin Generation (Skippable)
    # ==========================================
    code = None
    if prompt_yes_no("\nRun plugin generation?"):
        if proposal is None:
            logger.error("Cannot generate plugin: No schema proposal available (Step skipped).")
        else:
            # We pre-seed feedback to ensure subscription matches Routing Rule
            code_feedback = "Subscribe to 'raw_mcdata_events'. Use BasePlugin from casparian_flow.sdk."
            
            while True:
                try:
                    print("\nGenerating Plugin Code...")
                    code = generator.generate_plugin(proposal, user_feedback=code_feedback)
                    
                    print("\n" + "="*40)
                    print("GENERATED CODE")
                    print("="*40)
                    print("\n".join(code.source_code.splitlines()[:20]))
                    print("... (truncated) ...")
                    print("-" * 40)
                    
                    choice = input("Approve Code? [Y/n] or enter feedback > ").strip()
                    if choice.lower() in ['y', 'yes', '']:
                        break
                    code_feedback = choice
                except Exception as e:
                    logger.error(f"Code Gen Failed: {e}")
                    code_feedback = input("Retry hint? > ")

            if code:
                plugin_path = PLUGINS_DIR / code.filename
                plugin_path.write_text(code.source_code, encoding="utf-8")
                logger.info(f"Plugin written to {plugin_path}")
    else:
        logger.info("Skipping Plugin Generation.")

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
    max_wait = 10
    found_output = False
    
    for i in range(max_wait):
        out_dir = Path("sub_output")
        if out_dir.exists() and list(out_dir.rglob("*.parquet")):
            print("\nSUCCESS: Output files generated!")
            for f in out_dir.rglob("*.parquet"):
                print(f" - {f}")
            found_output = True
            break
        time.sleep(1)
        print(".", end="", flush=True)
    
    if not found_output:
        print("\nNote: No output files found. (This is expected if Plugin Generation was skipped)")

    sentinel.stop()
    worker.stop()
    st.join(timeout=1)
    wt.join(timeout=1)

if __name__ == "__main__":
    main()