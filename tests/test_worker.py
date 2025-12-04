"""
Unit tests for CasparianWorker functionality.
"""
import pytest
from pathlib import Path
from casparian_flow.engine.worker import CasparianWorker
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
from casparian_flow.db.models import (
    ProcessingJob, FileVersion, FileLocation, PluginConfig, 
    TopicConfig, SourceRoot, StatusEnum
)
from datetime import datetime


class TestWorker:
    """Test worker functionality."""
    
    def test_worker_initialization(self, test_db_engine):
        """Test that worker initializes correctly."""
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        
        worker = CasparianWorker(config)
        
        assert worker.engine is not None
        assert worker.queue is not None
        assert worker.plugins is not None
        assert worker.active is True
    
    def test_worker_resolves_file_path_correctly(self, test_db_engine, test_db_session, test_source_root, temp_test_dir):
        """Test that worker can resolve file paths from FileVersion."""
        # Create file location and version
        test_file = temp_test_dir / "data.csv"
        test_file.write_text("col1,col2\n1,2")
        
        location = FileLocation(
            source_root_id=test_source_root,
            rel_path="data.csv",
            filename="data.csv"
        )
        test_db_session.add(location)
        test_db_session.flush()
        
        version = FileVersion(
            location_id=location.id,
            content_hash="abc123",
            size_bytes=test_file.stat().st_size,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.commit()
        
        # Create worker and resolve path
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)
        
        resolved_path = worker._resolve_file_path(version.id)
        
        assert resolved_path == test_file
        assert resolved_path.exists()
    
    def test_worker_handles_missing_file_version(self, test_db_engine):
        """Test that worker raises error for missing FileVersion."""
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)
        
        with pytest.raises(ValueError, match="FileVersion .* not found"):
            worker._resolve_file_path(99999)  # Non-existent ID
    
    def test_worker_loads_plugins_on_init(self, test_db_engine):
        """Test that worker discovers plugins during initialization."""
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)
        
        # Should have discovered test_plugin
        plugin = worker.plugins.get_plugin("test_plugin")
        assert plugin is not None
    
    def test_worker_processes_job_end_to_end(self, test_db_engine, test_db_session, test_source_root, test_plugin_config, temp_test_dir):
        """Test complete job processing workflow."""
        # Create test file
        test_file = temp_test_dir / "test.csv"
        test_file.write_text("a,b\n1,2\n3,4")
        
        # Create topic config
        topic = TopicConfig(
            plugin_name="test_plugin",
            topic_name="test",
            uri="parquet://./test_worker_output",
            mode="append"
        )
        test_db_session.add(topic)
        test_db_session.commit()
        
        # Create file version
        location = FileLocation(
            source_root_id=test_source_root,
            rel_path="test.csv",
            filename="test.csv"
        )
        test_db_session.add(location)
        test_db_session.flush()
        
        version = FileVersion(
            location_id=location.id,
            content_hash="abc123",
            size_bytes=test_file.stat().st_size,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        # Create job
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        # Create and run worker
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)
        
        # Pop and execute job
        popped_job = worker.queue.pop_job("test_worker")
        assert popped_job is not None
        
        worker._execute_job(popped_job)
        worker.queue.complete_job(popped_job.id, summary="Success")
        
        # Verify job completed
        test_db_session.refresh(job)
        assert job.status == StatusEnum.COMPLETED
    
    def test_worker_handles_job_execution_errors(self, test_db_engine, test_db_session, test_source_root, test_plugin_config):
        """Test that worker handles errors during job execution."""
        # Create file version for non-existent file
        location = FileLocation(
            source_root_id=test_source_root,
            rel_path="nonexistent.csv",
            filename="nonexistent.csv"
        )
        test_db_session.add(location)
        test_db_session.flush()
        
        version = FileVersion(
            location_id=location.id,
            content_hash="abc123",
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        # Create topic config
        topic = TopicConfig(
            plugin_name="test_plugin",
            topic_name="test",
            uri="parquet://./output",
            mode="append"
        )
        test_db_session.add(topic)
        test_db_session.commit()
        
        # Create job
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        # Create worker
        config = WorkerConfig(
            database=DatabaseConfig(connection_string=str(test_db_engine.url))
        )
        worker = CasparianWorker(config)
        
        # Pop job
        popped_job = worker.queue.pop_job("test_worker")
        
        # Execute should raise error (file doesn't exist)
        with pytest.raises(Exception):
            worker._execute_job(popped_job)
