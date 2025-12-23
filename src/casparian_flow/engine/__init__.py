# src/casparian_flow/engine/__init__.py
"""
Casparian Flow Engine Module.

v5.0 Bridge Mode additions:
- VenvManager: Isolated virtual environment management
- BridgeExecutor: Subprocess execution with Arrow IPC
- BridgeShim: Guest process for isolated plugin execution
"""

from casparian_flow.engine.venv_manager import VenvManager, get_venv_manager, VenvManagerError
from casparian_flow.engine.bridge import BridgeExecutor, BridgeError, execute_bridge_job

__all__ = [
    # VenvManager
    "VenvManager",
    "get_venv_manager",
    "VenvManagerError",
    # Bridge
    "BridgeExecutor",
    "BridgeError",
    "execute_bridge_job",
]
