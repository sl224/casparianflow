# src/casparian_flow/interface.py
from typing import Any, Protocol, runtime_checkable, Dict, Union

# Forward reference to avoid hard dependency on pandas/arrow here
DataFrameLike = Any 

@runtime_checkable
class CaspContext(Protocol):
    """
    The runtime context for the Plugin.
    Implements the Topic-Based Publish Pattern.
    """
    def register_topic(self, topic: str, default_uri: str = None) -> int:
        """
        Declares that this plugin will publish data to a specific logical topic.
        Returns a handle (integer) to use for publishing.
        """
        ...

    def publish(self, handle: int, data: DataFrameLike):
        """
        Dispatches data to the sink associated with the topic handle.
        """
        ...

@runtime_checkable
class CaspPlugin(Protocol):
    def configure(self, ctx: CaspContext, config: Dict[str, Any]):
        """
        Framework Hook.
        Called by the Worker immediately after instantiation to inject dependencies.
        DO NOT override this unless you are building a custom SDK.
        """
        ...

    def execute(self, file_path: str):
        """
        Run the processing loop.
        """
        ...