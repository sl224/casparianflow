"""
Smoke tests for Casparian Flow end-to-end pipeline.
Tests file versioning, tagging, and routing logic.
"""
import pytest
import time
from pathlib import Path
import pandas as pd
from sqlalchemy import create_engine, text
from casparian_flow.db.models import (
    FileLocation, FileVersion, ProcessingJob, TopicConfig, SourceRoot, RoutingRule, PluginConfig
)
from casparian_flow.services.scout import Scout


@pytest.mark.smoke
@pytest.mark.parametrize("sink_type,sink_uri", [
    ("parquet", "parquet://./test_output"),
    ("sqlite", "sqlite://test_output.db/test_table"),
])
def test_end_to_end_pipeline(
    sink_type,
    sink_uri,
    temp_test_dir,
    test_db_engine,
    test_db_session,
    test_source_root,
    test_plugin_config
):
    """
    Test complete pipeline: Scout -> Queue -> Worker -> Sink
    Validates:
    1. RoutingRules tag the file.
    2. Plugin subscribes to that tag.
    3. Job is queued only because of the tag match.
    4. Versioning works.
    """
    # 1. Create test file
    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    # 2. Configure Tagging & Routing
    # A) Define a Rule: *.csv -> 'raw_csv'
    rule = RoutingRule(pattern="*.csv", tag="raw_csv", priority=10)
    test_db_session.add(rule)
    
    # B) Update Plugin to subscribe to 'raw_csv'
    # (test_plugin_config fixture creates the plugin, we just update it)
    test_plugin_config.subscription_tags = "raw_csv, other_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()
    
    # 3. Configure topic
    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri=sink_uri,
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    # 4. Run Scout (first scan)
    scout = Scout(test_db_session)
    root = test_db_session.query(SourceRoot).get(test_source_root)
    scout.scan_source(root)
    
    # 5. Verify initial state
    locations = test_db_session.query(FileLocation).all()
    versions = test_db_session.query(FileVersion).all()
    jobs = test_db_session.query(ProcessingJob).all()
    
    assert len(locations) == 1, f"Expected 1 location, got {len(locations)}"
    assert len(versions) == 1, f"Expected 1 version, got {len(versions)}"
    assert len(jobs) == 1, f"Expected 1 job, got {len(jobs)}"
    
    version1 = versions[0]
    job1 = jobs[0]
    
    # Verify Tags
    assert "raw_csv" in version1.applied_tags, f"Expected 'raw_csv' tag, got {version1.applied_tags}"
    
    assert job1.file_version_id == version1.id
    assert job1.plugin_name == "test_plugin"
    
    # 6. Modify file (test versioning)
    time.sleep(0.1)  # Ensure different timestamp
    test_file.write_text("col1,col2\n5,6\n7,8\n9,10")
    
    # 7. Run Scout again
    scout.scan_source(root)
    
    # 8. Verify version tracking
    locations = test_db_session.query(FileLocation).all()
    versions = test_db_session.query(FileVersion).order_by(FileVersion.detected_at).all()
    jobs = test_db_session.query(ProcessingJob).order_by(ProcessingJob.id).all()
    
    assert len(locations) == 1, "Should still be 1 location (same file)"
    assert len(versions) == 2, f"Expected 2 versions after modification, got {len(versions)}"
    assert len(jobs) == 2, f"Expected 2 jobs, got {len(jobs)}"
    
    # Verify different content hashes
    assert versions[0].content_hash != versions[1].content_hash
    
    # Verify correct job-version linkage
    assert jobs[0].file_version_id == versions[0].id
    assert jobs[1].file_version_id == versions[1].id


