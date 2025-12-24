"""
Test lineage propagation through Bridge Mode execution.

Verifies that file_version_id flows correctly:
Worker → BridgeExecutor → Guest Process → Plugin Handler
"""
import pytest
import tempfile
from pathlib import Path
from casparian_flow.engine.bridge import BridgeExecutor, BridgeError


def test_lineage_propagation_via_bridge():
    """
    Test that file_version_id propagates to the plugin via FileEvent.

    The plugin will raise ValueError if file_version_id doesn't match expected value.
    """
    # Spy plugin that verifies lineage
    spy_plugin_code = '''
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def consume(self, event):
        # CRITICAL: Verify file_version_id propagated correctly
        if event.file_id != 999:
            raise ValueError(f"Lineage Lost! Expected 999, got {event.file_id}")

        # Return dummy data to complete the test
        df = pd.DataFrame({"status": ["lineage_verified"], "file_id": [event.file_id]})
        return [df]
'''

    # Create temporary test file
    with tempfile.NamedTemporaryFile(mode='w', suffix='.csv', delete=False) as f:
        f.write("col1,col2\\n1,2\\n")
        test_file_path = f.name

    try:
        # Mock venv - use current Python interpreter for testing
        import sys
        interpreter_path = Path(sys.executable)

        # Create BridgeExecutor with specific file_version_id
        executor = BridgeExecutor(
            interpreter_path=interpreter_path,
            source_code=spy_plugin_code,
            file_path=test_file_path,
            job_id=123,
            file_version_id=999,  # Expected value
            timeout_seconds=30,
        )

        # Execute and verify - should NOT raise ValueError
        tables_received = []
        for table in executor.execute():
            tables_received.append(table)

        # Verify we got data back
        assert len(tables_received) > 0, "No tables received from bridge"

        # Verify the data contains our verification
        df = tables_received[0].to_pandas()
        assert df["status"].iloc[0] == "lineage_verified"
        assert df["file_id"].iloc[0] == 999

    finally:
        # Cleanup
        Path(test_file_path).unlink(missing_ok=True)


def test_lineage_failure_detection():
    """
    Test that incorrect file_version_id is detected by the plugin.

    This verifies the test itself is valid - if we pass wrong ID, it should fail.
    """
    # Spy plugin that expects file_id=777
    spy_plugin_code = '''
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def consume(self, event):
        if event.file_id != 777:
            raise ValueError(f"Expected 777, got {event.file_id}")
        return [pd.DataFrame({"ok": [1]})]
'''

    with tempfile.NamedTemporaryFile(mode='w', suffix='.csv', delete=False) as f:
        f.write("col1\\n1\\n")
        test_file_path = f.name

    try:
        import sys
        interpreter_path = Path(sys.executable)

        # Pass WRONG file_version_id (888 instead of 777)
        executor = BridgeExecutor(
            interpreter_path=interpreter_path,
            source_code=spy_plugin_code,
            file_path=test_file_path,
            job_id=456,
            file_version_id=888,  # Wrong value
            timeout_seconds=30,
        )

        # Should raise BridgeError due to ValueError in guest
        with pytest.raises(BridgeError, match="Expected 777, got 888"):
            list(executor.execute())

    finally:
        Path(test_file_path).unlink(missing_ok=True)


def test_lineage_with_legacy_execute_method():
    """
    Test lineage propagation when plugin uses legacy execute(path) method.

    Note: Legacy execute() doesn't receive FileEvent, so file_id won't be available.
    This test verifies the system doesn't crash.
    """
    legacy_plugin_code = '''
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path):
        # Legacy signature - no FileEvent, so no file_id access
        return [pd.DataFrame({"result": ["ok"]})]
'''

    with tempfile.NamedTemporaryFile(mode='w', suffix='.csv', delete=False) as f:
        f.write("col1\\n1\\n")
        test_file_path = f.name

    try:
        import sys
        interpreter_path = Path(sys.executable)

        executor = BridgeExecutor(
            interpreter_path=interpreter_path,
            source_code=legacy_plugin_code,
            file_path=test_file_path,
            job_id=789,
            file_version_id=555,
            timeout_seconds=30,
        )

        # Should execute without error
        tables = list(executor.execute())
        assert len(tables) > 0
        df = tables[0].to_pandas()
        assert df["result"].iloc[0] == "ok"

    finally:
        Path(test_file_path).unlink(missing_ok=True)
