"""
Unit tests for JobQueue operations.
"""
import pytest
import time
from datetime import datetime
from casparian_flow.engine.queue import JobQueue
from casparian_flow.db.models import ProcessingJob, FileVersion, FileLocation, PluginConfig, TopicConfig, SourceRoot, StatusEnum


class TestJobQueue:
    """Test JobQueue functionality."""
    
    def test_pop_job_fifo_by_priority(self, test_db_engine, test_db_session, test_source_root, test_plugin_config):
        """Test that jobs are popped in priority order (highest first)."""
        # Create test location and versions
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
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        # Create jobs with different priorities
        job_low = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED,
            priority=1
        )
        job_high = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED,
            priority=10
        )
        job_medium = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED,
            priority=5
        )
        
        test_db_session.add_all([job_low, job_high, job_medium])
        test_db_session.commit()
        
        # Pop jobs and verify priority order
        queue = JobQueue(test_db_engine)
        
        job1 = queue.pop_job("test_worker")
        assert job1.priority == 10, "Should pop highest priority first"
        
        job2 = queue.pop_job("test_worker")
        assert job2.priority == 5, "Should pop medium priority second"
        
        job3 = queue.pop_job("test_worker")
        assert job3.priority == 1, "Should pop lowest priority last"
    
    def test_pop_job_returns_none_when_empty(self, test_db_engine):
        """Test that pop_job returns None when queue is empty."""
        queue = JobQueue(test_db_engine)
        job = queue.pop_job("test_worker")
        assert job is None, "Should return None when no jobs available"
    
    def test_complete_job(self, test_db_engine, test_db_session, test_source_root, test_plugin_config):
        """Test marking a job as completed."""
        # Create job
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
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        # Complete job
        queue = JobQueue(test_db_engine)
        queue.complete_job(job.id, summary="Test completed successfully")
        
        # Verify
        test_db_session.refresh(job)
        assert job.status == StatusEnum.COMPLETED
        assert job.result_summary == "Test completed successfully"
        assert job.end_time is not None
    
    def test_fail_job(self, test_db_engine, test_db_session, test_source_root, test_plugin_config):
        """Test marking a job as failed."""
        # Create job
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
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        # Fail job
        queue = JobQueue(test_db_engine)
        error_message = "Test error: file not found"
        queue.fail_job(job.id, error=error_message)
        
        # Verify
        test_db_session.refresh(job)
        assert job.status == StatusEnum.FAILED
        assert job.error_message == error_message
        assert job.end_time is not None
        assert job.retry_count == 0
    
    def test_job_status_prevents_duplicate_pop(self, test_db_engine, test_db_session, test_source_root, test_plugin_config):
        """Test that claimed jobs are not popped again."""
        # Create job
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
            size_bytes=100,
            modified_time=datetime.now()
        )
        test_db_session.add(version)
        test_db_session.flush()
        
        job = ProcessingJob(
            file_version_id=version.id,
            plugin_name="test_plugin",
            status=StatusEnum.QUEUED
        )
        test_db_session.add(job)
        test_db_session.commit()
        
        queue = JobQueue(test_db_engine)
        
        # Pop once
        job1 = queue.pop_job("worker1")
        assert job1 is not None
        
        # Try to pop again - should return None
        job2 = queue.pop_job("worker2")
        assert job2 is None, "Already claimed job should not be popped again"
