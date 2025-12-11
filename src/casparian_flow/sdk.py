# src/casparian_flow/sdk.py
from typing import Dict, Any, Optional


class BasePlugin:
    def __init__(self):
        """
        The User's Constructor.
        The user can override this freely.
        They do NOT need to call super().__init__().
        """
        pass

    def configure(self, ctx: Any, config: Dict[str, Any]):
        """
        The System's Constructor.
        The Worker calls this to inject the infrastructure.
        User code should never touch this.
        """
        self._ctx = ctx
        self._config = config
        self._handles: Dict[str, int] = {}

    def publish(self, topic: str, data: Any):
        # 1. Lifecycle Check: Did the Worker do its job?
        if not hasattr(self, "_ctx") or self._ctx is None:
            raise RuntimeError(
                "Plugin infrastructure not loaded! "
                "Ensure you are running this via the Casparian Engine "
                "and not just 'python my_script.py'."
            )

        # 2. Lazy Registration (The "Handshake")
        if topic not in self._handles:
            self._handles[topic] = self._ctx.register_topic(topic)

        # 3. Dispatch
        self._ctx.publish(self._handles[topic], data)

    @property
    def config(self):
        # Safety check for config access too
        if not hasattr(self, "_config"):
            raise RuntimeError("Config not loaded! Plugin not configured.")
        return self._config
