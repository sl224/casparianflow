"""
Text Processor Plugin
Processes text files and extracts metadata.
"""
from pathlib import Path
from typing import Any, Dict
import pandas as pd

# Plugin metadata
MANIFEST = {
    "name": "text_processor",
    "version": "1.0.0",
    "subscriptions": ["txt", "doc", "report"],
    "sinks": {
        "output": {
            "uri": "parquet://data/output/text_processed.parquet",
            "mode": "append"
        }
    }
}


class Plugin:
    """Processes text files."""

    def __init__(self, config: Dict[str, Any]):
        """Initialize the plugin."""
        self.config = config
        print(f"[TextProcessor] Initialized")

    def consume(self, file_path: Path, context) -> Dict[str, Any]:
        """
        Process a text file.

        Args:
            file_path: Path to the input text file
            context: Execution context

        Returns:
            Processing results
        """
        print(f"[TextProcessor] Processing: {file_path.name}")

        try:
            # Read text file
            content = file_path.read_text()
            lines = content.split('\n')
            word_count = len(content.split())
            char_count = len(content)

            print(f"[TextProcessor] Lines: {len(lines)}, Words: {word_count}, Chars: {char_count}")

            # Create metadata DataFrame
            df = pd.DataFrame({
                'filename': [file_path.name],
                'lines': [len(lines)],
                'words': [word_count],
                'characters': [char_count],
                'preview': [content[:100] + '...' if len(content) > 100 else content]
            })

            # Publish to output
            output_handle = context.register_topic("output")
            context.publish(output_handle, df)

            print(f"[TextProcessor] Published metadata")

            return {
                "status": "success",
                "lines": len(lines),
                "words": word_count,
                "file": file_path.name
            }

        except Exception as e:
            print(f"[TextProcessor] Error: {e}")
            return {"status": "error", "error": str(e)}
