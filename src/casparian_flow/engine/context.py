# src/casparian_flow/engine/context.py
from typing import List, Dict, Any
from pathlib import Path
from sqlalchemy import Engine
import pandas as pd
from casparian_flow.engine.sinks import SinkFactory, DataSink

class InspectionInterrupt(Exception):
    """Raised to stop the plugin once we have captured the schema."""
    pass

class WorkerContext:
    def __init__(self, 
                 sql_engine: Engine, 
                 parquet_root: Path, 
                 topic_map: Dict[str, str] = None, 
                 inspect_mode: bool = False):
        
        self.sql_engine = sql_engine
        self.parquet_root = parquet_root
        
        # The Map from UI: {"sales": "mssql://prod/Sales"}
        self.topic_map = topic_map or {} 
        
        self.sinks: List[DataSink] = []
        self.topic_names: List[str] = []
        
        # Inspection State
        self.inspect_mode = inspect_mode
        self.rows_processed = 0
        self.INSPECT_LIMIT = 50 # Stop after 50 rows to be safe
        self.captured_schemas: Dict[str, Any] = {} 

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        """
        Resolves a logical topic to a physical URI.
        """
        self.topic_names.append(topic)

        # 1. UI Mapping (Priority)
        uri = self.topic_map.get(topic)
        
        # 2. Code Default / Fallback
        if not uri:
            uri = default_uri or f"parquet://{topic}.parquet"

        # Create Sink
        sink = SinkFactory.create(uri, self.sql_engine, self.parquet_root)
        self.sinks.append(sink)
        
        return len(self.sinks) - 1

    def publish(self, handle: int, data: Any):
        """
        Dispatches data. Intercepts for Inspection if needed.
        """
        if self.inspect_mode:
            self._handle_inspection(handle, data)
            return

        try:
            self.sinks[handle].write(data)
        except IndexError:
            raise ValueError(f"Invalid topic handle: {handle}")

    def _handle_inspection(self, handle: int, data: Any):
        """
        Captures schema and halts execution.
        """
        topic_name = self.topic_names[handle]

        # Capture Schema if we haven't already for this topic
        if topic_name not in self.captured_schemas:
            schema_info = {}
            
            # Prefer Arrow Schema
            if hasattr(data, "schema"):
                # Convert Arrow schema to simple JSON-able dict
                schema_info = {n: str(t) for n, t in zip(data.schema.names, data.schema.types)}
            
            # Fallback to Pandas dtypes
            elif hasattr(data, "dtypes"):
                schema_info = data.dtypes.astype(str).to_dict()
                
            self.captured_schemas[topic_name] = schema_info

        # Count rows to enforce limit
        count = len(data) if hasattr(data, "__len__") else 1
        self.rows_processed += count
        
        if self.rows_processed >= self.INSPECT_LIMIT:
            raise InspectionInterrupt("Inspection limit reached.")

    def close_all(self):
        for s in self.sinks:
            s.close()
        self.sinks.clear()