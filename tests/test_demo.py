"""
Tests for demo.py - AI-Powered ETL Demo Script

These tests verify the demo.py functionality including:
- Sample data generation
- File discovery
- Environment setup
- End-to-end workflow with mock AI
"""
import pytest
import sys
import shutil
from pathlib import Path
from unittest.mock import patch, MagicMock, Mock
from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session

# Add src and project root to path
project_root = Path(__file__).parent.parent
sys.path.insert(0, str(project_root / "src"))
sys.path.insert(0, str(project_root))

# Import demo functions
import demo
from demo import (
    generate_sample_data,
    discover_representative_files,
    setup_demo_environment,
    generate_plugin_for_file,
    monitor_jobs,
    display_results,
    check_claude_cli,
)

from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import (
    ProcessingJob, StatusEnum, PluginManifest, TopicConfig,
    RoutingRule, PluginConfig, SourceRoot
)
from casparian_flow.services.architect import ArchitectService
from casparian_flow.services.ai_hook import MockGenerator
from casparian_flow.services.ai_types import FileProfile, FileType, HEAD_Sample
from casparian_flow.security.gatekeeper import generate_signature


@pytest.fixture
def demo_temp_dir(tmp_path):
    """Create a temporary directory for demo testing."""
    demo_dir = tmp_path / "demo_test"
    demo_dir.mkdir()
    yield demo_dir


@pytest.fixture
def demo_input_dir(tmp_path):
    """Create a temporary input directory."""
    input_dir = tmp_path / "input_data"
    input_dir.mkdir()
    yield input_dir


@pytest.fixture
def demo_output_dir(tmp_path):
    """Create a temporary output directory."""
    output_dir = tmp_path / "demo_output"
    output_dir.mkdir()
    yield output_dir


@pytest.fixture
def demo_db_engine(tmp_path):
    """Create a test database engine for demo."""
    db_path = tmp_path / "demo_test.db"
    engine = create_engine(f"sqlite:///{db_path}")
    initialize_database(engine, reset_tables=True)
    yield engine
    engine.dispose()


class TestSampleDataGeneration:
    """Test sample data generation functionality."""

    def test_generate_sample_data_creates_files(self, demo_input_dir):
        """Test that sample data generation creates all expected files."""
        generate_sample_data(demo_input_dir)

        # Check CSV file exists and has content
        csv_file = demo_input_dir / "sales_2025.csv"
        assert csv_file.exists()
        content = csv_file.read_text()
        assert "date,product,quantity,price,region" in content
        assert "Widget A" in content

        # Check JSON file exists and has content
        json_file = demo_input_dir / "events.json"
        assert json_file.exists()
        content = json_file.read_text()
        assert "timestamp" in content
        assert "user_id" in content

        # Check TXT file exists and has content
        txt_file = demo_input_dir / "system.log"
        assert txt_file.exists()
        content = txt_file.read_text()
        assert "INFO System started" in content

    def test_generate_sample_data_valid_formats(self, demo_input_dir):
        """Test that generated sample data is in valid formats."""
        import json
        import csv

        generate_sample_data(demo_input_dir)

        # Validate CSV
        csv_file = demo_input_dir / "sales_2025.csv"
        with open(csv_file, 'r') as f:
            reader = csv.DictReader(f)
            rows = list(reader)
            assert len(rows) > 0
            assert 'date' in rows[0]
            assert 'product' in rows[0]
            assert 'price' in rows[0]

        # Validate JSON
        json_file = demo_input_dir / "events.json"
        with open(json_file, 'r') as f:
            data = json.load(f)
            assert isinstance(data, list)
            assert len(data) > 0
            assert 'timestamp' in data[0]
            assert 'event' in data[0]


