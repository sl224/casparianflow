import pytest
import time
import threading
import socket
import pandas as pd
from pathlib import Path
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

# Import the new components
from casparian_flow.engine.sentinel import Sentinel
from casparian_flow.engine.worker_client import GeneralistWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.services.scout import Scout
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import ProcessingJob, StatusEnum, RoutingRule, TopicConfig

def get_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

@pytest.fixture
def env(tmp_path):
    """Sets up the isolated test environment (DB, Dirs, Config)."""
    # 1. Directories
    root = tmp_path / "root"
    root.mkdir()
    
    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()
    
    parquet_out = tmp_path / "output"
    parquet_out.mkdir()
    
    # 2. Database
    db_path = tmp_path / "test.db"
    conn_str = f"sqlite:///{db_path}"
    engine = create_engine(conn_str)
    initialize_database(engine, reset_tables=True)
    
    # 3. Source Root (The watched folder)
    source_root_id = get_or_create_sourceroot(engine, str(root))
    
    # 4. Config Object
    config = WorkerConfig(
        database=DatabaseConfig(connection_string=conn_str),
        storage=StorageConfig(parquet_root=parquet_out),
        plugins=PluginsConfig(dir=plugins_dir)
    )
    
    return {
        "root": root,
        "plugins": plugins_dir,
        "parquet": parquet_out,
        "engine": engine,
        "config": config,
        "source_root_id": source_root_id,
        "zmq_port": get_free_port()
    }

def test_generalist_flow_e2e(env):
    """
    E2E Test:
    1. Write a plugin (defines the Tagger/RoutingRule via comments).
    2. Start Sentinel & Worker (Worker auto-registers the rule).
    3. Drop a file (Scout finds it -> matches Rule -> Queues Job).
    4. Sentinel dispatches to Worker -> Worker processes -> Output generated.
    """
    
    # --- Step 1: Write Plugin "The Tagger" ---
    # We define a plugin that handles *.gtest files and outputs to 'generalist_out'
    plugin_code = """
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.gtest",
    topic="generalist_out",
    version="1.0"
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Read the dummy file (CSV format)
        df = pd.read_csv(file_path)

        # Add a proof-of-processing column
        df['worker_type'] = 'GENERALIST'

        # Yield as Arrow Table
        yield pa.Table.from_pandas(df)
"""
    (env["plugins"] / "test_processor.py").write_text(plugin_code, encoding="utf-8")

    # --- Step 2: Start Infrastructure ---
    sentinel_addr = f"tcp://127.0.0.1:{env['zmq_port']}"
    
    # A. Start Sentinel (Broker)
    sentinel = Sentinel(env["config"], bind_addr=sentinel_addr)
    t_sentinel = threading.Thread(target=sentinel.run, daemon=True)
    t_sentinel.start()
    
    # B. Start Worker (Client)
    # This triggers 'register_plugins_from_source', writing RoutingRules to DB
    worker = GeneralistWorker(
        sentinel_addr, env["plugins"], env["engine"], parquet_root=env["parquet"]
    )
    t_worker = threading.Thread(target=worker.start, daemon=True)
    t_worker.start()
    
    # Wait for Registration (Worker -> Sentinel) and Auto-Configuration (Worker -> DB)
    time.sleep(2)
    
    # VERIFY: Did the worker register the routing rule?
    with Session(env["engine"]) as session:
        rule = session.query(RoutingRule).filter_by(pattern="*.gtest").first()
        assert rule is not None, "Worker failed to auto-register RoutingRule from plugin MANIFEST!"
        print(f"[OK] Auto-Registered Rule: {rule.pattern} -> {rule.tag}")

    # --- Step 3: Push Dummy File (The Trigger) ---
    data_file = env["root"] / "input.gtest"
    data_file.write_text("id,val\n1,100\n2,200")
    
    # --- Step 4: Run Scout ---
    # Scout scans dir, matches the new rule, creates FileVersion, queues ProcessingJob
    with Session(env["engine"]) as session:
        # We need to get the SourceRoot object to pass to Scout
        from casparian_flow.db.models import SourceRoot
        sr = session.get(SourceRoot, env["source_root_id"])
        
        scout = Scout(session)
        scout.scan_source(sr)
        
        # Verify Job Queued
        job = session.query(ProcessingJob).first()
        assert job is not None, "Scout failed to queue job"
        assert job.status == StatusEnum.QUEUED
        print(f"[OK] Job Queued: {job.id} for plugin {job.plugin_name}")

    # --- Step 5: Wait for Execution ---
    # Sentinel should pick up the QUEUED job and dispatch it to the Worker
    print("[WAIT] Waiting for Sentinel to process job...")

    success = False
    for _ in range(50): # 5 seconds timeout
        with Session(env["engine"]) as session:
            job = session.query(ProcessingJob).first()
            if job.status == StatusEnum.COMPLETED:
                success = True
                break
            elif job.status == StatusEnum.FAILED:
                pytest.fail(f"Job Failed: {job.error_message}")
        time.sleep(0.1)

    assert success, "Job did not complete in time"
    print("[OK] Job Completed")

    # --- Step 6: Verify Data Output ---
    # The Worker should have written parquet to the configured root
    # Let's check what files were created
    print(f"[DEBUG] Parquet root: {env['parquet']}")
    print(f"[DEBUG] Files created: {list(env['parquet'].rglob('*'))}")

    # Topic was 'generalist_out', so file is output/generalist_out.parquet
    output_file = env["parquet"] / "generalist_out.parquet"
    assert output_file.exists(), f"Output file was not created. Expected: {output_file}"

    df = pd.read_parquet(output_file)
    print("[OK] Output Data Found:\n", df)
    
    assert len(df) == 2
    assert "worker_type" in df.columns
    assert df.iloc[0]["worker_type"] == "GENERALIST"

    # Cleanup
    sentinel.running = False
    # Worker thread is daemon, will die with test