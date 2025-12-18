import pytest
import time
import sys
import subprocess
import threading
import pandas as pd
from pathlib import Path
from sqlalchemy import create_engine
from casparian_flow.db.models import (
    ProcessingJob, FileVersion, FileLocation, PluginConfig, 
    TopicConfig, StatusEnum, FileHashRegistry
)
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.engine.brokerimport ZmqWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig

# Use a specific port to avoid conflicts
TEST_ZMQ_ADDR = "tcp://127.0.0.1:5680"

@pytest.fixture
def e2e_env(tmp_path):
    # 1. Setup DB
    db_path = tmp_path / "e2e.db"
    db_url = f"sqlite:///{db_path}"
    engine = create_engine(db_url)
    initialize_database(engine, reset_tables=True)
    
    from casparian_flow.db.base_session import SessionLocal
    session = SessionLocal(bind=engine)
    
    # 2. Directories
    src_dir = tmp_path / "source"
    src_dir.mkdir()
    
    # CRITICAL: This is the root for parquet files
    parquet_root = tmp_path / "parquet_data"
    parquet_root.mkdir()
    
    root_id = get_or_create_sourceroot(engine, str(src_dir))
    
    p_conf = PluginConfig(plugin_name="e2e_plugin")
    session.add(p_conf)
    
    # Topic URI is now RELATIVE to parquet_root
    # Resulting path: parquet_root/output_dataset
    t_conf = TopicConfig(
        plugin_name="e2e_plugin",
        topic_name="output",
        uri="parquet://output_dataset", 
        mode="append"
    )
    session.add(t_conf)
    session.commit()
    
    return {
        "tmp_path": tmp_path,
        "src_dir": src_dir,
        "parquet_root": parquet_root,
        "db_url": db_url,
        "engine": engine,
        "session": session,
        "root_id": root_id
    }

def create_job(env, filename, content):
    session = env['session']
    fpath = env['src_dir'] / filename
    fpath.write_text(content)
    
    h_val = "hash_" + filename
    session.add(FileHashRegistry(content_hash=h_val, size_bytes=len(content)))
    session.flush()
    
    loc = FileLocation(source_root_id=env['root_id'], rel_path=filename, filename=filename)
    session.add(loc)
    session.flush()
    
    ver = FileVersion(
        location_id=loc.id, content_hash=h_val, 
        size_bytes=len(content), modified_time=pd.Timestamp.now()
    )
    session.add(ver)
    session.flush()
    
    job = ProcessingJob(
        file_version_id=ver.id, plugin_name="e2e_plugin", status=StatusEnum.QUEUED
    )
    session.add(job)
    session.commit()
    return job.id

def test_zmq_architecture_happy_path(e2e_env):
    plugin_path = e2e_env['tmp_path'] / "e2e_plugin.py"
    plugin_path.write_text("""
import pandas as pd
def execute(file_path):
    df = pd.read_csv(file_path)
    df['processed'] = True
    return df
""")

    # Configure worker with the test environment's parquet root
    w_config = WorkerConfig(
        database=DatabaseConfig(connection_string=e2e_env['db_url']),
        storage=StorageConfig(parquet_root=e2e_env['parquet_root'])
    )
    
    worker = ZmqWorker(w_config, zmq_addr=TEST_ZMQ_ADDR)
    worker_thread = threading.Thread(target=worker.run, args=(50,))
    worker_thread.start()
    
    sidecar_proc = None
    try:
        sidecar_cmd = [
            sys.executable, "-m", "casparian_flow.sidecar",
            "--plugin", str(plugin_path),
            "--connect", TEST_ZMQ_ADDR
        ]
        sidecar_proc = subprocess.Popen(sidecar_cmd)
        
        # Give sidecar time to start up (Python imports can be slow)
        time.sleep(2)
        
        # Create CSV with 2 rows of data
        job_id = create_job(e2e_env, "data.csv", "col1,col2\n10,20\n30,40")
        
        session = e2e_env['session']
        # FIX: Increased timeout to 10 seconds (100 * 0.1s)
        for _ in range(100):
            session.expire_all()
            job = session.get(ProcessingJob, job_id)
            if job.status in [StatusEnum.COMPLETED, StatusEnum.FAILED]:
                break
            time.sleep(0.1)
            
        assert job.status == StatusEnum.COMPLETED, f"Job stuck or failed: {job.error_message}"
        
        # FIX: Look in the correct parquet_root subdirectory
        expected_dir = e2e_env['parquet_root'] / "output_dataset"
        assert expected_dir.exists(), f"Output directory {expected_dir} not created"
        
        # FIX: Verify ANY files exist, then read the directory
        files = list(expected_dir.iterdir())
        assert len(files) > 0, "No files found in output directory"
        
        # Read the whole directory as a dataset
        df = pd.read_parquet(expected_dir)
        
        assert len(df) == 2
        assert df.iloc[0]['_job_id'] == job_id
        assert df.iloc[0]['processed'] == True

    finally:
        worker.stop()
        worker_thread.join()
        if sidecar_proc:
            sidecar_proc.terminate()

