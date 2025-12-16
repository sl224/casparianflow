
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        # Read CSV file and publish to configured topic
        df = pd.read_csv(file_path)
        self.publish("test", df)
