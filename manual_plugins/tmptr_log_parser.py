"""
TMPTR_LOG Parser Plugin

Parses TMPTR_LOG files (temperature monitoring logs).

PATTERN: *TMPTR_LOG*
TOPIC: tmptr_temperature_logs
"""
from casparian_flow.sdk import BasePlugin
import pandas as pd
import pyarrow as pa


class Handler(BasePlugin):
    """TMPTR_LOG parser for temperature monitoring data."""

    def execute(self, file_path: str):
        """
        Read TMPTR_LOG files and yield as Arrow tables.

        Args:
            file_path: Path to the TMPTR_LOG file

        Yields:
            pyarrow.Table with temperature monitoring data
        """
        try:
            # Read CSV without headers
            df = pd.read_csv(
                file_path,
                header=None,
                names=['component', 'date', 'time', 'sensor_location', 'temp_celsius', 'temp_fahrenheit'],
                dtype=str,  # Read as string to avoid parsing issues
                skipinitialspace=True,  # Trim whitespace
                encoding='utf-8',
                on_bad_lines='warn'
            )

            # Skip if empty
            if df.empty:
                return

            # Convert to Arrow table
            table = pa.Table.from_pandas(df)

            yield table

        except Exception as e:
            # Log error but don't crash
            print(f"Error processing {file_path}: {e}")
            raise
