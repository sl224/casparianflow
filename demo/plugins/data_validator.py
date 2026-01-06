"""
Data Validator Plugin - Demo

Validates data from the slow processor and categorizes into valid/error streams.
Used to demonstrate the pipeline topology with multiple outputs.
"""

import pandas as pd
import pyarrow as pa
from casparian_types import Output

TOPIC = "validated_data"
SINK = "parquet"


def parse(file_path: str) -> list[Output]:
    """
    Validate data quality and split into valid/error outputs.

    Args:
        file_path: Path to input parquet file

    Returns:
        List of Output objects (valid data, optionally error data)
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

    outputs = []

    if len(valid_df) > 0:
        valid_df["_validated_at"] = pd.Timestamp.now().isoformat()
        outputs.append(Output("validated_data", pa.Table.from_pandas(valid_df), "parquet"))

    if len(error_df) > 0:
        error_df["_error_reason"] = "null_values"
        outputs.append(Output("validation_errors", pa.Table.from_pandas(error_df), "sqlite"))

    print("[data_validator] Complete!")
    return outputs
