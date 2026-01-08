#!/usr/bin/env python3
"""
Casparian Run Shim - ZMQ-based parser execution.

This script is spawned by `casparian run` and communicates via ZMQ PUSH socket.
It loads a parser Python file, executes it, and streams Arrow IPC batches back.

Protocol:
    1. Send schema message: {"type": "schema", "outputs": {...}}
    2. Send batch messages: {"type": "batch", "sink": "name", "data": "<base64 Arrow IPC>"}
    3. Send done message: {"type": "done", "stats": {...}}
    On error: {"type": "error", "message": "...", "source_line": N}

Usage:
    python -m casparian_worker.run_shim <parser.py> <input.csv> \
        --zmq-endpoint tcp://127.0.0.1:12345 \
        --source-hash abc123 \
        --job-id uuid-string

Security:
    - Runs in isolated venv
    - No access to credentials
    - Timeout on ZMQ send to prevent hanging
"""

import argparse
import base64
import importlib.util
import json
import os
import sys
import traceback
from io import BytesIO
from pathlib import Path
from typing import Any, Dict, Iterator, List, Optional, Union

# Configure logging to stderr (keeps stdout clean for structured output)
import logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [RUN_SHIM] %(levelname)s: %(message)s",
    stream=sys.stderr,
)
logger = logging.getLogger(__name__)


def setup_zmq_socket(endpoint: str):
    """
    Create and connect a ZMQ PUSH socket.

    Sets SNDTIMEO and LINGER to prevent hanging if parent dies:
    - SNDTIMEO: 5 second timeout on send (raises zmq.Again if exceeded)
    - LINGER: 1 second max wait on close for pending messages
    - SNDHWM: 100 message high water mark for backpressure
    """
    import zmq

    context = zmq.Context()
    socket = context.socket(zmq.PUSH)

    # Prevent hanging if parent crashes
    socket.setsockopt(zmq.SNDTIMEO, 5000)  # 5 second send timeout
    socket.setsockopt(zmq.LINGER, 1000)    # 1 second linger on close
    socket.setsockopt(zmq.SNDHWM, 100)     # High water mark

    socket.connect(endpoint)
    logger.info(f"Connected to ZMQ endpoint: {endpoint}")

    return socket, context


def send_message(socket, message: dict):
    """Send a JSON message over ZMQ."""
    import zmq

    try:
        msg_bytes = json.dumps(message).encode('utf-8')
        socket.send(msg_bytes)
    except zmq.Again:
        logger.error("ZMQ send timeout - parent may have crashed")
        sys.exit(1)


def send_schema(socket, name: str, version: str, topics: List[str], outputs: Dict[str, dict]):
    """Send schema declaration message with parser metadata."""
    send_message(socket, {
        "type": "schema",
        "name": name,
        "version": version,
        "topics": topics,
        "outputs": outputs,
    })


def send_batch(socket, sink: str, data: bytes):
    """Send an Arrow IPC batch as base64."""
    send_message(socket, {
        "type": "batch",
        "sink": sink,
        "data": base64.b64encode(data).decode('ascii'),
    })


def send_done(socket, stats: Dict[str, int]):
    """Send completion message."""
    send_message(socket, {
        "type": "done",
        "stats": stats,
    })


def send_error(socket, message: str, source_line: Optional[int] = None):
    """Send error message."""
    msg = {
        "type": "error",
        "message": message,
    }
    if source_line is not None:
        msg["source_line"] = source_line
    send_message(socket, msg)


def serialize_to_arrow_ipc(data) -> bytes:
    """
    Serialize data to Arrow IPC format.

    Supports:
    - pyarrow Table
    - pyarrow RecordBatch
    - pandas DataFrame
    - polars DataFrame (if available)
    """
    import pyarrow as pa

    # Convert to Arrow Table if needed
    if hasattr(data, "to_arrow") and callable(data.to_arrow):
        # polars DataFrame
        table = data.to_arrow()
    elif hasattr(data, "__class__") and data.__class__.__name__ == "DataFrame":
        # pandas DataFrame
        import pandas as pd
        if isinstance(data, pd.DataFrame):
            table = pa.Table.from_pandas(data)
        else:
            raise TypeError(f"Unknown DataFrame type: {type(data)}")
    elif isinstance(data, pa.Table):
        table = data
    elif isinstance(data, pa.RecordBatch):
        table = pa.Table.from_batches([data])
    else:
        raise TypeError(f"Cannot serialize to Arrow: {type(data)}")

    # Serialize to IPC stream
    sink = BytesIO()
    with pa.ipc.new_stream(sink, table.schema) as writer:
        for batch in table.to_batches():
            writer.write_batch(batch)

    return sink.getvalue()


