"""
Test Self-Healing Sinks: Destructive Initialization

Verifies that sinks clean up stale staging artifacts when initialized,
ensuring idempotent job retries.
"""
import pytest
from pathlib import Path
from sqlalchemy import create_engine, text, inspect
from sqlalchemy.orm import Session

from casparian_flow.engine.sinks import ParquetSink, SqliteSink


class TestDestructiveInit:
    """Test cases for Self-Healing Sink behavior."""

    def test_parquet_sink_cleans_stale_staging_file(self, tmp_path):
        """
        GIVEN a stale .stg file exists from a previous failed job
        WHEN ParquetSink is initialized with the same job_id
        THEN the stale file should be deleted
        """
        output_root = tmp_path / "output"
        output_root.mkdir()
        
        job_id = 101
        file_version_id = 1
        
        # Create a stale .stg file (simulating previous failed job)
        stale_staging_file = output_root / f"data_{job_id}.parquet.stg.{job_id}"
        stale_staging_file.write_text("stale data from previous attempt")
        assert stale_staging_file.exists(), "Precondition: stale file should exist"
        
        # Initialize ParquetSink - should trigger destructive cleanup
        sink = ParquetSink(
            root_path=output_root,
            relative_path="data.parquet",
            options={},
            job_id=job_id,
            file_version_id=file_version_id,
        )
        
        # Verify stale file was cleaned up
        assert not stale_staging_file.exists(), "Destructive Init FAILED: stale .stg file should be deleted"
        
        # Optional: Verify sink paths are set correctly
        assert f"_{job_id}.parquet" in str(sink.final_path)
        
    def test_parquet_sink_cleans_stale_staging_directory(self, tmp_path):
        """
        GIVEN a stale .stg directory exists from a previous partitioned write
        WHEN ParquetSink is initialized
        THEN the stale directory should be removed
        """
        output_root = tmp_path / "output"
        output_root.mkdir()
        
        job_id = 102
        file_version_id = 1
        
        # Create a stale staging directory (partitioned parquet)
        stale_staging_dir = output_root / f"data_{job_id}.parquet.stg.{job_id}"
        stale_staging_dir.mkdir()
        (stale_staging_dir / "part-0000.parquet").write_text("stale partition")
        assert stale_staging_dir.is_dir(), "Precondition: stale dir should exist"
        
        # Initialize ParquetSink
        sink = ParquetSink(
            root_path=output_root,
            relative_path="data.parquet",
            options={},
            job_id=job_id,
            file_version_id=file_version_id,
        )
        
        # Verify stale directory was cleaned up
        assert not stale_staging_dir.exists(), "Destructive Init FAILED: stale .stg directory should be deleted"

    def test_sqlite_sink_drops_stale_staging_table(self, tmp_path):
        """
        GIVEN a stale staging table exists from a previous failed job
        WHEN SqliteSink is initialized with the same job_id
        THEN the stale table should be dropped
        """
        db_path = tmp_path / "test.db"
        job_id = 201
        file_version_id = 1
        table_name = "output_data"
        staging_table_name = f"{table_name}_stg_{job_id}"
        
        # Create database with a stale staging table
        engine = create_engine(f"sqlite:///{db_path}")
        with engine.begin() as conn:
            conn.execute(text(f"CREATE TABLE {staging_table_name} (id INT, value TEXT)"))
            conn.execute(text(f"INSERT INTO {staging_table_name} VALUES (1, 'stale')"))
        
        # Verify precondition
        inspector = inspect(engine)
        assert staging_table_name in inspector.get_table_names(), "Precondition: stale table should exist"
        engine.dispose()
        
        # Initialize SqliteSink - should drop stale staging table
        sink = SqliteSink(
            db_path=str(db_path),
            table_name=table_name,
            options={},
            job_id=job_id,
            file_version_id=file_version_id,
        )
        
        # Verify stale table was dropped
        inspector = inspect(sink.engine)
        table_names = inspector.get_table_names()
        assert staging_table_name not in table_names, f"Destructive Init FAILED: stale table {staging_table_name} should be dropped"
        
        sink.close()

    def test_sqlite_sink_handles_no_stale_table(self, tmp_path):
        """
        GIVEN no stale staging table exists
        WHEN SqliteSink is initialized
        THEN initialization should succeed without error
        """
        db_path = tmp_path / "clean.db"
        job_id = 301
        
        # Initialize SqliteSink on fresh database
        sink = SqliteSink(
            db_path=str(db_path),
            table_name="fresh_table",
            options={},
            job_id=job_id,
            file_version_id=1,
        )
        
        # Should not raise any errors
        assert sink.staging_table == f"fresh_table_stg_{job_id}"
        sink.close()
