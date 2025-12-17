from casparian_flow.sdk import BasePlugin
import pandas as pd
import pyarrow as pa


class Handler(BasePlugin):
    def execute(self, file_path: str):
        """
        Read CSV file with sales transaction data and yield as Arrow Table.
        """
        # Read CSV with pandas, parsing date column as datetime
        df = pd.read_csv(
            file_path,
            parse_dates=['date'],
            dtype={
                'product': 'str',
                'quantity': 'int64',
                'price': 'float64',
                'region': 'str'
            }
        )
        
        # Convert to Arrow Table for zero-copy efficiency
        table = pa.Table.from_pandas(df)
        
        # Yield the table
        yield table