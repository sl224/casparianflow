# src/casparian_flow/engine/context.py
import logging
from typing import Dict, List, Any, Optional
from sqlalchemy.engine import Engine
from pathlib import Path

from casparian_flow.engine.sinks import SinkFactory

logger = logging.getLogger(__name__)

class InspectionInterrupt(Exception):
    pass

class WorkerContext:
    def __init__(
        self,
        sql_engine: Engine,
        parquet_root: Path,
        topic_config: Dict[str, List[Dict[str, Any]]], # Topic -> List[Configs]
        job_id: int,
        file_version_id: int,
        file_location_id: int = None,
        inspect_mode: bool = False,
    ):
        self.sql_engine = sql_engine
        self.parquet_root = parquet_root
        self.topic_config = topic_config or {}
        self.job_id = job_id
        self.file_version_id = file_version_id
        self.file_location_id = file_location_id
        self.inspect_mode = inspect_mode

        # State
        self.topic_names: List[str] = []
        self.sinks: List[List[Dict[str, Any]]] = [] # Handle -> List[SinkWrappers]
        self.captured_schemas = {}

    def register_topic(self, topic: str, default_uri: str = None) -> int:
        if topic in self.topic_names:
            return self.topic_names.index(topic)

        self.topic_names.append(topic)
        
        # Determine configs (Fan-Out Support)
        configs = self.topic_config.get(topic, [])
        if not configs:
            # Create default config if none exists
            uri = default_uri or f"parquet://{topic}.parquet"
            configs = [{"uri": uri, "mode": "infer"}]
        
        # Instantiate Sinks
        topic_sinks = []
        for conf in configs:
            try:
                sink = SinkFactory.create(
                    conf["uri"], 
                    self.sql_engine, 
                    self.parquet_root, 
                    job_id=self.job_id
                )
                topic_sinks.append({
                    "sink": sink,
                    "mode": conf.get("mode", "infer"),
                    "schema": conf.get("schema", None)
                })
            except Exception as e:
                logger.error(f"Failed to create sink for {topic} ({conf['uri']}): {e}")
                # Continue? Or fail hard? Fail hard ensures data safety.
                raise

        self.sinks.append(topic_sinks)
        return len(self.topic_names) - 1

    def publish(self, handle: int, data: Any):
        if handle >= len(self.sinks):
            raise ValueError(f"Invalid topic handle: {handle}")
        
        sink_group = self.sinks[handle]
        
        # Inject Lineage Columns
        # We assume data is PyArrow Table. Sinks handle conversion.
        if hasattr(data, "append_column"):
            import pyarrow as pa
            # Efficiently append constant columns
            # Note: This might be expensive for every batch. 
            # Ideally done inside Sink or just trusted?
            # For now, let's trust sink or do simple Pandas inject if it's pandas
            pass 

        # Write to all sinks
        for channel in sink_group:
            # (Optional) Schema Validation here
            channel["sink"].write(data)

    def commit(self):
        """Promote all staging sinks to production."""
        for group in self.sinks:
            for channel in group:
                channel["sink"].promote()

    def close_all(self):
        """Close all sinks (release handles)."""
        for group in self.sinks:
            for channel in group:
                try:
                    channel["sink"].close()
                except Exception as e:
                    logger.error(f"Error closing sink: {e}")