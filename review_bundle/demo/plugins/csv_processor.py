"""
CSV Processor Plugin

A straightforward CSV processor that:
1. Reads CSV files
2. Adds processing metadata
3. Returns Output with sink destination
"""

import pandas as pd
import pyarrow as pa
from casparian_types import Output

TOPIC = "csv_output"
SINK = "parquet"


def parse(file_path: str) -> pa.Table:
    """
    Process a CSV file and return Arrow table.

    Args:
        file_path: Path to input CSV file

    Returns:
        Arrow table of processed data
    """
    print(f"[csv-processor] Reading: {file_path}")

    # Read the CSV file
    df = pd.read_csv(file_path)
    total_rows = len(df)

    print(f"[csv-processor] Loaded {total_rows} rows, {len(df.columns)} columns")

    # Add processing metadata
    df["_source_file"] = file_path
    df["_processed_at"] = pd.Timestamp.now().isoformat()
    df["_row_count"] = total_rows

    # Convert to Arrow table
    table = pa.Table.from_pandas(df)
    print(f"[csv-processor] Returning Arrow table with schema: {table.schema}")

    print(f"[csv-processor] Complete: {file_path}")
    return table
