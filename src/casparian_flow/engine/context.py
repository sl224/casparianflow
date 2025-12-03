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
                 topic_config: Dict[str, Any] = None, 
                 inspect_mode: bool = False):
        
        self.sql_engine = sql_engine
        self.parquet_root = parquet_root
        self.topic_config = topic_config or {} 
        
        self.sinks: List[Dict[str, Any]] = []
        self.topic_names: List[str] = []
        
        self.inspect_mode = inspect_mode
        self.rows_processed = 0
        self.INSPECT_LIMIT = 50 
        self.captured_schemas: Dict[str, Any] = {} 

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        self.topic_names.append(topic)

        # 1. Get Config
        t_conf = self.topic_config.get(topic, {})
        uri = t_conf.get("uri")
        if not uri:
            uri = default_uri or f"parquet://{topic}.parquet"

        # 2. Create Physical Sink
        sink = SinkFactory.create(uri, self.sql_engine, self.parquet_root)
        
        # 3. Store sink + validation rules
        self.sinks.append({
            "sink": sink,
            "mode": t_conf.get("validation_mode", "infer"), # 'strict' or 'infer'
            "schema": t_conf.get("schema", None)
        })
        
        return len(self.sinks) - 1

    def publish(self, handle: int, data: Any):
        channel = self.sinks[handle]

        # Scenario A: Inspection Mode (The "Interrupt")
        if self.inspect_mode:
            self._handle_inspection(handle, data)
            return

        # Scenario B: Strict Enforcement
        if channel["mode"] == "strict" and channel["schema"]:
            self._validate_schema(data, channel["schema"])

        # Write
        channel["sink"].write(data)

    def _handle_inspection(self, handle: int, data: Any):
        topic_name = self.topic_names[handle]
        if topic_name not in self.captured_schemas:
            schema_info = {}
            if hasattr(data, "schema"): # Arrow
                schema_info = {n: str(t) for n, t in zip(data.schema.names, data.schema.types)}
            elif hasattr(data, "dtypes"): # Pandas
                schema_info = data.dtypes.astype(str).to_dict()
            self.captured_schemas[topic_name] = schema_info

        count = len(data) if hasattr(data, "__len__") else 1
        self.rows_processed += count
        
        if self.rows_processed >= self.INSPECT_LIMIT:
            raise InspectionInterrupt("Inspection limit reached.")

    def _validate_schema(self, data, required_schema):
        # TODO: Implement actual validation logic here
        # raising ValueError("Schema Mismatch") if columns fail
        pass

    def close_all(self):
        for s in self.sinks:
            s["sink"].close()
        self.sinks.clear()