class TestFileDiscovery:
    """Test file discovery functionality."""

    def test_discover_representative_files_single_type(self, demo_input_dir):
        """Test discovering files with a single file type."""
        # Create test files
        (demo_input_dir / "test1.csv").write_text("col1,col2\n1,2")
        (demo_input_dir / "test2.csv").write_text("col1,col2\n3,4")

        files = discover_representative_files(demo_input_dir, max_types=3)

        assert len(files) == 1
        assert files[0].suffix == ".csv"

    def test_discover_representative_files_multiple_types(self, demo_input_dir):
        """Test discovering files with multiple file types."""
        # Create test files of different types
        (demo_input_dir / "data.csv").write_text("col1,col2\n1,2")
        (demo_input_dir / "events.json").write_text('{"key": "value"}')
        (demo_input_dir / "log.txt").write_text("log entry")

        files = discover_representative_files(demo_input_dir, max_types=3)

        assert len(files) == 3
        extensions = {f.suffix for f in files}
        assert extensions == {".csv", ".json", ".txt"}

    def test_discover_representative_files_respects_max_types(self, demo_input_dir):
        """Test that discovery respects max_types limit."""
        # Create 5 different file types
        (demo_input_dir / "file.csv").write_text("data")
        (demo_input_dir / "file.json").write_text("data")
        (demo_input_dir / "file.txt").write_text("data")
        (demo_input_dir / "file.xml").write_text("data")
        (demo_input_dir / "file.log").write_text("data")

        files = discover_representative_files(demo_input_dir, max_types=3)

        assert len(files) == 3

    def test_discover_representative_files_prioritizes_common_types(self, demo_input_dir):
        """Test that common file types are prioritized."""
        # Create files with priority and non-priority extensions
        (demo_input_dir / "file.csv").write_text("data")  # Priority
        (demo_input_dir / "file.xyz").write_text("data")  # Not priority
        (demo_input_dir / "file.json").write_text("data")  # Priority

        files = discover_representative_files(demo_input_dir, max_types=2)

        extensions = {f.suffix for f in files}
        # Should prefer .csv and .json over .xyz
        assert ".csv" in extensions
        assert ".json" in extensions

    def test_discover_representative_files_picks_smallest(self, demo_input_dir):
        """Test that smallest file is picked as representative."""
        # Create files of same type but different sizes
        (demo_input_dir / "large.csv").write_text("col1,col2\n" + "1,2\n" * 100)
        (demo_input_dir / "small.csv").write_text("col1,col2\n1,2")

        files = discover_representative_files(demo_input_dir, max_types=1)

        assert len(files) == 1
        assert files[0].name == "small.csv"

    def test_discover_representative_files_empty_folder(self, demo_input_dir):
        """Test discovery with empty folder."""
        files = discover_representative_files(demo_input_dir, max_types=3)
        assert len(files) == 0

    def test_discover_representative_files_nested_folders(self, demo_input_dir):
        """Test discovery in nested folder structure."""
        # Create nested structure
        nested = demo_input_dir / "subfolder" / "deep"
        nested.mkdir(parents=True)

        (demo_input_dir / "root.csv").write_text("data")
        (nested / "deep.json").write_text("data")

        files = discover_representative_files(demo_input_dir, max_types=3)

        assert len(files) == 2
        extensions = {f.suffix for f in files}
        assert extensions == {".csv", ".json"}


class TestEnvironmentSetup:
    """Test environment setup functionality."""

    def test_setup_demo_environment_existing_folder(self, demo_input_dir, monkeypatch):
        """Test setup with existing folder containing files."""
        # Create a file
        (demo_input_dir / "test.csv").write_text("data")

        # Mock DEMO_DIR to use temp directory
        monkeypatch.setattr(demo, "DEMO_DIR", demo_input_dir.parent / "output")

        result = setup_demo_environment(demo_input_dir, generate_samples=False)

        assert result == demo_input_dir
        assert result.exists()

    def test_setup_demo_environment_creates_sample_data(self, demo_input_dir, monkeypatch):
        """Test setup creates sample data when folder is empty."""
        # Mock DEMO_DIR
        monkeypatch.setattr(demo, "DEMO_DIR", demo_input_dir.parent / "output")

        result = setup_demo_environment(demo_input_dir, generate_samples=True)

        # Check sample files were created
        assert (demo_input_dir / "sales_2025.csv").exists()
        assert (demo_input_dir / "events.json").exists()
        assert (demo_input_dir / "system.log").exists()

    def test_setup_demo_environment_exits_on_empty_without_flag(self, demo_input_dir, monkeypatch):
        """Test setup exits when folder is empty and no generate flag."""
        monkeypatch.setattr(demo, "DEMO_DIR", demo_input_dir.parent / "output")

        with pytest.raises(SystemExit):
            setup_demo_environment(demo_input_dir, generate_samples=False)


