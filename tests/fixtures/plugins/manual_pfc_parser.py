
from casparian_flow.sdk import BasePlugin, PluginMetadata, FileEvent
import pandas as pd
import sys

MANIFEST = PluginMetadata(subscriptions=["raw_mcdata_events"])

class Handler(BasePlugin):
    def consume(self, event: FileEvent):
        print(f"DEBUG: Processing {event.path}")
        rows = []
        try:
            with open(event.path, 'r', encoding='utf-8') as f:
                for line in f:
                    parts = line.strip().split(',')
                    if len(parts) < 10: 
                        continue
                    
                    if parts[1] == "PFC_DB:":
                        print(f"DEBUG: Found PFC_DB Row: {parts[4]}")
                        rows.append({
                            "sequence": parts[0],
                            "record_type": parts[1],
                            "fault_id": parts[4],
                            "description": parts[5],
                            "timestamp": parts[6],
                            "subsystem": parts[7],
                            "module": parts[8]
                        })
        except Exception as e:
            print(f"DEBUG: Error reading file: {e}")
            raise

        if rows:
            print(f"DEBUG: Publishing {len(rows)} rows to 'pfc_db'")
            df = pd.DataFrame(rows)
            self.publish("pfc_db", df)
        else:
            print("DEBUG: No rows matched.")
