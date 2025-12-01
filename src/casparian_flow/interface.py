from typing import Any, Protocol, runtime_checkable, Dict
from enum import StrEnum

class SinkType(StrEnum):
    MSSQL = "MSSQL"
    PARQUET = "PARQUET"
    ICEBERG = "ICEBERG"
    BLOB = "BLOB"

@runtime_checkable
class HostContext(Protocol):
    """
    The 'pointer' the Plugin uses to talk to the engine.
    Implements the Immediate Mode pattern.
    """
    def register_sink(self, sink_type: str, target: str, options: Dict[str, Any] = None) -> int:
        """
        Call this ONCE in init().
        
        Args:
            sink_type: "MSSQL", "PARQUET", etc.
            target: The destination name (Table Name or File Path).
            options: Configuration (e.g. {"mode": "append", "schema": "analytics"}).
            
        Returns:
            int: A handle (index) to use in the push() loop.
        """
        ...

    def push(self, handle: int, data: Any):
        """
        The Hot Loop call.
        Dispatches data directly to the pre-resolved sink.
        """
        ...

@runtime_checkable
class CasparianPlugin(Protocol):
    """
    The implementation contract for your Logic.
    """
    def init(self, ctx: HostContext, config: Dict[str, Any]):
        """
        Pre-calculate everything here. Resolve handles.
        """
        ...

    def execute(self, file_path: str):
        """
        Run the processing loop. 
        MUST NOT allocate complex objects or do string lookups here.
        """
        ...