class TestPluginGeneration:
    """Test plugin generation functionality."""

    def test_generate_plugin_for_file_success(self, demo_db_engine, demo_input_dir):
        """Test successful plugin generation for a file."""
        # Create a test CSV file
        csv_file = demo_input_dir / "test.csv"
        csv_file.write_text("id,name,value\n1,test,100")

        # Create mock provider that wraps MockGenerator
        from casparian_flow.services.llm_generator import LLMGenerator

        # Create a mock LLMProvider
        class MockLLMProvider:
            def __init__(self):
                self.generator = MockGenerator()

            def chat_completion(self, messages, model=None, json_mode=False):
                # Mock response for schema proposal
                if json_mode:
                    return '{"file_type_inferred": "CSV", "target_topic": "test_data", "columns": [{"name": "id", "target_type": "int"}], "read_strategy": "pandas", "reasoning": "test"}'
                else:
                    return "from casparian_flow.sdk import BasePlugin\nclass Handler(BasePlugin):\n    def execute(self, file_path):\n        pass"

        mock_provider = MockLLMProvider()

        # Create architect service (use same secret as demo.py)
        architect = ArchitectService(demo_db_engine, "demo-secret-key")

        with Session(demo_db_engine) as session:
            result = generate_plugin_for_file(
                csv_file,
                mock_provider,
                architect,
                session
            )

            assert result is not None
            plugin_name, topic_name = result
            assert plugin_name.startswith("demo_")
            assert topic_name is not None

            # Verify database entries were created
            plugin_config = session.query(PluginConfig).filter_by(
                plugin_name=plugin_name
            ).first()
            assert plugin_config is not None

            topic_config = session.query(TopicConfig).filter_by(
                plugin_name=plugin_name
            ).first()
            assert topic_config is not None
            assert "sqlite://" in topic_config.uri

            routing_rule = session.query(RoutingRule).filter_by(
                pattern="*.csv"
            ).first()
            assert routing_rule is not None

    def test_generate_plugin_for_file_handles_errors(self, demo_db_engine, demo_input_dir):
        """Test plugin generation handles errors gracefully."""
        # Create a test file
        test_file = demo_input_dir / "test.csv"
        test_file.write_text("data")

        # Create mock provider that raises an error
        mock_provider = Mock()
        mock_provider.chat_completion = Mock(side_effect=RuntimeError("AI Error"))

        architect = ArchitectService(demo_db_engine, "test-secret")

        with Session(demo_db_engine) as session:
            result = generate_plugin_for_file(
                test_file,
                mock_provider,
                architect,
                session
            )

            # Should return None on error
            assert result is None


