import pytest
import json
import time
from pathlib import Path
from datetime import datetime
from sqlalchemy import create_engine
from casparian_flow.engine.worker import CasparianWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
from casparian_flow.db.models import (
    ProcessingJob, FileVersion, FileLocation, PluginConfig, TopicConfig, 
    StatusEnum, SourceRoot, FileHashRegistry
)

class TestWorker:
    def test_worker_handles_job_execution_errors(self, test_db_engine, test_db_session, test_source_root, test_plugin_config, monkeypatch):
        """Test that worker handles errors during job execution."""
        # 1. Setup Data
        # Create file location
        location = FileLocation(
            source_root_id=test_source_root,
            rel_path="nonexistent.csv",
            filename="nonexistent.csv"
        )
        test_db_session.add(location)
        test_db_session.flush()

        # Create Hash (required by FK)
        h_reg = FileHashRegistry(content_hash="abc12345", size_bytes=100)
        test_db_session.add(h_reg)
        test_db_session.flush()

        # Create Version
        version = FileVersion(
            location_id=location.id,
            content_hash="abc12345",
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()

        # Register Topic
        topic = TopicConfig(
            plugin_name="test_plugin",
            topic_name="test",
            uri="parquet://./output",
            mode="append"
        )
        test_db_session.add(topic)
        
        # Create Job
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()

        # 2. Setup Worker
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)

        # 3. CRITICAL FIX: Inject the mock plugin directly into the registry cache.
        # The Worker's PluginRegistry loads plugins dynamically from files, creating separate 
        # class objects than a static import. Monkeypatching the import doesn't work.
        # By injecting into _cache, we force the worker to use our mock class.
        
        class CrashingPlugin:
            def configure(self, ctx, config):
                pass
            def execute(self, file_path):
                raise ValueError("Simulated Plugin Crash")

        # Directly overwrite the plugin in the worker's registry cache
        worker.plugins._cache["test_plugin"] = CrashingPlugin

        # 4. Run Execution Manually
        # Pop job (simulating the queue loop)
        popped_job = worker.queue.pop_job("test_worker")
        assert popped_job is not None
        
        # Execute (should raise the ValueError from our mock)
        with pytest.raises(ValueError, match="Simulated Plugin Crash"):
            worker._execute_job(popped_job)
            
        # 5. Verify DB State 
        # Simulate the Worker's run loop exception handling to verify it marks the job FAILED
        try:
            worker._execute_job(popped_job)
        except Exception as e:
            worker.queue.fail_job(popped_job.id, str(e))
            
        test_db_session.expire_all()
        updated_job = test_db_session.get(ProcessingJob, job.id)
        assert updated_job.status == StatusEnum.FAILED
        assert "Simulated Plugin Crash" in updated_job.error_message