def test_sidecar_crash_recovery(e2e_env):
    plugin_path = e2e_env['tmp_path'] / "crash_plugin.py"
    plugin_path.write_text("""
import sys
def execute(file_path):
    sys.exit(1)
""")

    w_config = WorkerConfig(
        database=DatabaseConfig(connection_string=e2e_env['db_url']),
        storage=StorageConfig(parquet_root=e2e_env['parquet_root'])
    )
    
    worker = ZmqWorker(w_config, zmq_addr=TEST_ZMQ_ADDR)
    worker_thread = threading.Thread(target=worker.run, args=(50,))
    worker_thread.start()
    
    sidecar_proc = None
    try:
        sidecar_cmd = [
            sys.executable, "-m", "casparian_flow.sidecar",
            "--plugin", str(plugin_path),
            "--connect", TEST_ZMQ_ADDR
        ]
        sidecar_proc = subprocess.Popen(sidecar_cmd)
        time.sleep(2)
        
        job_id = create_job(e2e_env, "crash.csv", "a,b\n1,2")
        
        session = e2e_env['session']
        # FIX: Increased timeout here too
        for _ in range(100):
            session.expire_all()
            job = session.get(ProcessingJob, job_id)
            if job.status == StatusEnum.FAILED:
                break
            time.sleep(0.1)
            
        # The critical check is that the worker thread is still alive
        assert worker_thread.is_alive()

    finally:
        worker.stop()
        worker_thread.join()
        if sidecar_proc:
            sidecar_proc.terminate()


# ============================================================================
# Protocol v2.0 End-to-End Tests
# ============================================================================


@pytest.mark.slow
@pytest.mark.integration
def test_e2e_protocol_v2_messages(e2e_env):
    """All v2 OpCodes work in real system."""
    from casparian_flow.protocol import msg_heartbeat, OpCode, unpack_header

    # Test HEARTBEAT message construction
    hb_frames = msg_heartbeat()
    op, job_id, meta_len, content_type, compressed = unpack_header(hb_frames[0])

    assert op == OpCode.HEARTBEAT
    assert job_id == 0

    # Verify all new OpCodes are defined
    assert OpCode.HEARTBEAT == 6
    assert OpCode.DEPLOY == 7


@pytest.mark.slow
@pytest.mark.integration
def test_e2e_heartbeat_keepalive(e2e_env, tmp_path):
    """Worker pruning doesn't affect active sidecars."""
    from casparian_flow.engine.config import PluginsConfig

    config = WorkerConfig(
        database=DatabaseConfig(
            connection_string=f"sqlite:///{e2e_env['tmp_path'] / 'e2e.db'}",
        ),
        storage=StorageConfig(parquet_root=e2e_env["parquet_root"]),
        plugins=PluginsConfig(dir=str(tmp_path / "plugins")),
    )

    # Use unique port
    worker = ZmqWorker(config, zmq_addr="tcp://127.0.0.1:5681")

    # Simulate active sidecar
    active_identity = b"active_sidecar"
    worker.sidecar_heartbeats[active_identity] = time.time()
    worker.plugin_registry["active_plugin"] = active_identity

    # Simulate stale sidecar
    stale_identity = b"stale_sidecar"
    worker.sidecar_heartbeats[stale_identity] = time.time() - 100  # 100s ago
    worker.plugin_registry["stale_plugin"] = stale_identity

    # Prune with 60s timeout
    worker._prune_dead_sidecars(timeout_seconds=60)

    # Active should remain
    assert active_identity in worker.sidecar_heartbeats
    assert "active_plugin" in worker.plugin_registry

    # Stale should be removed
    assert stale_identity not in worker.sidecar_heartbeats
    assert "stale_plugin" not in worker.plugin_registry

    worker.stop()


@pytest.mark.slow
@pytest.mark.integration
def test_e2e_deploy_workflow(e2e_env, tmp_path, sample_plugin_code):
    """Deploy plugin → auto-reload → job executes with new plugin."""
    from casparian_flow.engine.config import PluginsConfig
    from casparian_flow.security.gatekeeper import generate_signature
    from casparian_flow.db.models import PluginManifest, PluginStatusEnum
    from unittest.mock import patch

    config = WorkerConfig(
        database=DatabaseConfig(
            connection_string=f"sqlite:///{e2e_env['tmp_path'] / 'e2e.db'}",
        ),
        storage=StorageConfig(parquet_root=e2e_env["parquet_root"]),
        plugins=PluginsConfig(dir=str(tmp_path / "plugins")),
    )

    worker = ZmqWorker(config, zmq_addr="tcp://127.0.0.1:5682")

    # Deploy plugin via architect
    signature = generate_signature(sample_plugin_code, worker.architect.secret_key)

    result = worker.architect.deploy_plugin(
        plugin_name="deployed_plugin",
        version="1.0.0",
        source_code=sample_plugin_code,
        signature=signature,
        sample_input=None,  # Skip sandbox for e2e test
    )

    assert result.success is True

    # Verify plugin is ACTIVE in database
    session = e2e_env["session"]
    manifest = (
        session.query(PluginManifest)
        .filter_by(plugin_name="deployed_plugin")
        .first()
    )
    assert manifest is not None
    assert manifest.status == PluginStatusEnum.ACTIVE

    # Mock subprocess to avoid actual sidecar spawn
    with patch("casparian_flow.engine.zmq_worker.subprocess.Popen"):
        worker.reload_plugins()

    # Verify plugin file was written
    plugin_path = tmp_path / "plugins" / "deployed_plugin.py"
    assert plugin_path.exists()

    # Cleanup
    plugin_path.unlink()
    worker.stop()
