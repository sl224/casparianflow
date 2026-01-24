"""
Minimal fixture plugin for testing.

Supports modes via environment variables:
- CF_FIXTURE_MODE=slow: sleep for CF_FIXTURE_SLEEP_SECS (default 10)
- CF_FIXTURE_MODE=collision: add _cf_job_id column (tests lineage collision)
- CF_FIXTURE_MODE=error: raise an error with CF_FIXTURE_ERROR_MSG
- CF_FIXTURE_MODE=normal: deterministic output (default)
- CF_FIXTURE_ROWS: number of rows to output (default 10)

Environment Variables:
    CF_FIXTURE_MODE: One of 'normal', 'slow', 'collision', 'error'
    CF_FIXTURE_ROWS: Number of rows to generate (default 10)
    CF_FIXTURE_SLEEP_SECS: Sleep duration in seconds for 'slow' mode (default 10)
    CF_FIXTURE_ERROR_MSG: Error message for 'error' mode (default "Fixture error")

Output Schema:
    id: int64 - Sequential row ID (0 to rows-1)
    value: string - String value "value_{id}"

Example Usage:
    # Normal mode - deterministic 10 rows
    CF_FIXTURE_MODE=normal casparian run fixture_plugin.py input.txt

    # Slow mode - sleeps 5 seconds before output
    CF_FIXTURE_MODE=slow CF_FIXTURE_SLEEP_SECS=5 casparian run fixture_plugin.py input.txt

    # Collision mode - adds reserved _cf_job_id column to test lineage collision detection
    CF_FIXTURE_MODE=collision casparian run fixture_plugin.py input.txt

    # Error mode - raises an exception
    CF_FIXTURE_MODE=error CF_FIXTURE_ERROR_MSG="Test error" casparian run fixture_plugin.py input.txt
"""
import os
import time

import pyarrow as pa

# Parser metadata (required by Casparian)
name = "fixture_plugin"
version = "1.0.0"
topics = ["fixture_topic"]

# Output schema declaration
outputs = {
    "fixture_output": pa.schema([
        ("id", pa.int64()),
        ("value", pa.string()),
    ])
}


def parse(file_path: str):
    """
    Parse function that generates deterministic test data.

    Behavior is controlled by environment variables - see module docstring.

    Args:
        file_path: Path to the input file (not used, but required by contract)

    Returns:
        List of Output tuples: [(output_name, arrow_table), ...]
    """
    from casparian_types import Output

    mode = os.environ.get("CF_FIXTURE_MODE", "normal")
    rows = int(os.environ.get("CF_FIXTURE_ROWS", "10"))

    # Handle slow mode - sleep before processing
    if mode == "slow":
        sleep_secs = int(os.environ.get("CF_FIXTURE_SLEEP_SECS", "10"))
        time.sleep(sleep_secs)

    # Handle error mode - raise an exception
    if mode == "error":
        error_msg = os.environ.get("CF_FIXTURE_ERROR_MSG", "Fixture error")
        raise RuntimeError(error_msg)

    # Build deterministic data
    data = {
        "id": list(range(rows)),
        "value": [f"value_{i}" for i in range(rows)],
    }

    # Handle collision mode - add reserved lineage column
    if mode == "collision":
        # Add reserved column to test lineage collision detection
        # The worker should reject this because _cf_* columns are reserved
        data["_cf_job_id"] = ["fake_job_id"] * rows

    table = pa.table(data)
    return [Output("fixture_output", table)]
