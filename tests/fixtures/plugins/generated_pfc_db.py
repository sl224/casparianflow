# tests/fixtures/plugins/generated_pfc_db.py
from casparian_flow.sdk import BasePlugin, PluginMetadata, FileEvent
import pandas as pd

# Configuration
MANIFEST = PluginMetadata(
    # Subscribe to the tag that the RoutingRule assigns to *MCData* files
    subscriptions=["raw_mcdata_events"],
    # Define a default Parquet sink (The SQLite sink will be added via DB config)
    sinks={"pfc_db_data": "parquet://pfc_db_data"}
)

class Handler(BasePlugin):
    def consume(self, event: FileEvent):
        """
        Parses PFC_DB lines from the mixed log file.
        """
        rows = []
        try:
            with open(event.path, 'r', encoding='utf-8') as f:
                for line in f:
                    # Robust CSV splitting
                    parts = [p.strip() for p in line.split(',')]
                    
                    # Discriminator Check: Look for "PFC_DB:" in column 1
                    if len(parts) > 5 and parts[1] == "PFC_DB:":
                        # Map columns based on your sample data
                        rows.append({
                            "sequence_id": parts[0],
                            "record_type": parts[1],
                            "fault_id": parts[4],
                            "message": parts[5],
                            "timestamp": parts[6],
                            "subsystem": parts[7],
                            "category": parts[8],
                            "status": parts[12] if len(parts) > 12 else None
                        })
        except Exception as e:
            print(f"Error processing file: {e}")
            raise

        if rows:
            df = pd.DataFrame(rows)
            # Publish to the logical topic. 
            # The Sentinel will route this to BOTH Parquet and SQLite.
            self.publish("pfc_db_data", df)