def schema_to_json(schema) -> dict:
    """Convert Arrow schema to JSON-serializable dict."""
    import pyarrow as pa

    fields = []
    for field in schema:
        fields.append({
            "name": field.name,
            "type": str(field.type),
            "nullable": field.nullable,
        })
    return {"fields": fields}


def load_parser(parser_path: Path) -> Any:
    """
    Load a parser Python file dynamically.

    The parser file should define a Parser subclass or a parse() function.

    Adds the parser's directory to sys.path so imports work.
    """
    # Add parser's directory to sys.path for relative imports
    parser_dir = str(parser_path.parent.resolve())
    if parser_dir not in sys.path:
        sys.path.insert(0, parser_dir)

    # Load module from file
    spec = importlib.util.spec_from_file_location("parser_module", parser_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load parser: {parser_path}")

    module = importlib.util.module_from_spec(spec)
    sys.modules["parser_module"] = module
    spec.loader.exec_module(module)

    return module


def find_parser_class(module) -> Optional[type]:
    """Find a Parser subclass in the module."""
    for name in dir(module):
        obj = getattr(module, name)
        if isinstance(obj, type) and name != "Parser":
            # Check if it has 'outputs' attribute (our Parser contract)
            if hasattr(obj, "outputs"):
                return obj
    return None


def run_parser(
    parser_path: Path,
    input_path: Path,
    source_hash: str,
    job_id: str,
    socket,
) -> Dict[str, int]:
    """
    Execute the parser and stream results over ZMQ.

    Returns stats dict with record counts per sink.
    """
    import pyarrow as pa

    # Load the parser module
    logger.info(f"Loading parser: {parser_path}")
    module = load_parser(parser_path)

    # Find parser class or parse function
    parser_class = find_parser_class(module)
    parse_func = getattr(module, "parse", None)

    if parser_class is None and parse_func is None:
        raise ValueError(
            f"Parser must define either:\n"
            f"  1. A class with 'outputs' attribute and 'parse' method\n"
            f"  2. A top-level 'parse()' function\n"
            f"Found neither in {parser_path}"
        )

    # Extract schema information
    outputs_info = {}
    stats = {}

    if parser_class is not None:
        # Class-based parser
        logger.info(f"Found parser class: {parser_class.__name__}")

        # Extract required metadata
        parser_name = getattr(parser_class, "name", None)
        parser_version = getattr(parser_class, "version", None)
        parser_topics = getattr(parser_class, "topics", None)

        # Validate required fields
        if parser_name is None:
            raise ValueError(
                f"Parser class '{parser_class.__name__}' must define 'name' attribute.\n"
                f"Example: name = 'my_parser'"
            )
        if parser_version is None:
            raise ValueError(
                f"Parser class '{parser_class.__name__}' must define 'version' attribute.\n"
                f"Example: version = '1.0.0'"
            )
        if parser_topics is None:
            raise ValueError(
                f"Parser class '{parser_class.__name__}' must define 'topics' attribute.\n"
                f"Example: topics = ['sales_data']"
            )
        if not isinstance(parser_topics, (list, tuple)):
            raise ValueError(
                f"Parser 'topics' must be a list, got: {type(parser_topics).__name__}"
            )
        if len(parser_topics) == 0:
            raise ValueError(
                f"Parser 'topics' must contain at least one topic.\n"
                f"Example: topics = ['sales_data']"
            )

        logger.info(f"Parser: {parser_name} v{parser_version}, topics: {parser_topics}")

        # Get outputs declaration
        outputs = getattr(parser_class, "outputs", {})

        for name, output in outputs.items():
            if hasattr(output, "schema"):
                # Output object with schema attribute
                schema = output.schema
                sink = getattr(output, "sink", None)
            elif isinstance(output, pa.Schema):
                # Direct Arrow schema
                schema = output
                sink = None
            else:
                raise ValueError(f"Output '{name}' must have a 'schema' attribute or be a pyarrow.Schema")

            outputs_info[name] = {
                "schema": schema_to_json(schema),
                "sink": sink,
            }
            stats[name] = 0

        # Send schema with parser metadata
        send_schema(socket, parser_name, parser_version, list(parser_topics), outputs_info)

        # Instantiate parser and run
        parser_instance = parser_class()

        # Create context-like object for the parser
        class ParseContext:
            def __init__(self, input_path, source_hash, job_id):
                self.input_path = input_path
                self.source_hash = source_hash
                self.job_id = job_id
                self._current_line = 0

            @property
            def current_line(self):
                return self._current_line

            def iter_csv(self):
                """Iterate over CSV rows with line tracking."""
                import csv
                with open(self.input_path, newline='', encoding='utf-8') as f:
                    reader = csv.DictReader(f)
                    self._current_line = 1  # Header
                    for row in reader:
                        self._current_line += 1
                        yield row

        ctx = ParseContext(input_path, source_hash, job_id)

        # Check for parse method
        if hasattr(parser_instance, "parse"):
            results = parser_instance.parse(ctx)
        else:
            raise ValueError(f"Parser class must have a 'parse' method")

        # Process results
        if results is not None:
            for record in results:
                # Parser yields tuples: (sink_name, data)
                if not isinstance(record, tuple) or len(record) != 2:
                    raise ValueError(
                        f"Parser must yield (sink_name, data) tuples, got: {type(record)}\n"
                        f"Example: yield ('orders', df)"
                    )
                sink_name, data = record

                # Serialize and send batch
                ipc_data = serialize_to_arrow_ipc(data)
                send_batch(socket, sink_name, ipc_data)

                # Update stats
                if hasattr(data, "num_rows"):
                    stats[sink_name] = stats.get(sink_name, 0) + data.num_rows
                elif hasattr(data, "__len__"):
                    stats[sink_name] = stats.get(sink_name, 0) + len(data)
                else:
                    stats[sink_name] = stats.get(sink_name, 0) + 1

    else:
        # Function-based parser - deprecated, require class-based
        raise ValueError(
            f"Function-based parsers are not supported.\n"
            f"Please use a class-based parser with required attributes:\n\n"
            f"class MyParser:\n"
            f"    name = 'my_parser'\n"
            f"    version = '1.0.0'\n"
            f"    topics = ['my_topic']\n"
            f"    outputs = {{'default': pa.schema([...])}}\n\n"
            f"    def parse(self, ctx):\n"
            f"        yield ('default', dataframe)\n"
        )

    return stats


def main():
    parser = argparse.ArgumentParser(
        description="Casparian Run Shim - Execute parser and stream results via ZMQ"
    )
    parser.add_argument("parser_path", type=Path, help="Path to parser Python file")
    parser.add_argument("input_path", type=Path, help="Path to input file")
    parser.add_argument("--zmq-endpoint", required=True, help="ZMQ endpoint to connect to")
    parser.add_argument("--source-hash", required=True, help="Blake3 hash of input file")
    parser.add_argument("--job-id", required=True, help="Job ID for this run")

    args = parser.parse_args()

    # Validate paths
    if not args.parser_path.exists():
        logger.error(f"Parser file not found: {args.parser_path}")
        sys.exit(1)

    if not args.input_path.exists():
        logger.error(f"Input file not found: {args.input_path}")
        sys.exit(1)

    # Setup ZMQ
    socket = None
    context = None

    try:
        socket, context = setup_zmq_socket(args.zmq_endpoint)

        # Run parser
        stats = run_parser(
            parser_path=args.parser_path,
            input_path=args.input_path,
            source_hash=args.source_hash,
            job_id=args.job_id,
            socket=socket,
        )

        # Send completion
        send_done(socket, stats)

        logger.info(f"Parser completed successfully: {stats}")
        sys.exit(0)

    except Exception as e:
        error_msg = traceback.format_exc()
        logger.error(f"Parser failed: {e}\n{error_msg}")

        if socket is not None:
            try:
                send_error(socket, str(e))
            except Exception:
                pass  # Best effort

        sys.exit(1)

    finally:
        if socket is not None:
            socket.close()
        if context is not None:
            context.term()


if __name__ == "__main__":
    main()
