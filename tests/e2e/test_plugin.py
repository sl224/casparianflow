#!/usr/bin/env python3
"""
Minimal test plugin for E2E testing.
Reads a CSV file and outputs to Arrow IPC.
"""

import pyarrow as pa
import sys

def process(input_path: str):
    """
    Simple test plugin that creates a test table.
    In real usage, this would read from input_path.
    """
    # Create a simple test table
    schema = pa.schema([
        ('id', pa.int64()),
        ('name', pa.string()),
        ('value', pa.float64())
    ])

    data = {
        'id': [1, 2, 3],
        'name': ['Alice', 'Bob', 'Charlie'],
        'value': [10.5, 20.3, 30.1]
    }

    table = pa.Table.from_pydict(data, schema=schema)

    # Write to stdout as Arrow IPC stream
    writer = pa.ipc.new_stream(sys.stdout.buffer, schema)
    writer.write_table(table)
    writer.close()

    return 0

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: test_plugin.py <input_file>", file=sys.stderr)
        sys.exit(1)

    sys.exit(process(sys.argv[1]))
