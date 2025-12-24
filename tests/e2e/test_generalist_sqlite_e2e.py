import pytest
import time
import threading
import socket
import pandas as pd
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

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
def sqlite_env(tmp_path):
    # 1. Setup Metadata DB
    meta_db_path = tmp_path / "metadata.db"
    conn_str = f"sqlite:///{meta_db_path}"
    engine = create_engine(conn_str)
    initialize_database(engine, reset_tables=True)

    # 2. Setup Output DB (Sink)
    output_db_path = tmp_path / "sink_output.db"
    
    # 3. Directories
    root = tmp_path / "root"
    root.mkdir()
    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()

    # 4. Source Root
    source_root_id = get_or_create_sourceroot(engine, str(root))

    return {
        "root": root,
        "plugins": plugins_dir,
        "engine": engine,
        "output_db_url": f"sqlite:///{output_db_path}",
        "output_db_path": output_db_path,
        "source_root_id": source_root_id,
        "zmq_port": get_free_port(),
        "conn_str": conn_str
    }

def test_pull_arch_to_sqlite(sqlite_env):
    """
    Verifies Sentinel -> Worker -> SQLite Sink flow.
    """
    # 1. Define Plugin with SQLite Topic Config via MANIFEST
    # Note: We use a relative path for the DB in the topic URI
    plugin_code = f"""
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.sqltest",
    topic="output",
    version="1.0"
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.read_csv(file_path)
        # Publish to 'output' topic
        self.publish("output", pa.Table.from_pandas(df))
"""
    (sqlite_env["plugins"] / "sql_plugin.py").write_text(plugin_code, encoding="utf-8")

    # 2. Manually Override Topic Config in DB to point to our test SQLite DB
    with Session(sqlite_env["engine"]) as session:
        from casparian_flow.db.models import PluginConfig
        p_conf = PluginConfig(plugin_name="sql_plugin", subscription_tags="auto_sql_plugin")
        session.add(p_conf)
        
        # Override the topic to point to SQLite
        t_conf = TopicConfig(
            plugin_name="sql_plugin",
            topic_name="output",
            uri=f"{sqlite_env['output_db_url']}/results_table",
            mode="append"
        )
        session.add(t_conf)
        session.commit()

    # 3. Start Infrastructure
    config = WorkerConfig(
        database=DatabaseConfig(connection_string=sqlite_env["conn_str"]),
        storage=StorageConfig(parquet_root=sqlite_env["root"]), # Unused for SQL
        plugins=PluginsConfig(dir=sqlite_env["plugins"])
    )
    
    sentinel_addr = f"tcp://127.0.0.1:{sqlite_env['zmq_port']}"
    sentinel = Sentinel(config, bind_addr=sentinel_addr)
    st = threading.Thread(target=sentinel.run, daemon=True)
    st.start()
    
    worker = GeneralistWorker(
        sentinel_addr, sqlite_env["plugins"], sqlite_env["engine"], parquet_root=sqlite_env["root"]
    )
    wt = threading.Thread(target=worker.start, daemon=True)
    wt.start()
    
    time.sleep(2) # Warmup

    # 4. Ingest Data
    data_file = sqlite_env["root"] / "data.sqltest"
    data_file.write_text("id,val\n10,A\n20,B")
    
    with Session(sqlite_env["engine"]) as session:
        # Manual Rule Injection if registrar didn't overwrite our pre-seed
        if not session.query(RoutingRule).filter_by(pattern="*.sqltest").first():
             session.add(RoutingRule(pattern="*.sqltest", tag="auto_sql_plugin", priority=10))
             session.commit()

        # Run Scout
        from casparian_flow.db.models import SourceRoot
        sr = session.get(SourceRoot, sqlite_env["source_root_id"])
        Scout(session).scan_source(sr)

    # 5. Wait for Completion
    success = False
    for _ in range(50):
        with Session(sqlite_env["engine"]) as session:
            job = session.query(ProcessingJob).first()
            if job and job.status == StatusEnum.COMPLETED:
                success = True
                break
            elif job and job.status == StatusEnum.FAILED:
                pytest.fail(f"Job Failed: {job.error_message}")
        time.sleep(0.1)
        
    assert success, "Job timed out"

    # 6. Verify SQLite Data
    verify_engine = create_engine(sqlite_env["output_db_url"])
    with verify_engine.connect() as conn:
        # Check table existence
        tables = conn.execute(text("SELECT name FROM sqlite_master WHERE type='table' AND name='results_table'")).fetchall()
        assert len(tables) == 1
        
        # Check content and Lineage
        rows = conn.execute(text("SELECT * FROM results_table ORDER BY id")).fetchall()
        assert len(rows) == 2
        assert rows[0].val == "A"
        
        # Lineage Columns Verification
        cols = conn.execute(text("PRAGMA table_info(results_table)")).fetchall()
        col_names = [c[1] for c in cols]
        assert "_job_id" in col_names
        assert "_file_version_id" in col_names

    # Clean Shutdown
    sentinel.stop()
    worker.stop()
    st.join(timeout=2)
    wt.join(timeout=2)