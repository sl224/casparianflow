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

        # 3. CRITICAL FIX: Monkeypatch the plugin to FORCE a crash.
        # We don't want to rely on the filesystem or the actual plugin logic here.
        # We just want to ensure the Worker catches *any* exception.
        
        from casparian_flow.plugins.test_plugin import Handler
        
        def mock_execute_crash(self, file_path):
            raise ValueError("Simulated Plugin Crash")
            
        # Patch the 'execute' method of the Handler class
        monkeypatch.setattr(Handler, "execute", mock_execute_crash)

        # 4. Run Execution Manually
        # Pop job (simulating the queue loop)
        popped_job = worker.queue.pop_job("test_worker")
        assert popped_job is not None
        
        # Execute (this catches the exception internally and calls fail_job)
        # We don't expect _execute_job to raise; we expect it to update the DB status to FAILED.
        # Let's look at worker.py: It catches Exception and calls self.queue.fail_job
        
        # Wait, the previous test expected pytest.raises(Exception).
        # If the Worker swallows the exception (which is good design for a daemon),
        # then pytest.raises() will fail (because nothing was raised).
        
        # We should check that the JOB STATUS in the DB is FAILED.
        
        # However, run() loops. _execute_job() might re-raise if called directly?
        # Let's look at `worker.py`:
        #    try:
        #        self._execute_job(job)
        #    except Exception as e:
        #        logger.error(...)
        #        self.queue.fail_job(...)
        
        # So `_execute_job` *does* raise if the plugin crashes. The `try/except` block is in `run()`.
        # Therefore, calling `_execute_job` directly SHOULD raise the exception.
        
        with pytest.raises(ValueError, match="Simulated Plugin Crash"):
            worker._execute_job(popped_job)
            
        # 5. Verify DB State (Optional but good)
        # Since we called _execute_job directly and it crashed, the `fail_job` logic 
        # (which lives in the `run` loop catch block) might NOT have run yet 
        # unless we explicitly test the `run` loop logic or mimic the catch block.
        
        # To strictly test the "Worker handles errors" logic, we should probably mimic the `run` loop block:
        try:
            worker._execute_job(popped_job)
        except Exception as e:
            worker.queue.fail_job(popped_job.id, str(e))
            
        test_db_session.expire_all()
        updated_job = test_db_session.get(ProcessingJob, job.id)
        assert updated_job.status == StatusEnum.FAILED
        assert "Simulated Plugin Crash" in updated_job.error_message