@pytest.mark.smoke
def test_no_routing_match(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that jobs are NOT queued if tags don't match."""
    # 1. Create a file
    test_file = temp_test_dir / "skip_me.txt"
    test_file.write_text("ignore")
    
    # 2. Rule: *.txt -> 'text_file'
    rule = RoutingRule(pattern="*.txt", tag="text_file")
    test_db_session.add(rule)
    
    # 3. Plugin subscribes to 'raw_csv' ONLY
    test_plugin_config.subscription_tags = "raw_csv"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()
    
    # 4. Scout
    scout = Scout(test_db_session)
    root = test_db_session.query(SourceRoot).get(test_source_root)
    scout.scan_source(root)
    
    # 5. Verify
    jobs = test_db_session.query(ProcessingJob).all()
    versions = test_db_session.query(FileVersion).all()
    
    assert len(versions) == 1
    assert "text_file" in versions[0].applied_tags
    assert len(jobs) == 0, "Should not queue job because plugin does not subscribe to 'text_file'"


@pytest.mark.smoke
def test_parquet_output_verification(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that parquet output is created and readable."""
    from casparian_flow.engine.worker import CasparianWorker
    from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
    
    # Setup Routing
    rule = RoutingRule(pattern="*.csv", tag="test_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "test_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

    # Create test file
    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    # Configure parquet topic
    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri="parquet://./test_parquet_output",
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    # Run Scout
    scout = Scout(test_db_session)
    root = test_db_session.query(SourceRoot).get(test_source_root)
    scout.scan_source(root)
    
    # Run Worker with typed config
    worker_config = WorkerConfig(
        database=DatabaseConfig(connection_string=str(test_db_engine.url))
    )
    
    worker = CasparianWorker(worker_config)
    
    # Process one job
    job = worker.queue.pop_job('test_signature')
    assert job is not None, "No job found in queue"
    
    worker._execute_job(job)
    worker.queue.complete_job(job.id, summary="Test completed")
    
    # Verify parquet file exists
    output_path = Path("data/parquet/test_parquet_output")
    assert output_path.exists(), f"Parquet output not found at {output_path}"
    
    # Verify can read parquet
    df = pd.read_parquet(output_path)
    assert len(df) > 0, "Parquet file is empty"
    assert df.shape[1] == 1, f"Expected 1 column, got {df.shape[1]}"


@pytest.mark.smoke
def test_sqlite_output_verification(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that SQLite output is created and queryable."""
    from casparian_flow.engine.worker import CasparianWorker
    
    # Setup Routing
    rule = RoutingRule(pattern="*.csv", tag="test_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "test_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

    # Create test file
    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    # Configure SQLite topic
    sqlite_db = "test_sink_output.db"
    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri=f"sqlite://{sqlite_db}/test_results",
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    # Run Scout
    scout = Scout(test_db_session)
    root = test_db_session.query(SourceRoot).get(test_source_root)
    scout.scan_source(root)
    
    # Run Worker with typed config
    from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
    
    worker_config = WorkerConfig(
        database=DatabaseConfig(connection_string=str(test_db_engine.url))
    )
    
    worker = CasparianWorker(worker_config)
    
    # Process one job
    job = worker.queue.pop_job('test_signature')
    assert job is not None, "No job found in queue"
    
    worker._execute_job(job)
    worker.queue.complete_job(job.id, summary="Test completed")
    
    # Verify SQLite database exists and has data
    sqlite_path = Path(sqlite_db)
    assert sqlite_path.exists(), f"SQLite database not found at {sqlite_path}"
    
    # Query the table
    verify_engine = create_engine(f"sqlite:///{sqlite_db}")
    with verify_engine.connect() as conn:
        result = conn.execute(text("SELECT COUNT(*) FROM test_results"))
        count = result.scalar()
        assert count > 0, "SQLite table is empty"
        
        # Get all rows
        result = conn.execute(text("SELECT * FROM test_results"))
        rows = result.fetchall()
        assert len(rows) > 0, "No data in SQLite table"
    
    # Cleanup
    sqlite_path.unlink()


@pytest.mark.smoke
def test_version_lineage_query(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that we can query version lineage correctly."""
    # Setup Routing
    rule = RoutingRule(pattern="*.csv", tag="lineage_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "lineage_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

    # Create and modify file
    test_file = temp_test_dir / "lineage_test.csv"
    test_file.write_text("a,b\n1,2")
    
    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri="parquet://./lineage_output",
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    scout = Scout(test_db_session)
    root = test_db_session.query(SourceRoot).get(test_source_root)
    
    # First version
    scout.scan_source(root)
    version1_hash = test_db_session.query(FileVersion).first().content_hash
    
    # Second version
    time.sleep(0.1)
    test_file.write_text("a,b\n3,4\n5,6")
    scout.scan_source(root)
    
    # Query lineage
    jobs = test_db_session.query(ProcessingJob).order_by(ProcessingJob.id).all()
    
    for i, job in enumerate(jobs):
        version = test_db_session.query(FileVersion).get(job.file_version_id)
        location = test_db_session.query(FileLocation).get(version.location_id)
        
        assert version is not None, f"Version not found for job {job.id}"
        assert location is not None, f"Location not found for version {version.id}"
        assert location.rel_path == "lineage_test.csv"
        assert "lineage_tag" in version.applied_tags
        
        # Verify job 0 has original hash
        if i == 0:
            assert version.content_hash == version1_hash
        else:
            assert version.content_hash != version1_hash