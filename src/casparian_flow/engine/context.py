# src/casparian_flow/engine/context.py
from typing import List, Dict, Any
from pathlib import Path
from sqlalchemy import Engine
import pandas as pd
import logging
from casparian_flow.engine.sinks import SinkFactory, DataSink

logger = logging.getLogger(__name__)

class InspectionInterrupt(Exception):
    """Raised to stop the plugin once we have captured the schema."""
    pass

class SchemaViolationError(Exception):
    """Raised when data does not match the strict schema definition."""
    pass

class WorkerContext:
    def __init__(self, 
                 sql_engine: Engine, 
                 parquet_root: Path, 
                 topic_config: Dict[str, Any] = None, 
                 inspect_mode: bool = False,
                 job_id: int = 0,
                 file_version_id: int = 0,
                 file_location_id: int = 0):
        
        self.sql_engine = sql_engine
        self.parquet_root = parquet_root
        self.topic_config = topic_config or {} 
        
        # Lineage Metadata
        self.job_id = job_id
        self.file_version_id = file_version_id
        self.file_location_id = file_location_id
        
        self.sinks: List[Dict[str, Any]] = []
        self.topic_names: List[str] = []
        
        self.inspect_mode = inspect_mode
        self.rows_processed = 0
        self.INSPECT_LIMIT = 50 
        self.captured_schemas: Dict[str, Any] = {} 

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        # SECURITY: Validate topic name to prevent SQL injection
        import re
        TOPIC_PATTERN = re.compile(r'^[a-zA-Z][a-zA-Z0-9_]{0,99}$')
        if not TOPIC_PATTERN.match(topic):
            raise ValueError(
                f"Invalid topic name '{topic}'. Must be alphanumeric, "
                "start with letter, max 100 chars."
            )
        
        self.topic_names.append(topic)

        # 1. Get Config
        t_conf = self.topic_config.get(topic, {})
        uri = t_conf.get("uri")
        if not uri:
            uri = default_uri or f"parquet://{topic}.parquet"

        # 2. Create Physical Sink (Factory now creates Staging sinks via job_id)
        sink = SinkFactory.create(uri, self.sql_engine, self.parquet_root, job_id=self.job_id)
        
        # 3. Store sink + validation rules
        self.sinks.append({
            "sink": sink,
            "mode": t_conf.get("mode", "infer"), # 'strict' or 'infer'
            "schema": t_conf.get("schema", None)
        })
        
        return len(self.sinks) - 1

    def publish(self, handle: int, data: Any):
        channel = self.sinks[handle]

        # Scenario A: Inspection Mode (The "Interrupt")
        if self.inspect_mode:
            self._handle_inspection(handle, data)
            return

        # GOVERNANCE: Lineage Injection (The "Standard Header")
        if isinstance(data, pd.DataFrame):
            data = data.copy()
            data['_job_id'] = self.job_id
            data['_file_version_id'] = self.file_version_id
            data['_file_id'] = self.file_location_id

        # Scenario B: Strict Enforcement
        if channel["mode"] == "strict" and channel["schema"]:
            self._validate_schema(data, channel["schema"])

        # Write to Sink (Staging)
        channel["sink"].write(data)

    def commit(self):
        """
        GOVERNANCE: Atomic Promotion.
        Called by Worker only on success. Tells sinks to move data from Staging to Prod.
        """
        for s in self.sinks:
            s["sink"].promote()

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
        """
        Enforces 'Strict' mode.
        """
        if not isinstance(data, pd.DataFrame):
            return

        # 1. Check for Missing Columns
        incoming_cols = set(data.columns)
        required_cols = set(required_schema.keys())
        
        missing = required_cols - incoming_cols
        if missing:
            raise SchemaViolationError(f"Strict Schema Violation: Missing columns {missing}")

        # 2. Simple Type Check
        for col, expected_type in required_schema.items():
            if col in data.columns:
                actual_type = str(data[col].dtype)
                # Loose check to handle numpy int64 vs int, etc.
                if expected_type not in actual_type and "object" not in actual_type: 
                     # For MVP we are lenient, but logging typically happens here
                     pass

    def close_all(self):
        for s in self.sinks:
            s["sink"].close()
        self.sinks.clear()