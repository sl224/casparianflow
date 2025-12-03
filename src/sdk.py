# src/casparian_sdk/base.py
from typing import Dict, Any, Optional
# from casparian_sdk.interface import CaspContext (assuming split package)

class BasePlugin:
    def __init__(self):
        # User Zone: They can put whatever they want here.
        # It runs FIRST.
        pass

    def configure(self, ctx: Any, config: Dict[str, Any]):
        # System Zone: The Worker calls this. 
        # It runs SECOND.
        # We mark it final/internal by convention (users shouldn't override this).
        self._ctx = ctx
        self._config = config
        self._handles: Dict[str, int] = {} 

    def publish(self, topic: str, data: Any):
        # Lazy check to ensure they didn't break the lifecycle
        if not hasattr(self, '_ctx') or self._ctx is None:
             raise RuntimeError("Plugin not configured! Did the Worker call .configure()?")
        
        if topic not in self._handles:
            self._handles[topic] = self._ctx.register_topic(topic)
            
        # 2. Dispatch
        handle = self._handles[topic]
        self._ctx.publish(handle, data)

    @property
    def config(self):
        return self._config