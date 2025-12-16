
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.DataFrame({"test": [1, 2, 3]})
        self.publish("output", df)
