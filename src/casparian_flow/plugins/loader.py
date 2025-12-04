import importlib.util
import logging
import sys
from pathlib import Path
from typing import Dict, Type, Any
from casparian_flow.interface import CaspPlugin

logger = logging.getLogger(__name__)

class PluginRegistry:
    def __init__(self, plugin_dir: Path):
        self.plugin_dir = plugin_dir
        self._cache: Dict[str, Type[CaspPlugin]] = {}

    def discover(self):
        """
        Loads raw .py files from disk, allowing them to import 
        libraries (Pandas, etc.) that are frozen inside THIS executable.
        """
        if not self.plugin_dir.exists():
            self.plugin_dir.mkdir(parents=True, exist_ok=True)
            return

        # 1. Add plugin dir to sys.path so plugins can import 'utils' relative to themselves
        if str(self.plugin_dir) not in sys.path:
            sys.path.insert(0, str(self.plugin_dir))

        for py_file in self.plugin_dir.glob("*.py"):
            if py_file.name.startswith("_"): continue
            
            module_name = py_file.stem
            try:
                # 2. Dynamic Import Magic
                spec = importlib.util.spec_from_file_location(module_name, py_file)
                if not spec or not spec.loader:
                    continue
                    
                mod = importlib.util.module_from_spec(spec)
                
                # This executes the script. 
                # Since we are inside the Frozen process, 'import pandas' 
                # in the script will find the pandas bundled in our EXE.
                spec.loader.exec_module(mod)
                
                # 3. Convention Check
                if hasattr(mod, "Handler"):
                    self._cache[module_name] = mod.Handler
                    logger.info(f"Loaded External Plugin: {module_name}")
                else:
                    logger.debug(f"Skipping {module_name}: No 'Handler' class found.")
                    
            except Exception as e:
                logger.error(f"Failed to load plugin {module_name}: {e}", exc_info=True)

    def get_plugin(self, name: str) -> Type[CaspPlugin]:
        if name not in self._cache:
            # Auto-reload attempt? Or just fail.
            raise ValueError(f"Plugin '{name}' not found. loaded: {list(self._cache.keys())}")
        return self._cache[name]