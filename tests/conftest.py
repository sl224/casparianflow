"""
Pytest fixtures and configuration for Casparian Flow tests.
"""
import pytest
import shutil
from pathlib import Path
from sqlalchemy import create_engine
from casparian_flow.config import settings
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import PluginConfig, TopicConfig


@pytest.fixture(scope="function")
def temp_test_dir(tmp_path):
    """Create a temporary test directory with cleanup."""
    test_dir = tmp_path / "test_data"
    test_dir.mkdir()
    yield test_dir
    # Cleanup happens automatically with tmp_path


@pytest.fixture(scope="function")
def test_db_engine():
    """Create a test database engine with cleanup."""
    db_path = "test_casparian_flow.sqlite3"
    db_url = f"sqlite:///{db_path}"
    engine = create_engine(db_url)
    
    # Initialize with reset
    initialize_database(engine, reset_tables=True)
    
    yield engine
    
    # Cleanup
    engine.dispose()
    if Path(db_path).exists():
        Path(db_path).unlink()


@pytest.fixture(scope="function")
def test_db_session(test_db_engine):
    """Create a test database session."""
    session = SessionLocal(bind=test_db_engine)
    yield session
    session.close()


@pytest.fixture(scope="function")
def test_source_root(test_db_engine, temp_test_dir):
    """Create a test source root in the database."""
    root_id = get_or_create_sourceroot(
        test_db_engine,
        str(temp_test_dir.absolute())
    )
    return root_id


@pytest.fixture(scope="function")
def test_plugin_config(test_db_session):
    """Create a test plugin configuration."""
    plugin = PluginConfig(plugin_name="test_plugin")
    test_db_session.add(plugin)
    test_db_session.flush()
    return plugin


@pytest.fixture
def sink_configs():
    """Provide different sink configurations for parametrized tests."""
    return {
        "parquet": {
            "uri": "parquet://./output",
            "mode": "append",
            "verify_func": "verify_parquet_output"
        },
        "sqlite": {
            "uri": "sqlite://test_output.db/test_table",
            "mode": "append",
            "verify_func": "verify_sqlite_output"
        }
    }
