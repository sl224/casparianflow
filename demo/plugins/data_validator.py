"""
Data Validator Plugin - Demo

Validates data from the slow processor and categorizes into valid/error streams.
Used to demonstrate the pipeline topology with multiple outputs.
"""

import pandas as pd
import pyarrow as pa

# Plugin manifest
MANIFEST = {
    "pattern": "demo/output/processed_*.parquet",
    "topic": "validated_data",
}


class Handler:
    """
    Validates processed data and splits into valid/error streams.
    """

    def execute(self, file_path: str):
        """
        Validate data quality.

        Args:
            file_path: Path to input parquet file

        Yields:
            Arrow tables of validated data
        """
        print(f"[data_validator] Validating: {file_path}")

        # Read input
        df = pd.read_parquet(file_path)
        total = len(df)

        # Simple validation: check for null values
        valid_mask = df.notna().all(axis=1)
        valid_df = df[valid_mask].copy()
        error_df = df[~valid_mask].copy()

        print(f"[data_validator] {len(valid_df)}/{total} rows valid")

        if len(valid_df) > 0:
            valid_df["_validated_at"] = pd.Timestamp.now().isoformat()
            yield pa.Table.from_pandas(valid_df)

        print("[data_validator] Complete!")
