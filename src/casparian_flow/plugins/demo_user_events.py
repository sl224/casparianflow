import pandas as pd
import pyarrow as pa
from casparian_flow.sdk import BasePlugin


class Handler(BasePlugin):
    """Handler for processing JSON user events data."""
    
    def execute(self, file_path: str):
        """
        Read JSON file containing user events and yield as PyArrow Table.
        
        Args:
            file_path: Path to the JSON file
            
        Yields:
            pyarrow.Table: User events data with schema-compliant types
        """
        # Read JSON file into pandas DataFrame
        df = pd.read_json(file_path)
        
        # Define explicit schema for type consistency
        schema = pa.schema([
            ("timestamp", pa.timestamp("us")),
            ("user_id", pa.int64()),
            ("event", pa.string()),
            ("duration_seconds", pa.int64()),
            ("amount", pa.float64()),
            ("source", pa.string())
        ])
        
        # Convert timestamp column to datetime if it's not already
        if not pd.api.types.is_datetime64_any_dtype(df["timestamp"]):
            df["timestamp"] = pd.to_datetime(df["timestamp"])
        
        # Ensure optional columns exist (fill with None if missing)
        for col in ["duration_seconds", "amount", "source"]:
            if col not in df.columns:
                df[col] = None
        
        # Convert to PyArrow Table with explicit schema
        table = pa.Table.from_pandas(df, schema=schema)
        
        # Yield the table for downstream processing
        yield table