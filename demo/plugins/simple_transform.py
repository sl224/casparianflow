"""
Simple Transform Plugin - Demo

A minimal plugin that demonstrates basic data transformation.
Good for quick testing without the artificial delays of slow_processor.
"""

import pandas as pd
import pyarrow as pa

MANIFEST = {
    "pattern": "**/*.csv",
    "topic": "transformed_output",
}


class Handler:
    """Simple transformer that adds computed columns."""

    def execute(self, file_path: str):
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

        yield pa.Table.from_pandas(df)
        print("[simple_transform] Done!")
