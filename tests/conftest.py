"""
Pytest fixtures and configuration for Casparian Flow tests.
"""
import pytest
import shutil
import os
from pathlib import Path
from sqlalchemy import create_engine
from casparian_flow.config import settings
from casparian_flow.db.base_session import SessionLocal
from casparian_flow.db.setup import initialize_database, get_or_create_sourceroot
from casparian_flow.db.models import PluginConfig, TopicConfig


# =============================================================================
# VCR Configuration for Azure Integration Tests
# =============================================================================


@pytest.fixture(scope="module")
def vcr_config():
    """Configure VCR.py for Azure integration tests with secret scrubbing."""
    return {
        "cassette_library_dir": "tests/cassettes",
        "record_mode": "once",  # Record on first run, then replay
        "match_on": ["method", "scheme", "host", "port", "path", "query"],
        "filter_headers": [
            "Authorization",  # Remove bearer tokens
            "client-secret",  # Remove Azure client secrets
        ],
        "filter_post_data_parameters": [
            "client_secret",
            "client_assertion",
        ],
        "before_record_response": scrub_response_data,
    }


def scrub_response_data(response):
    """Sanitize sensitive data from VCR cassettes before recording."""
    # Scrub access tokens and refresh tokens from response bodies
    if "body" in response and "string" in response["body"]:
        body = response["body"]["string"]
        if isinstance(body, bytes):
            body_str = body.decode("utf-8")
            # Replace tokens with dummy values
            import json
            try:
                data = json.loads(body_str)
                if "access_token" in data:
                    data["access_token"] = "SCRUBBED_ACCESS_TOKEN"
                if "refresh_token" in data:
                    data["refresh_token"] = "SCRUBBED_REFRESH_TOKEN"
                if "id_token" in data:
                    data["id_token"] = "SCRUBBED_ID_TOKEN"
                response["body"]["string"] = json.dumps(data).encode("utf-8")
            except (json.JSONDecodeError, TypeError):
                pass  # Not JSON, skip scrubbing
    return response


@pytest.fixture(scope="function")
def temp_test_dir(tmp_path):
    """Create a temporary test directory with cleanup."""
    test_dir = tmp_path / "test_data"
    test_dir.mkdir()
    yield test_dir
    # Cleanup happens automatically with tmp_path


@pytest.fixture(scope="function")
def test_db_engine(tmp_path):
    """Create a test database engine with cleanup."""
    db_path = tmp_path / "test_casparian_flow.sqlite3"
    db_url = f"sqlite:///{db_path}"
    engine = create_engine(db_url)
    
    # Initialize with reset
    initialize_database(engine, reset_tables=True)
    
    yield engine
    
    # Cleanup
    engine.dispose()
    # No need to unlink, tmp_path handles cleanup


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


# New fixtures for v2.0 testing


@pytest.fixture
def architect_service(test_db_engine):
    """Architect service with test secret key."""
    from casparian_flow.services.architect import ArchitectService

    return ArchitectService(test_db_engine, "test-secret-key")


@pytest.fixture
def sample_plugin_code():
    """Valid plugin source code for testing."""
    return """
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.DataFrame({"test": [1, 2, 3]})
        self.publish("output", df)
"""


@pytest.fixture
def dangerous_plugin_code():
    """Unsafe plugin code for validation tests."""
    return """
import os
import subprocess

class BadPlugin:
    def execute(self, file_path):
        os.system("rm -rf /")
"""


# =============================================================================
# v5.0 Bridge Mode Fixtures
# =============================================================================


@pytest.fixture
def sample_lockfile():
    """Sample uv.lock content for testing."""
    return """
version = 1
requires-python = ">=3.11"

[[package]]
name = "pandas"
version = "2.2.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "numpy" },
    { name = "python-dateutil" },
]

[[package]]
name = "numpy"
version = "1.26.0"
source = { registry = "https://pypi.org/simple" }
"""


@pytest.fixture
def bridge_plugin_code():
    """Valid plugin source code for Bridge Mode testing."""
    return """
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Simple plugin that reads CSV and outputs
        df = pd.read_csv(file_path)
        return [df]
"""


@pytest.fixture
def local_identity_provider(tmp_path):
    """LocalProvider with ephemeral keys for testing."""
    from casparian_flow.security.local_provider import LocalProvider
    return LocalProvider(keys_dir=tmp_path, ephemeral=True)
