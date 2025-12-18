from casparian_flow.sdk import BasePlugin, PluginMetadata
import pandas as pd


MANIFEST = PluginMetadata(
    pattern="*MCDATA",
    topics=["pfc_db_data"]
)


class Handler(BasePlugin):
    def execute(self, file_path: str):
        records = []
        
        with open(file_path, 'r', encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                
                parts = line.split(',')
                
                # Filter only rows where column 2 (index 1) equals 'pfc_db'
                if len(parts) > 1 and parts[1] == 'pfc_db':
                    # Pad to ensure we have at least 20 fields
                    while len(parts) < 20:
                        parts.append('')
                    
                    # Parse record according to schema
                    record = {
                        'record_id': int(parts[0]) if parts[0] else 0,
                        'record_type': parts[1],
                        'field_3': parts[2],
                        'timestamp': pd.to_datetime(parts[3], errors='coerce') if len(parts) > 3 else pd.NaT,
                        'field_5': parts[4] if len(parts) > 4 else '',
                        'field_6': parts[5] if len(parts) > 5 else '',
                        'field_7': parts[6] if len(parts) > 6 else '',
                        'field_8': parts[7] if len(parts) > 7 else '',
                        'field_9': parts[8] if len(parts) > 8 else '',
                        'field_10': parts[9] if len(parts) > 9 else '',
                        'field_11': parts[10] if len(parts) > 10 else '',
                        'field_12': parts[11] if len(parts) > 11 else '',
                        'field_13': parts[12] if len(parts) > 12 else '',
                        'field_14': parts[13] if len(parts) > 13 else '',
                        'field_15': parts[14] if len(parts) > 14 else '',
                        'field_16': parts[15] if len(parts) > 15 else '',
                        'field_17': parts[16] if len(parts) > 16 else '',
                        'field_18': parts[17] if len(parts) > 17 else '',
                        'field_19': parts[18] if len(parts) > 18 else '',
                        'field_20': parts[19] if len(parts) > 19 else ''
                    }
                    records.append(record)
        
        df = pd.DataFrame(records)
        self.publish('pfc_db_data', df)