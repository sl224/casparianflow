from casparian_flow.interface import  CaspContext
from casparian_flow.sdk import BasePlugin
from typing import Dict, Any
import pandas as pd

# The loader looks for this exact name:
class TestPlugin(BasePlugin): 
    def execute(self, file_path: str):
        self.publish('test', pd.DataFrame([1, 2, 34]))