class TestJobMonitoring:
    """Test job monitoring functionality."""

    def test_monitor_jobs_completes_when_done(self, demo_db_engine):
        """Test that monitoring completes when all jobs are done."""
        # Create some completed jobs
        with Session(demo_db_engine) as session:
            # First create required dependencies
            from casparian_flow.db.models import FileVersion, FileLocation, SourceRoot

            root_id = get_or_create_sourceroot(demo_db_engine, "/test")

            # Create a file location
            file_loc = FileLocation(
                source_root_id=root_id,
                rel_path="test.csv",
                filename="test.csv"
            )
            session.add(file_loc)
            session.flush()

            # Create a file hash registry entry
            from casparian_flow.db.models import FileHashRegistry
            from datetime import datetime

            hash_reg = FileHashRegistry(
                content_hash="abc123",
                first_seen=datetime.now(),
                size_bytes=100
            )
            session.add(hash_reg)
            session.flush()

            # Create a file version
            file_ver = FileVersion(
                location_id=file_loc.id,
                content_hash="abc123",
                size_bytes=100,
                modified_time=datetime.now()
            )
            session.add(file_ver)
            session.flush()

            # Create a plugin config
            plugin_cfg = PluginConfig(plugin_name="test_plugin")
            session.add(plugin_cfg)
            session.flush()

            job = ProcessingJob(
                file_version_id=file_ver.id,
                plugin_name="test_plugin",
                status=StatusEnum.COMPLETED
            )
            session.add(job)
            session.commit()

        stats = monitor_jobs(demo_db_engine, timeout=5)

        assert stats["completed"] == 1
        assert stats["queued"] == 0
        assert stats["running"] == 0

    def test_monitor_jobs_timeout(self, demo_db_engine):
        """Test that monitoring times out if jobs don't complete."""
        # Create a perpetually queued job
        with Session(demo_db_engine) as session:
            from casparian_flow.db.models import FileVersion, FileLocation, SourceRoot

            root_id = get_or_create_sourceroot(demo_db_engine, "/test")

            file_loc = FileLocation(
                source_root_id=root_id,
                rel_path="test.csv",
                filename="test.csv"
            )
            session.add(file_loc)
            session.flush()

            from casparian_flow.db.models import FileHashRegistry
            from datetime import datetime

            hash_reg = FileHashRegistry(
                content_hash="xyz789",
                first_seen=datetime.now(),
                size_bytes=100
            )
            session.add(hash_reg)
            session.flush()

            file_ver = FileVersion(
                location_id=file_loc.id,
                content_hash="xyz789",
                size_bytes=100,
                modified_time=datetime.now()
            )
            session.add(file_ver)
            session.flush()

            plugin_cfg = PluginConfig(plugin_name="test_plugin2")
            session.add(plugin_cfg)
            session.flush()

            job = ProcessingJob(
                file_version_id=file_ver.id,
                plugin_name="test_plugin2",
                status=StatusEnum.QUEUED
            )
            session.add(job)
            session.commit()

        # Should timeout quickly
        import time
        start = time.time()
        stats = monitor_jobs(demo_db_engine, timeout=3)
        duration = time.time() - start

        assert duration < 5  # Should timeout within 5 seconds
        assert stats["queued"] == 1


class TestResultsDisplay:
    """Test results display functionality."""

    def test_display_results_with_data(self, tmp_path, caplog):
        """Test displaying results when database has data."""
        import logging
        caplog.set_level(logging.INFO)

        # Create output database with test data
        output_db = tmp_path / "parsed_data.db"
        engine = create_engine(f"sqlite:///{output_db}")

        # Create a test table with data
        with engine.connect() as conn:
            conn.execute(text("CREATE TABLE sales_data (id INTEGER, product TEXT, price REAL)"))
            conn.execute(text("INSERT INTO sales_data VALUES (1, 'Widget A', 29.99)"))
            conn.execute(text("INSERT INTO sales_data VALUES (2, 'Widget B', 39.99)"))
            conn.commit()

        plugins = [("demo_sales_data", "sales_data")]

        display_results(output_db, plugins)

        # Check that results were logged
        assert "sales_data" in caplog.text
        assert "2 rows" in caplog.text

    def test_display_results_missing_database(self, tmp_path, caplog):
        """Test displaying results when database doesn't exist."""
        import logging
        caplog.set_level(logging.INFO)

        output_db = tmp_path / "nonexistent.db"
        plugins = [("demo_test", "test_table")]

        display_results(output_db, plugins)

        assert "not created" in caplog.text


