"""
E2D XML Parser Plugin

Parses E2D XML files and flattens them to tabular format.

PATTERN: *.xml
TOPIC: e2d_xml_data
"""
from casparian_flow.sdk import BasePlugin
import pandas as pd
import pyarrow as pa
import xml.etree.ElementTree as ET


class Handler(BasePlugin):
    """Generic E2D XML parser that flattens XML to table."""

    def execute(self, file_path: str):
        """
        Read E2D XML files and yield as Arrow tables.

        Args:
            file_path: Path to the XML file

        Yields:
            pyarrow.Table with flattened XML data
        """
        try:
            # Parse XML
            tree = ET.parse(file_path)
            root = tree.getroot()

            # Flatten XML to list of dicts
            records = []

            def flatten_element(elem, parent_key=''):
                """Recursively flatten XML element to dict."""
                record = {}

                # Add element text if present
                if elem.text and elem.text.strip():
                    key = f"{parent_key}_{elem.tag}" if parent_key else elem.tag
                    record[key] = elem.text.strip()

                # Add attributes
                for attr_key, attr_value in elem.attrib.items():
                    key = f"{parent_key}_{elem.tag}_{attr_key}" if parent_key else f"{elem.tag}_{attr_key}"
                    record[key] = attr_value

                # Process children
                for child in elem:
                    child_key = f"{parent_key}_{elem.tag}" if parent_key else elem.tag
                    child_record = flatten_element(child, child_key)
                    record.update(child_record)

                return record

            # Process all top-level children
            for child in root:
                record = flatten_element(child)
                if record:  # Only add non-empty records
                    records.append(record)

            # If no child records, flatten the root itself
            if not records:
                record = flatten_element(root)
                if record:
                    records.append(record)

            # Convert to DataFrame
            if records:
                df = pd.DataFrame(records)

                # Convert to Arrow table
                table = pa.Table.from_pandas(df)

                yield table

        except Exception as e:
            # Log error but don't crash
            print(f"Error processing {file_path}: {e}")
            raise
