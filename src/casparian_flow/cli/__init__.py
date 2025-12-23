# src/casparian_flow/cli/__init__.py
"""
Casparian Flow CLI commands.

v5.0 Bridge Mode additions:
- publish: Deploy artifacts to the registry
"""

from casparian_flow.cli.publish import publish_plugin, main as publish_main

__all__ = ["publish_plugin", "publish_main"]
