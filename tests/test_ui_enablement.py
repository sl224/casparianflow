import pytest
import time
import threading
from datetime import datetime
import json
import zmq
from pathlib import Path
from sqlalchemy import create_engine, inspect, exc, text
from sqlalchemy.orm import Session
from fastapi.testclient import TestClient

from casparian_flow.db.models import (
    FileLocation, FileTag, PluginSubscription, ProcessingJob, 
    PluginConfig, StatusEnum, SourceRoot, FileVersion, FileHashRegistry
)
from casparian_flow.engine.sentinel import Sentinel
from casparian_flow.engine.worker_client import GeneralistWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.server.api import app, signal_sentinel_reload, get_db
from casparian_flow.db.setup import initialize_database

client = TestClient(app)

@pytest.fixture
def env(tmp_path):
    """Sets up the isolated test environment."""
    root = tmp_path / "root"
    root.mkdir()
    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()
    parquet_out = tmp_path / "output"
    parquet_out.mkdir()
    db_path = tmp_path / "test.db"
    conn_str = f"sqlite:///{db_path}"
    
    engine = create_engine(conn_str)
    initialize_database(engine, reset_tables=True)

    # Setup Config
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
        "zmq_port_sentinel": 5555 # Using default for simplicity in test
    }

def test_db_migration_hardened(env):
    """Verify new tables and constraints."""
    inspector = inspect(env["engine"])
    tables = inspector.get_table_names()
    
    assert "cf_file_tag" in tables
    assert "cf_plugin_subscription" in tables
    
    # Test Unique Constraint on Subscription
    with Session(env["engine"]) as s:
        # Create Plugin Config first
        p = PluginConfig(plugin_name="test_plugin")
        s.add(p)
        s.commit()
        
        # Insert first sub
        s.add(PluginSubscription(plugin_name="test_plugin", topic_name="sales"))
        s.commit()
        
        # Insert duplicate sub -> Should Fail
        try:
            s.add(PluginSubscription(plugin_name="test_plugin", topic_name="sales"))
            s.commit()
            pytest.fail("UniqueConstraint failed! Duplicate subscription allowed.")
        except exc.IntegrityError:
            s.rollback()
            pass # Expected

def test_config_overrides_execution(env):
    """Verify ad-hoc job execution with overrides."""
    # 1. Setup Plugin
    plugin_code = """
from casparian_flow.sdk import BasePlugin, PluginMetadata
import pyarrow as pa

MANIFEST = PluginMetadata(
    pattern="*.csv",
    topic="output",
    version="1.0"
)

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Yield a simple Arrow table
        table = pa.table({"val": [1]})
        yield table
"""
    (env["plugins"] / "override_test.py").write_text(plugin_code)
    
    # 2. Start Sentinel & Worker
    sentinel = Sentinel(env["config"], bind_addr="tcp://127.0.0.1:5555")
    t_sentinel = threading.Thread(target=sentinel.run, daemon=True)
    t_sentinel.start()
    
    worker = GeneralistWorker(
        "tcp://127.0.0.1:5555", env["plugins"], env["engine"], parquet_root=env["parquet"]
    )
    t_worker = threading.Thread(target=worker.start, daemon=True)
    t_worker.start()
    time.sleep(2) # Wait for startup
    
    # 3. Create Dummy File & Version in DB (Mocking Scout)
    with Session(env["engine"]) as s:
        sr = SourceRoot(path=str(env["root"]))
        s.add(sr)
        s.commit()
        
        fl = FileLocation(source_root_id=sr.id, rel_path="test.csv", filename="test.csv")
        s.add(fl)
        s.flush()
        
        fh = FileHashRegistry(content_hash="hash123", size_bytes=10)
        s.add(fh)
        
        fv = FileVersion(location_id=fl.id, content_hash="hash123", size_bytes=10, modified_time=datetime.now())
        s.add(fv)
        s.commit()
        file_id = fv.id

    # 4. Submit Job with Overrides via API Logic (simulated)
    # We want to override 'output' topic to a specific SQLite file
    override_db_path = env["root"] / "override.db"
    overrides = {
        "output": {
            "uri": f"sqlite:///{override_db_path}/override_table",
            "mode": "replace"
        }
    }
    
    # Push job directly using queue logic (or client if we ran uvicorn, but direct call is easier)
    from casparian_flow.engine.queue import JobQueue
    queue = JobQueue(env["engine"])
    queue.push_job(file_id, "override_test", priority=10, overrides=overrides)
    
    # 5. Wait for finish
    success = False
    for _ in range(50):
        with Session(env["engine"]) as s:
            job = s.query(ProcessingJob).filter_by(file_version_id=file_id).first()
            if job and job.status == StatusEnum.COMPLETED:
                success = True
                break
            if job and job.status == StatusEnum.FAILED:
                pytest.fail(f"Job Failed: {job.error_message}")
        time.sleep(0.1)
        
    assert success, "Job did not complete"
    
    # 6. Verify Result
    # Parquet SHOULD NOT exist (default)
    assert not list(env["parquet"].glob("*.parquet"))
    
    # SQLite SHOULD exist (override)
    assert override_db_path.exists()
    
    # Verify Content
    eng = create_engine(f"sqlite:///{override_db_path}")
    with eng.connect() as conn:
        res = conn.execute(text("SELECT * FROM override_table")).fetchall()
        assert len(res) == 1
        assert res[0][0] == 1 # val column

    # Cleanup
    sentinel.stop()
    worker.stop()

def test_api_tags(env):
    """Verify API Tagging endpoints."""
    # Mock DB session provider
    # We can just skip running full API app and test logic or use TestClient with dependency override.
    # For now, let's test DB logic via API endpoint call through client?
    # Dependency override is cleaner.
    
    def override_get_db():
        with Session(env["engine"]) as s:
            yield s
            
    app.dependency_overrides[get_db] = override_get_db
    
    # Create File
    with Session(env["engine"]) as s:
        sr = SourceRoot(path=str(env["root"]))
        s.add(sr)
        s.commit()
        fl = FileLocation(source_root_id=sr.id, rel_path="tag_test.csv", filename="tag_test.csv")
        s.add(fl)
        s.commit()
        fid = fl.id
        
    # Add Tag
    resp = client.post(f"/files/{fid}/tags?tag=URGENT")
    assert resp.status_code == 200
    
    # Verify in DB
    with Session(env["engine"]) as s:
        tags = s.query(FileTag).filter_by(file_id=fid).all()
        assert len(tags) == 1
        assert tags[0].tag == "URGENT"
        
    # Browse Files by Tag
    resp = client.get("/files?tag=URGENT")
    assert resp.status_code == 200
    data = resp.json()
    assert len(data) == 1
    assert data[0]["filename"] == "tag_test.csv"
    assert "URGENT" in data[0]["tags"]
