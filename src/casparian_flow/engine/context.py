from typing import List, Dict, Any
from pathlib import Path
from sqlalchemy import Engine
from casparian_flow.engine.sinks import MssqlSink, ParquetSink, DataSink

class WorkerContext:
    """
    The runtime context passed to the Plugin.
    Manages the 'Handle Table' (list of sinks).
    """
    def __init__(self, sql_engine: Engine, parquet_root: Path):
        self.sql_engine = sql_engine
        self.parquet_root = parquet_root
        
        # The "Handle Table" - a flat list for O(1) access
        self.sinks: List[DataSink] = []

    def register_sink(self, sink_type: str, target: str, options: Dict[str, Any] = None) -> int:
        opts = options or {}
        sink = None

        if sink_type == "MSSQL":
            sink = MssqlSink(self.sql_engine, table_name=target, options=opts)
        
        elif sink_type == "PARQUET":
            sink = ParquetSink(self.parquet_root, relative_path=target, options=opts)
        
        else:
            raise ValueError(f"Unknown sink type: {sink_type}")

        self.sinks.append(sink)
        # The handle is simply the index in the list
        return len(self.sinks) - 1

    def push(self, handle: int, data: Any):
        """
        Immediate Mode dispatch.
        No dictionary lookups, no string hashing. Just array index access.
        """
        try:
            self.sinks[handle].write(data)
        except IndexError:
            raise ValueError(f"Invalid sink handle: {handle}")

    def close_all(self):
        for s in self.sinks:
            s.close()
        self.sinks.clear()