"""Minimal parser for testing pipelines (no banned imports)."""

from __future__ import annotations

import pyarrow as pa

TOPIC = "raw_lines"
name = "raw_lines_parser"
version = "0.1.0"
topics = ["fix"]


def parse(file_path: str) -> pa.Table:
    line_numbers = []
    lines = []

    with open(file_path, "r", encoding="utf-8", errors="replace") as handle:
        for idx, line in enumerate(handle, 1):
            line_numbers.append(idx)
            lines.append(line.rstrip("\n"))

    return pa.table(
        {
            "line_number": pa.array(line_numbers, type=pa.int64()),
            "raw": pa.array(lines, type=pa.string()),
        }
    )
