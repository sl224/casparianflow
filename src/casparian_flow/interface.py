# src/casparian_flow/interface.py
from typing import Any, Protocol, runtime_checkable, Dict, Union

# We use Any for data to avoid hard dependencies on Pandas/Arrow in the interface definition
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
        
        Args:
            topic: The logical name of the stream (e.g., "sales", "logs").
            default_uri: A fallback destination if the UI/Config doesn't provide one.
                         (e.g., "parquet://scans/sales.parquet")
                         
        Returns:
            int: A handle (index) to use in the publish() loop.
        """
        ...

    def publish(self, handle: int, data: DataFrameLike):
        """
        Dispatches data to the sink associated with the topic handle.
        """
        ...

@runtime_checkable
class CaspPlugin(Protocol):
    def init(self, ctx: CaspContext, config: Dict[str, Any]):
        """
        Register your topics here.
        Example:
            self.sales_h = ctx.register_topic("sales")
        """
        ...

    def execute(self, file_path: str):
        """
        Run the processing loop.
        Example:
            df = pd.read_csv(file_path)
            ctx.publish(self.sales_h, df)
        """
        ...