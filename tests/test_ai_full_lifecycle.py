
# tests/test_ai_full_lifecycle.py
import pytest
import time
import json
import threading
from pathlib import Path
from unittest.mock import MagicMock, patch

from casparian_flow.engine.zmq_worker import ZmqWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig, StorageConfig, PluginsConfig
from casparian_flow.services.ai_types import FileProfile, SchemaProposal, PluginCode
from casparian_flow.services.llm_generator import LLMGenerator
from datetime import datetime
from casparian_flow.services.inspector import profile_file
from casparian_flow.db.models import PluginManifest, PluginStatusEnum, ProcessingJob, StatusEnum, FileVersion, FileLocation, FileHashRegistry
from casparian_flow.security.signing import Signer
import socket
import logging

logging.basicConfig(level=logging.INFO)


def get_free_port():
    """Get a free port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

@pytest.fixture
def unique_zmq_addr():
    """Provide a unique TCP address for each test."""
    port = get_free_port()
    return f"tcp://127.0.0.1:{port}"

GENERATED_PLUGIN_CODE = """
from casparian_flow.sdk import BasePlugin
import pandas as pd
import pyarrow as pa

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Simple CSV passthrough
        df = pd.read_csv(file_path)
        # Add a column to prove we ran
        df['processed_by'] = 'AI_PLUGIN'
        
        # Publish to output topic
        self.publish("output", df)
