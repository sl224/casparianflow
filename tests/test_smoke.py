"""
Smoke tests for Casparian Flow end-to-end pipeline.
Tests file versioning, tagging, and routing logic.
"""
import pytest
import time
import shutil
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
    """
    # 1. Create test file
    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    # 2. Configure Tagging & Routing
    rule = RoutingRule(pattern="*.csv", tag="raw_csv", priority=10)
    test_db_session.add(rule)
    
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
    
    # 4. Run Scout
    scout = Scout(test_db_session)
    root = test_db_session.get(SourceRoot, test_source_root)
    scout.scan_source(root)
    
    # 5. Verify initial state
    locations = test_db_session.query(FileLocation).all()
    versions = test_db_session.query(FileVersion).all()
    jobs = test_db_session.query(ProcessingJob).all()
    
    assert len(locations) == 1
    assert len(versions) == 1
    assert len(jobs) == 1
    
    version1 = versions[0]
    job1 = jobs[0]
    
    assert "raw_csv" in version1.applied_tags
    assert job1.file_version_id == version1.id
    
    # 6. Modify file
    time.sleep(0.1)
    test_file.write_text("col1,col2\n5,6\n7,8\n9,10")
    
    # 7. Run Scout again
    scout.scan_source(root)
    
    # 8. Verify versioning
    versions = test_db_session.query(FileVersion).order_by(FileVersion.detected_at).all()
    jobs = test_db_session.query(ProcessingJob).order_by(ProcessingJob.id).all()
    
    assert len(versions) == 2
    assert len(jobs) == 2
    assert versions[0].content_hash != versions[1].content_hash
    assert jobs[1].file_version_id == versions[1].id


@pytest.mark.smoke
def test_no_routing_match(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that jobs are NOT queued if tags don't match."""
    test_file = temp_test_dir / "skip_me.txt"
    test_file.write_text("ignore")
    
    rule = RoutingRule(pattern="*.txt", tag="text_file")
    test_db_session.add(rule)
    
    test_plugin_config.subscription_tags = "raw_csv"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()
    
    scout = Scout(test_db_session)
    root = test_db_session.get(SourceRoot, test_source_root)
    scout.scan_source(root)
    
    jobs = test_db_session.query(ProcessingJob).all()
    versions = test_db_session.query(FileVersion).all()
    
    assert len(versions) == 1
    assert "text_file" in versions[0].applied_tags
    assert len(jobs) == 0


@pytest.mark.smoke
def test_parquet_output_verification(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that parquet output is created and readable."""
    from casparian_flow.engine.worker import CasparianWorker
    from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
    
    # Cleanup previous run
    if Path("data/parquet/test_parquet_output").exists():
        shutil.rmtree("data/parquet/test_parquet_output")

    rule = RoutingRule(pattern="*.csv", tag="test_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "test_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri="parquet://./test_parquet_output",
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    scout = Scout(test_db_session)
    root = test_db_session.get(SourceRoot, test_source_root)
    scout.scan_source(root)
    
    # Run Worker with CORRECT connection
    worker_config = WorkerConfig(
        database=DatabaseConfig(connection_string=str(test_db_engine.url))
    )
    
    worker = CasparianWorker(worker_config)
    
    job = worker.queue.pop_job('test_signature')
    assert job is not None, "No job found in queue"
    
    worker._execute_job(job)
    worker.queue.complete_job(job.id, summary="Test completed")
    
    output_path = Path("data/parquet/test_parquet_output")
    assert output_path.exists()
    
    # ParquetSink now creates a directory. Read contents.
    df = pd.read_parquet(output_path)
    assert len(df) > 0
    # Check for lineage injection
    assert "_job_id" in df.columns
    assert "_file_version_id" in df.columns


@pytest.mark.smoke
def test_sqlite_output_verification(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that SQLite output is created and queryable."""
    from casparian_flow.engine.worker import CasparianWorker
    from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
    
    rule = RoutingRule(pattern="*.csv", tag="test_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "test_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

    test_file = temp_test_dir / "data.csv"
    test_file.write_text("col1,col2\n1,2\n3,4")
    
    sqlite_db = "test_sink_output.db"
    if Path(sqlite_db).exists():
        Path(sqlite_db).unlink()

    topic = TopicConfig(
        plugin_name="test_plugin",
        topic_name="test",
        uri=f"sqlite://{sqlite_db}/test_results",
        mode="append"
    )
    test_db_session.add(topic)
    test_db_session.commit()
    
    scout = Scout(test_db_session)
    root = test_db_session.get(SourceRoot, test_source_root)
    scout.scan_source(root)
    
    worker_config = WorkerConfig(
        database=DatabaseConfig(connection_string=str(test_db_engine.url))
    )
    
    worker = CasparianWorker(worker_config)
    
    job = worker.queue.pop_job('test_signature')
    assert job is not None, "No job found in queue"
    
    worker._execute_job(job)
    worker.queue.complete_job(job.id, summary="Test completed")
    
    sqlite_path = Path(sqlite_db)
    assert sqlite_path.exists()
    
    verify_engine = create_engine(f"sqlite:///{sqlite_db}")
    with verify_engine.connect() as conn:
        result = conn.execute(text("SELECT COUNT(*) FROM test_results"))
        count = result.scalar()
        assert count > 0
        
        # Verify Lineage injection
        cols = conn.execute(text("PRAGMA table_info(test_results)")).fetchall()
        col_names = [c[1] for c in cols]
        assert "_job_id" in col_names
        assert "_file_version_id" in col_names
    
    verify_engine.dispose()
    if sqlite_path.exists():
        sqlite_path.unlink()


@pytest.mark.smoke
def test_version_lineage_query(temp_test_dir, test_db_engine, test_db_session, test_source_root, test_plugin_config):
    """Test that we can query version lineage correctly."""
    rule = RoutingRule(pattern="*.csv", tag="lineage_tag")
    test_db_session.add(rule)
    test_plugin_config.subscription_tags = "lineage_tag"
    test_db_session.add(test_plugin_config)
    test_db_session.commit()

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
    root = test_db_session.get(SourceRoot, test_source_root)
    
    scout.scan_source(root)
    version1_hash = test_db_session.query(FileVersion).first().content_hash
    
    time.sleep(0.1)
    test_file.write_text("a,b\n3,4\n5,6")
    scout.scan_source(root)
    
    jobs = test_db_session.query(ProcessingJob).order_by(ProcessingJob.id).all()
    
    for i, job in enumerate(jobs):
        version = test_db_session.get(FileVersion, job.file_version_id)
        location = test_db_session.get(FileLocation, version.location_id)
        
        assert version is not None
        assert location.rel_path == "lineage_test.csv"
        assert "lineage_tag" in version.applied_tags
        
        if i == 0:
            assert version.content_hash == version1_hash
        else:
            assert version.content_hash != version1_hash