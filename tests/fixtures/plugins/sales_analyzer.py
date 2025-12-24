"""
Sales Analyzer Plugin
Analyzes CSV files containing sales data and generates insights.
"""
import pandas as pd
from pathlib import Path
from typing import Any, Dict

# Plugin metadata
MANIFEST = {
    "name": "sales_analyzer",
    "version": "1.0.0",
    "subscriptions": ["csv", "sales", "data"],
    "sinks": {
        "output": {
            "uri": "parquet://data/output/sales_analyzed.parquet",
            "mode": "append"
        }
    }
}


class Plugin:
    """Analyzes sales CSV files."""

    def __init__(self, config: Dict[str, Any]):
        """Initialize the plugin."""
        self.config = config
        print(f"[SalesAnalyzer] Initialized")

    def consume(self, file_path: Path, context) -> Dict[str, Any]:
        """
        Process a sales CSV file.

        Args:
            file_path: Path to the input CSV file
            context: Execution context

        Returns:
            Processing results
        """
        print(f"[SalesAnalyzer] Processing: {file_path.name}")

        try:
            # Read CSV
            df = pd.read_csv(file_path)
            print(f"[SalesAnalyzer] Loaded {len(df)} rows, columns: {list(df.columns)}")

            # Add analysis metadata
            df['_analyzed_by'] = 'sales_analyzer'
            df['_source'] = file_path.name

            # Publish to output
            output_handle = context.register_topic("output")
            context.publish(output_handle, df)

            print(f"[SalesAnalyzer] Published {len(df)} rows")

            return {
                "status": "success",
                "rows": len(df),
                "columns": list(df.columns),
                "file": file_path.name
            }

        except Exception as e:
            print(f"[SalesAnalyzer] Error: {e}")
            return {"status": "error", "error": str(e)}
