"""
Test CSV Parser Plugin
A simple plugin for testing the import and job execution flow.
"""
import pandas as pd
from pathlib import Path
from typing import Any, Dict

# Plugin metadata
MANIFEST = {
    "name": "test_csv_parser",
    "version": "1.0.0",
    "subscriptions": ["csv", "data"],
    "sinks": {
        "output": {
            "uri": "parquet://data/output/csv_parsed.parquet",
            "mode": "append"
        },
        "summary": {
            "uri": "parquet://data/output/csv_summary.parquet",
            "mode": "append"
        }
    }
}


class Plugin:
    """Simple CSV parser that reads CSV files and outputs parsed data."""

    def __init__(self, config: Dict[str, Any]):
        """Initialize the plugin."""
        self.config = config
        print(f"[test_csv_parser] Initialized with config: {config}")

    def consume(self, file_path: Path, context) -> Dict[str, Any]:
        """
        Process a CSV file.

        Args:
            file_path: Path to the input file
            context: Execution context with publish() method

        Returns:
            Dictionary with processing results
        """
        print(f"[test_csv_parser] Processing file: {file_path}")

        try:
            # Read CSV file
            df = pd.read_csv(file_path)

            print(f"[test_csv_parser] Read {len(df)} rows, {len(df.columns)} columns")
            print(f"[test_csv_parser] Columns: {list(df.columns)}")

            # Add metadata
            df['_processed_by'] = 'test_csv_parser'
            df['_source_file'] = file_path.name

            # Publish full data to output topic
            output_handle = context.register_topic("output")
            context.publish(output_handle, df)
            print(f"[test_csv_parser] Published {len(df)} rows to 'output'")

            # Create summary
            summary = pd.DataFrame({
                'source_file': [file_path.name],
                'row_count': [len(df)],
                'column_count': [len(df.columns)],
                'columns': [','.join(df.columns)]
            })

            # Publish summary
            summary_handle = context.register_topic("summary")
            context.publish(summary_handle, summary)
            print(f"[test_csv_parser] Published summary to 'summary'")

            return {
                "status": "success",
                "rows_processed": len(df),
                "columns": list(df.columns)
            }

        except Exception as e:
            print(f"[test_csv_parser] Error: {e}")
            return {
                "status": "error",
                "error": str(e)
            }
