from casparian_flow.sdk import BasePlugin, PluginMetadata, FileEvent
import pandas as pd

MANIFEST = PluginMetadata(subscriptions=["*.csv"])

class Handler(BasePlugin):
    def consume(self, event: FileEvent):
        """
        Read CSV with variable column lengths per row based on record_type discriminator.
        Parses each line individually to handle heterogeneous record structures.
        """
        records = []
        
        with open(event.path, 'r', encoding='utf-8') as f:
            header = f.readline().strip().split(',')
            
            for line in f:
                line = line.strip()
                if not line:
                    continue
                
                # Split the line and pad/truncate to match expected schema
                values = line.split(',')
                
                # Build record with up to 56 fields (record_id, record_type, field_3, timestamp, 
                # subsystem, component_type, status, data_field_1 through data_field_50)
                record = {
                    'record_id': None,
                    'record_type': None,
                    'field_3': None,
                    'timestamp': None,
                    'subsystem': None,
                    'component_type': None,
                    'status': None
                }
                
                # Add data_field_1 through data_field_50
                for i in range(1, 51):
                    record[f'data_field_{i}'] = None
                
                # Map values to fields based on actual columns present
                for idx, val in enumerate(values):
                    if idx < len(header):
                        col_name = header[idx].strip()
                        record[col_name] = val.strip() if val.strip() else None
                
                records.append(record)
        
        if records:
            df = pd.DataFrame(records)
            
            # Convert types where possible
            if 'record_id' in df.columns:
                df['record_id'] = pd.to_numeric(df['record_id'], errors='coerce').astype('Int64')
            
            if 'timestamp' in df.columns:
                df['timestamp'] = pd.to_datetime(df['timestamp'], errors='coerce')
            
            self.publish('mc_data_telemetry', df)