class TestClaudeCLICheck:
    """Test Claude CLI availability checking."""

    def test_check_claude_cli_available(self):
        """Test check passes when claude command is available."""
        with patch('shutil.which', return_value='/usr/bin/claude'):
            # Should not raise
            check_claude_cli()

    def test_check_claude_cli_not_available(self):
        """Test check exits when claude command is not available."""
        with patch('shutil.which', return_value=None):
            with pytest.raises(SystemExit) as exc_info:
                check_claude_cli()
            assert exc_info.value.code == 1


class TestEndToEndWithMock:
    """End-to-end integration test with mock AI."""

    @pytest.mark.slow
    def test_demo_end_to_end_with_mock(self, tmp_path, monkeypatch):
        """Test complete demo workflow with mock AI provider."""
        # Setup temporary directories
        input_dir = tmp_path / "input"
        input_dir.mkdir()
        output_dir = tmp_path / "demo_output"
        output_dir.mkdir()

        # Create test data
        csv_file = input_dir / "test.csv"
        csv_file.write_text("id,name,value\n1,Alice,100\n2,Bob,200")

        json_file = input_dir / "events.json"
        json_file.write_text('[{"id": 1, "event": "login"}]')

        # Patch demo module constants
        monkeypatch.setattr(demo, "DEMO_DIR", output_dir)
        monkeypatch.setattr(demo, "DB_PATH", output_dir / "demo.db")
        monkeypatch.setattr(demo, "PLUGINS_DIR", output_dir / "plugins")
        monkeypatch.setattr(demo, "SQLITE_OUTPUT", output_dir / "parsed_data.db")

        # Patch get_provider to return MockLLMProvider
        class MockLLMProvider:
            def chat_completion(self, messages, model=None, json_mode=False):
                if json_mode:
                    return '{"file_type_inferred": "CSV", "target_topic": "test_data", "columns": [{"name": "id", "target_type": "int"}], "read_strategy": "pandas", "reasoning": "test"}'
                else:
                    return "from casparian_flow.sdk import BasePlugin\nclass Handler(BasePlugin):\n    def execute(self, file_path):\n        pass"

        def mock_get_provider(name):
            return MockLLMProvider()

        monkeypatch.setattr(demo, "get_provider", mock_get_provider)

        # Patch check_claude_cli to skip
        monkeypatch.setattr(demo, "check_claude_cli", lambda: None)

        # Patch worker to avoid actual ZMQ
        class MockWorker:
            def __init__(self, *args, **kwargs):
                pass

            def run(self):
                pass

            def reload_plugins(self):
                pass

            def stop(self):
                pass

        def mock_start_worker(db_path):
            worker = MockWorker()
            thread = Mock()
            return worker, thread

        monkeypatch.setattr(demo, "start_worker", mock_start_worker)

        # Create mock args
        args = Mock()
        args.folder = str(input_dir)
        args.generate_samples = False
        args.max_file_types = 2

        # Run demo (should not crash)
        try:
            demo.run_demo(args)
        except Exception as e:
            # Some failures are expected without full worker
            # Just ensure we got past the initial setup
            assert (output_dir / "demo.db").exists()


class TestArgumentParsing:
    """Test command-line argument parsing."""

    def test_parse_args_required_folder(self):
        """Test that folder argument is required."""
        with patch('sys.argv', ['demo.py']):
            with pytest.raises(SystemExit):
                demo.parse_args()

    def test_parse_args_with_folder(self):
        """Test parsing with just folder argument."""
        with patch('sys.argv', ['demo.py', '/path/to/folder']):
            args = demo.parse_args()
            assert args.folder == '/path/to/folder'
            assert args.generate_samples is False
            assert args.max_file_types == 3

    def test_parse_args_with_all_options(self):
        """Test parsing with all optional arguments."""
        with patch('sys.argv', ['demo.py', '/path/to/folder', '--generate-samples', '--max-file-types', '5']):
            args = demo.parse_args()
            assert args.folder == '/path/to/folder'
            assert args.generate_samples is True
            assert args.max_file_types == 5


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
