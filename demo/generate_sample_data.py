#!/usr/bin/env python3
"""
Generate sample parquet files for the UI demo.

Creates realistic output files that the Data tab can query.
"""

import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq
from pathlib import Path
from datetime import datetime, timedelta
import random

# Output directory
OUTPUT_DIR = Path(__file__).parent / "output"
OUTPUT_DIR.mkdir(exist_ok=True)


def generate_processed_output():
    """Generate a sample processed_output.parquet file."""
    # Simulate data from slow_processor
    data = []
    base_time = datetime.now() - timedelta(hours=1)

    for i in range(100):
        batch_num = (i // 5) + 1
        data.append({
            "id": i + 1,
            "name": f"Item_{i+1:03d}",
            "value": round(random.uniform(50, 500), 2),
            "category": random.choice(["A", "B", "C", "D"]),
            "timestamp": (base_time + timedelta(minutes=i)).isoformat(),
            "_batch": batch_num,
            "_processed_at": datetime.now().isoformat(),
        })

    df = pd.DataFrame(data)
    output_path = OUTPUT_DIR / "processed_output.parquet"
    df.to_parquet(output_path, index=False)
    print(f"Created: {output_path} ({len(df)} rows)")


def generate_validated_output():
    """Generate a sample validated.parquet file."""
    data = []
    base_time = datetime.now() - timedelta(minutes=30)

    for i in range(75):  # Fewer rows (some filtered out)
        data.append({
            "id": i + 1,
            "name": f"Valid_{i+1:03d}",
            "value": round(random.uniform(100, 400), 2),
            "status": "validated",
            "score": round(random.uniform(0.7, 1.0), 3),
            "_validated_at": (base_time + timedelta(seconds=i*10)).isoformat(),
        })

    df = pd.DataFrame(data)
    output_path = OUTPUT_DIR / "validated.parquet"
    df.to_parquet(output_path, index=False)
    print(f"Created: {output_path} ({len(df)} rows)")


def generate_errors_output():
    """Generate a sample errors.parquet file."""
    error_types = ["null_value", "out_of_range", "invalid_format", "duplicate"]

    data = []
    base_time = datetime.now() - timedelta(minutes=30)

    for i in range(15):  # Small number of errors
        data.append({
            "record_id": random.randint(1, 1000),
            "error_type": random.choice(error_types),
            "error_message": f"Validation failed: {random.choice(error_types)}",
            "field_name": random.choice(["value", "timestamp", "category", "name"]),
            "detected_at": (base_time + timedelta(seconds=i*60)).isoformat(),
        })

    df = pd.DataFrame(data)
    output_path = OUTPUT_DIR / "errors.parquet"
    df.to_parquet(output_path, index=False)
    print(f"Created: {output_path} ({len(df)} rows)")


def generate_mixed_types():
    """Generate parquet with various data types for testing DataGrid."""
    data = {
        "int_col": [1, 2, 3, None, 5, 1000000, -42],
        "float_col": [3.14159, 2.71828, None, 0.0001, 1e10, -0.5, 42.0],
        "str_col": ["hello", "world", None, "", "with spaces", "特殊字符", "emoji 🎉"],
        "bool_col": [True, False, True, None, False, True, False],
        "timestamp": [
            "2024-01-01T00:00:00",
            "2024-06-15T12:30:45",
            None,
            "2024-12-31T23:59:59",
            "2025-01-01T00:00:00",
            "2024-07-04T04:00:00",
            "2024-02-29T12:00:00",  # Leap year
        ],
    }

    df = pd.DataFrame(data)
    output_path = OUTPUT_DIR / "mixed_types.parquet"
    df.to_parquet(output_path, index=False)
    print(f"Created: {output_path} ({len(df)} rows)")


if __name__ == "__main__":
    print("Generating sample parquet files for demo...")
    print()

    generate_processed_output()
    generate_validated_output()
    generate_errors_output()
    generate_mixed_types()

    print()
    print("Done! Files created in:", OUTPUT_DIR)
