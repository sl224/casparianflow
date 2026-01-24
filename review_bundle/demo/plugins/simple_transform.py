"""
Simple Transform Plugin - Demo

A minimal plugin that demonstrates basic data transformation.
Good for quick testing without the artificial delays of slow_processor.
"""

import pandas as pd
import pyarrow as pa
from casparian_types import Output

TOPIC = "transformed_output"
SINK = "parquet"


def parse(file_path: str) -> pa.Table:
    """Simple transformer that adds computed columns."""
    print(f"[simple_transform] Processing: {file_path}")

    df = pd.read_csv(file_path)

    # Add some computed columns
    if "value" in df.columns:
        df["value_doubled"] = df["value"] * 2
        df["value_category"] = pd.cut(
            df["value"],
            bins=[0, 100, 200, 300, 500],
            labels=["low", "medium", "high", "very_high"]
        )

    df["_source_file"] = file_path
    df["_processed_at"] = pd.Timestamp.now().isoformat()

    print("[simple_transform] Done!")
    return pa.Table.from_pandas(df)
