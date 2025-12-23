# src/casparian_flow/sdk.py
from typing import Dict, Any, List, Optional
from dataclasses import dataclass, field

@dataclass
class FileEvent:
    """
    The event payload passed to plugins.
    Decouples the plugin from the physical file system logic.
    """
    path: str
    file_id: int = 0
    event_type: str = "file_found"
    metadata: Dict[str, Any] = field(default_factory=dict)

@dataclass
class PluginMetadata:
    """
    Configuration contract for Plugins.
    """
    # File pattern for auto-routing (creates RoutingRule)
    # e.g. "finance/*.csv" or "*.gtest"
    pattern: Optional[str] = None

    # Default output topic name (creates TopicConfig)
    # e.g. "finance_data" or "generalist_out"
    topic: Optional[str] = None

    # Priority for routing rule (higher = processed first)
    priority: int = 50

    # Input Topics: What events does this plugin react to?
    # e.g. ["finance_files", "raw_invoices"]
    subscriptions: List[str] = field(default_factory=list)

    # Output Sinks: Where does data go? (Optional explicit URIs)
    # e.g. {"clean_sales": "sqlite://data.db/sales"}
    sinks: Dict[str, str] = field(default_factory=dict)

    version: str = "1.0.0"
    description: Optional[str] = None

class BasePlugin:
    def __init__(self):
        pass

    def configure(self, ctx: Any, config: Dict[str, Any]):
        """System lifecycle hook."""
        self._ctx = ctx
        self._config = config
        self._handles: Dict[str, int] = {}

    def consume(self, event: FileEvent):
        """
        The new standard entry point for Event-Driven plugins.
        """
        raise NotImplementedError("Plugin must implement consume(self, event)")

    def execute(self, file_path: str):
        """
        Legacy shim for backward compatibility.
        Wraps file_path in a dummy event.
        """
        self.consume(FileEvent(path=file_path))

    def publish(self, topic: str, data: Any):
        """Send data to the Output Sink."""
        if not hasattr(self, "_ctx") or self._ctx is None:
            raise RuntimeError("Plugin infrastructure not loaded!")
        
        if topic not in self._handles:
            self._handles[topic] = self._ctx.register_topic(topic)
            
        self._ctx.publish(self._handles[topic], data)

    @property
    def config(self):
        if not hasattr(self, "_config"):
            raise RuntimeError("Config not loaded! Plugin not configured.")
        return self._config