import pandas as pd
from casparian_flow.sdk import BasePlugin


class Handler(BasePlugin):
    def execute(self, file_path: str):
        """Read CSV temperature log using pandas and yield as chunks."""
        # Read CSV using pandas
        df = pd.read_csv(
            file_path,
            dtype={
                'device_id': str,
                'date': str,  # Keep as string initially to handle YYYYMMDD format
                'time': str,
                'component': str,
                'temp_celsius': str,
                'temp_fahrenheit': str
            }
        )
        
        # Convert date column from YYYYMMDD string to datetime
        df['date'] = pd.to_datetime(df['date'], format='%Y%m%d', errors='coerce')
        
        # Optional: Strip C/F suffix and convert to numeric
        df['temp_celsius_value'] = df['temp_celsius'].str.rstrip('C').astype(float)
        df['temp_fahrenheit_value'] = df['temp_fahrenheit'].str.rstrip('F').astype(float)
        
        # Yield the entire dataframe (or chunk it if needed)
        yield df