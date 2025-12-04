"""
Unit tests for HeartbeatThread functionality.
"""
import pytest
import time
from datetime import datetime, timedelta
from casparian_flow.engine.heartbeat import HeartbeatThread
from casparian_flow.db.models import WorkerNode


class TestHeartbeat:
    """Test heartbeat functionality."""
    
    def test_heartbeat_creates_worker_record(self, test_db_engine, test_db_session):
        """Test that heartbeat creates a new worker record."""
        # Create heartbeat thread (but don't start it)
        db_url = str(test_db_engine.url)
        heartbeat = HeartbeatThread(db_url, interval=30)
        heartbeat.engine = test_db_engine  # Set engine manually
        
        # Send one heartbeat
        heartbeat._send_heartbeat()
        
        # Verify worker record was created
        workers = test_db_session.query(WorkerNode).all()
        assert len(workers) == 1
        
        worker = workers[0]
        assert worker.hostname == heartbeat.hostname
        assert worker.status == "ONLINE"
        assert worker.last_heartbeat is not None
    
    def test_heartbeat_updates_existing_record(self, test_db_engine, test_db_session):
        """Test that heartbeat updates existing worker record."""
        # Create initial worker record
        worker = WorkerNode(
            hostname="test-host",
            pid=12345,
            ip_address="192.168.1.1",
            env_signature="old_signature",
            status="OFFLINE"
        )
        test_db_session.add(worker)
        test_db_session.commit()
        
        initial_heartbeat = worker.last_heartbeat
        
        # Wait a bit to ensure timestamp difference
        time.sleep(0.1)
        
        # Create heartbeat with same hostname AND pid (composite key)
        db_url = str(test_db_engine.url)
        heartbeat = HeartbeatThread(db_url, interval=30)
        heartbeat.engine = test_db_engine
        heartbeat.hostname = "test-host"  # Override to match
        heartbeat.pid = 12345  # Must match composite key
        heartbeat.env_signature = "new_signature"
        
        # Send heartbeat
        heartbeat._send_heartbeat()
        
        # Verify record was updated, not duplicated
        workers = test_db_session.query(WorkerNode).all()
        assert len(workers) == 1, "Should update existing record, not create new one"
        
        # Query fresh copy from DB
        updated_worker = test_db_session.query(WorkerNode).filter_by(
            hostname="test-host", pid=12345
        ).first()
        
        assert updated_worker.status == "ONLINE"
        assert updated_worker.env_signature == "new_signature"
        assert updated_worker.last_heartbeat > initial_heartbeat
    
    def test_heartbeat_handles_database_errors_gracefully(self, test_db_engine, test_db_session):
        """Test that heartbeat handles database errors without crashing."""
        # Create heartbeat with invalid engine
        heartbeat = HeartbeatThread("sqlite:///nonexistent.db", interval=30)
        
        # This should not raise an exception
        try:
            heartbeat._send_heartbeat()
            # If we get here, the error was handled gracefully
            assert True
        except Exception as e:
            pytest.fail(f"Heartbeat should handle errors gracefully, but raised: {e}")
    
    def test_multiple_workers_tracked_separately(self, test_db_engine, test_db_session):
        """Test that multiple workers are tracked separately."""
        db_url = str(test_db_engine.url)
        
        # Create two heartbeat instances simulating different workers
        heartbeat1 = HeartbeatThread(db_url, interval=30)
        heartbeat1.engine = test_db_engine
        heartbeat1.hostname = "worker1"
        heartbeat1.pid = 1001
        
        heartbeat2 = HeartbeatThread(db_url, interval=30)
        heartbeat2.engine = test_db_engine
        heartbeat2.hostname = "worker2"
        heartbeat2.pid = 1002
        
        # Send heartbeats
        heartbeat1._send_heartbeat()
        heartbeat2._send_heartbeat()
        
        # Verify both workers are tracked
        workers = test_db_session.query(WorkerNode).order_by(WorkerNode.hostname).all()
        assert len(workers) == 2
        assert workers[0].hostname == "worker1"
        assert workers[1].hostname == "worker2"
        assert workers[0].pid == 1001
        assert workers[1].pid == 1002
