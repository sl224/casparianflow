"""
E2D CSV Parser Plugin

Parses E2D CSV files with flexible handling for various formats.

PATTERN: *.csv
TOPIC: e2d_csv_data
"""
from casparian_flow.sdk import BasePlugin
import pandas as pd
import pyarrow as pa


class Handler(BasePlugin):
    """Generic E2D CSV parser that handles various CSV formats."""

    def execute(self, file_path: str):
        """
        Read E2D CSV files and yield as Arrow tables.

        Args:
            file_path: Path to the CSV file

        Yields:
            pyarrow.Table with parsed data
        """
        try:
            # Read CSV with flexible options
            # Use dtype=str to avoid parsing errors, then convert later if needed
            df = pd.read_csv(
                file_path,
                dtype=str,  # Read everything as string first
                keep_default_na=False,  # Don't convert "NA" to NaN
                na_values=[''],  # Only empty strings are NA
                skip_blank_lines=True,
                encoding='utf-8',
                on_bad_lines='warn'  # Warn but continue on bad lines
            )

            # Skip if empty
            if df.empty:
                return

            # Convert to Arrow table for zero-copy transfer
            table = pa.Table.from_pandas(df)

            yield table

        except Exception as e:
            # Log error but don't crash
            print(f"Error processing {file_path}: {e}")
            raise
