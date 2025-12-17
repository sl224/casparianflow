from casparian_flow.sdk import BasePlugin
import pandas as pd


class Handler(BasePlugin):
    """E2D Ghost List processor - handles suppressed system messages/alerts."""
    
    def execute(self, file_path: str):
        """
        Read E2D Ghost List CSV and yield as DataFrame chunks.
        
        Args:
            file_path: Path to the CSV file containing ghost list data
            
        Yields:
            pandas.DataFrame with columns:
                - message_id (int)
                - system (str)
                - message_text (str)
                - display_flag (bool)
                - display_text (str)
                - category (str)
                - acknowledgment_required (str)
                - start_version (str)
                - end_version (str)
        """
        # Read CSV, skipping metadata header row
        df = pd.read_csv(
            file_path,
            skiprows=1,
            names=[
                'message_id',
                'system',
                'message_text',
                'display_flag',
                'display_text',
                'category',
                'acknowledgment_required',
                'start_version',
                'end_version'
            ],
            dtype={
                'message_id': 'Int64',
                'system': str,
                'message_text': str,
                'display_flag': bool,
                'display_text': str,
                'category': str,
                'acknowledgment_required': str,
                'start_version': str,
                'end_version': str
            },
            keep_default_na=True
        )
        
        yield df