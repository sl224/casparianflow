"""
Transport RSM EGI Maintenance Data Handler

Processes Transport RSM (Railway System Management) EGI (Electronic Geographic Information)
maintenance sensor data. Expected format: CSV with maintenance readings and diagnostics.
"""

import pandas as pd
from casparian_flow.sdk import BasePlugin


class Handler(BasePlugin):
    """
    Handler for Transport RSM EGI Maintenance Data.
    
    Reads CSV files containing railway maintenance sensor readings.
    Expected file naming: {equipment_id}_{timestamp}_EGI{sensor}_{region}_{zone}.csv
    """
    
    def execute(self, file_path: str):
        """
        Process Transport RSM EGI maintenance CSV file.
        
        Args:
            file_path: Path to the CSV file containing maintenance data
            
        Yields:
            pandas.DataFrame: Chunks of processed maintenance data
        """
        try:
            # Read CSV with pandas (as per strategy requirement)
            # Using chunksize for memory-efficient processing of large files
            chunk_size = 10000
            
            # Read in chunks to handle potentially large maintenance logs
            for chunk in pd.read_csv(
                file_path,
                chunksize=chunk_size,
                # Common CSV options for maintenance data
                na_values=['', 'NA', 'N/A', 'null', 'NULL'],
                low_memory=False,
                # Infer types but keep flexibility
                dtype_backend='numpy_nullable'
            ):
                # Skip empty chunks
                if chunk.empty:
                    continue
                
                # Basic validation: ensure we have data
                if len(chunk) > 0:
                    yield chunk
                    
        except pd.errors.EmptyDataError:
            # Handle empty files gracefully
            self.logger.warning(f"File {file_path} is empty")
            return
        except Exception as e:
            self.logger.error(f"Error processing {file_path}: {str(e)}")
            raise