"""

class MockProvider:
    """Simulates an OpenAI/Anthropic provider returning perfect JSON."""
    def chat_completion(self, messages, model=None, json_mode=False):
        # We don't care about the prompt, we just return the code.
        # The Generator expects specific responses for specific steps.
        last_msg = messages[-1]["content"]
        
        if "Analyze this file profile" in last_msg:
            # Step 1: Proposal
            return json.dumps({
                "file_type_inferred": "CSV",
                "target_topic": "output",
                "read_strategy": "pd.read_csv",
                "columns": [{"name": "col1", "target_type": "int64"}],
                "reasoning": "It looks like a CSV"
            })
        elif "Generate the Python code" in last_msg:
            # Step 2: Code
            return f"```python\n{GENERATED_PLUGIN_CODE}\n```"
        return "{}"

@pytest.fixture
def setup_environment(tmp_path, test_db_engine):
    """Setup directories and config."""
    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()
    
    parquet_dir = tmp_path / "parquet"
    parquet_dir.mkdir()
    
    config = WorkerConfig(
        database=DatabaseConfig(connection_string=str(test_db_engine.url)),
        storage=StorageConfig(parquet_root=parquet_dir),
        plugins=PluginsConfig(directory=plugins_dir)
    )
    return config, plugins_dir, parquet_dir

def test_full_ai_lifecycle(setup_environment, test_db_session, unique_zmq_addr):
    """
    Test the flow: Inspect -> Generate -> Sign -> Deploy -> Run -> Verify.
    """
    config, plugins_dir, parquet_dir = setup_environment
    
    # 1. Create a Dummy Data File
    data_file = plugins_dir / "data.csv"
    data_file.write_text("col1,col2\n1,10\n2,20")
    
    # 2. Simulate AI Generation (CLI Phase)
    # We'll use the real LLMGenerator but with a MockProvider
    provider = MockProvider()
    generator = LLMGenerator(provider)
    
    # Run the Generation Pipeline manually (simulating inspect_and_generate.py)
    profile = profile_file(str(data_file))
    proposal = generator.propose_schema(profile)
    plugin_code = generator.generate_plugin(proposal)
    
    # Write to disk + Sign (Simulate User Approval)
    target_path = plugins_dir / "generated_processor.py"
    target_path.write_text(plugin_code.source_code, encoding="utf-8")
    
    sig_path = plugins_dir / "generated_processor.py.sig"
    sig_path.write_text(Signer.sign(plugin_code.source_code), encoding="utf-8")
    
    assert target_path.exists()
    assert sig_path.exists()
    
    # 3. Start ZmqWorker (Infrastructure Phase)
    # This will spin up the SystemDeployer
    worker = ZmqWorker(config, zmq_addr=unique_zmq_addr)
    
    # Run worker in a separate thread because it has a blocking loop
    # Actually, we can just run one tick or rely on polling? 
    # The worker.run() block. We need to run it in a thread.
    
    worker_thread = threading.Thread(target=worker.run, args=(100, 200), daemon=True)
    worker_thread.start()
    
    
    # 3b. Trigger Deployment (Simulate Scout)
    # The SystemDeployer is reactive. It needs a job to tell it to check the file.
    # In a real run, the Scout would see .py file and dispatch to system_deployer.
    
    # We need to register the system_deployer in the topic config/plugin config?
    # Actually, built-ins don't need DB config to run, but they need a Job.
    
    # Create SourceRoot for plugins_dir
    source_root_cls = FileLocation.source_root.property.mapper.class_
    sr = source_root_cls(path=str(plugins_dir))
    test_db_session.add(sr) 
    test_db_session.flush()

    # Register the "Source" of the plugin file
    plugin_src_loc = FileLocation(source_root_id=sr.id, rel_path="generated_processor.py", filename="generated_processor.py")
    test_db_session.add(plugin_src_loc)
    test_db_session.flush()
    
    # Add Hash to Registry first (FK constraint)
    h_reg_code = FileHashRegistry(content_hash="codehash", size_bytes=len(plugin_code.source_code))
    test_db_session.add(h_reg_code)
    test_db_session.flush()

    plugin_ver = FileVersion(
        location_id=plugin_src_loc.id, 
        content_hash="codehash", 
        size_bytes=len(plugin_code.source_code),
        modified_time=datetime.now()
    )
    test_db_session.add(plugin_ver)
    test_db_session.flush()
    
    deploy_job = ProcessingJob(
        file_version_id=plugin_ver.id,
        plugin_name="system_deployer",  # The built-in name
        status=StatusEnum.QUEUED
    )
    test_db_session.add(deploy_job)
    test_db_session.commit()
    print(f"Submitted Deployment Job {deploy_job.id}")
    
    try:
        # Wait for SystemDeployer to pick up the file, send DEPLOY, and Worker to reload.
        # This is async. We poll the DB.
        
        print(f"Waiting for deployment... Registry: {list(worker.plugin_registry.keys())}")
        max_retries = 20
        deployed = False
        for _ in range(max_retries):
            test_db_session.expire_all()
            manifest = test_db_session.query(PluginManifest).filter_by(plugin_name="generated_processor").first()
            if manifest:
                if manifest.status == PluginStatusEnum.ACTIVE:
                    deployed = True
                    break
                elif manifest.status in [PluginStatusEnum.REJECTED, PluginStatusEnum.FAILED]:
                     print(f"Deployment rejected: {manifest.validation_error}")
                     assert False, f"Deployment rejected: {manifest.validation_error}"

            # Check job status
            test_db_session.expire_all()
            j = test_db_session.get(ProcessingJob, deploy_job.id)
            if j.status == StatusEnum.FAILED:
                 print(f"Deployment Job Failed: {j.error_message}")
                 assert False, f"Deployment Job Failed: {j.error_message}"

            time.sleep(0.5)
            
        assert deployed, "Plugin failed to deploy within timeout"
        
        # 4. Verification: Submit a Job
        # We need to manually inject the job since we bypassed the Scout
        
        # Create Location/Version metadata for input file
        # sr already defined
        
        fl = FileLocation(source_root_id=sr.id, rel_path="data.csv", filename="data.csv")
        test_db_session.add(fl)
        test_db_session.flush()
        
        h_reg = FileHashRegistry(content_hash="abc", size_bytes=100)
        test_db_session.add(h_reg)
        
        fv = FileVersion(location_id=fl.id, content_hash="abc", size_bytes=100)
        test_db_session.add(fv)
        test_db_session.flush()
        
        # Create Job
        job = ProcessingJob(
            file_version_id=fv.id,
            plugin_name="generated_processor",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        print(f"Submitted Job {job.id}")
        
        # Wait for Job Completion
        for _ in range(max_retries):
            test_db_session.expire_all()
            j = test_db_session.get(ProcessingJob, job.id)
            if j.status in [StatusEnum.COMPLETED, StatusEnum.FAILED]:
                break
            time.sleep(0.5)
            
        assert j.status == StatusEnum.COMPLETED
        
        # Check if output exists (Parquet or SQLite? Defaults to ParquetSink?)
        # BasePlugin.publish goes to context.publish -> Sinks.
        # We configured parquet_root.
        pass
        
    finally:
        worker.stop()
        worker_thread.join(timeout=2)
