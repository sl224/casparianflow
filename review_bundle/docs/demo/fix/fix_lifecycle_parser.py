"""
FIX demo parser - emits a lightweight fix_order_lifecycle table.

Input format:
  - One FIX message per line.
  - Fields separated by '|' (human-readable) or SOH (\x01).
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional

try:
    import pandas as pd
except ImportError:  # pragma: no cover - handled at runtime
    pd = None

try:
    import pyarrow as pa
except ImportError:  # pragma: no cover - handled at runtime
    pa = None

TOPIC = "fix_order_lifecycle"


def _parse_int(value: Optional[str]) -> Optional[int]:
    if value is None:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def _parse_float(value: Optional[str]) -> Optional[float]:
    if value is None:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def _infer_event(msg_type: Optional[str], exec_type: Optional[str], ord_status: Optional[str]) -> Optional[str]:
    if msg_type == "D":
        return "new_order"
    if msg_type != "8":
        return None
    if exec_type == "0":
        return "ack"
    if exec_type == "1":
        return "partial_fill"
    if exec_type == "2":
        return "fill"
    if exec_type == "4":
        return "cancel"
    if exec_type == "5":
        return "replace"
    if ord_status == "8":
        return "reject"
    return "exec_report"


def _rows_to_table(rows: List[Dict[str, Any]]):
    if pd is not None:
        return pd.DataFrame(rows)
    if pa is not None:
        return pa.Table.from_pylist(rows)
    raise RuntimeError(
        "fix_lifecycle_parser requires pandas or pyarrow. "
        "Install with: python3 -m pip install pandas pyarrow"
    )


def parse(file_path: str):
    rows: List[Dict[str, Any]] = []

    with open(file_path, "r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue

            delimiter = "\x01" if "\x01" in line else "|"
            fields = line.split(delimiter)
            tags: Dict[str, str] = {}

            for field in fields:
                if not field or "=" not in field:
                    continue
                tag, value = field.split("=", 1)
                tags[tag] = value

            if "35" not in tags or "11" not in tags:
                continue

            msg_type = tags.get("35")
            exec_type = tags.get("150")
            ord_status = tags.get("39")

            rows.append(
                {
                    "cl_ord_id": tags.get("11"),
                    "order_id": tags.get("37"),
                    "exec_id": tags.get("17"),
                    "msg_type": msg_type,
                    "exec_type": exec_type,
                    "ord_status": ord_status,
                    "symbol": tags.get("55"),
                    "side": tags.get("54"),
                    "order_qty": _parse_int(tags.get("38")),
                    "price": _parse_float(tags.get("44")),
                    "cum_qty": _parse_int(tags.get("14")),
                    "leaves_qty": _parse_int(tags.get("151")),
                    "last_qty": _parse_int(tags.get("32")),
                    "last_px": _parse_float(tags.get("31")),
                    "sending_time": tags.get("52"),
                    "transact_time": tags.get("60"),
                    "msg_seq_num": _parse_int(tags.get("34")),
                    "sender_comp_id": tags.get("49"),
                    "target_comp_id": tags.get("56"),
                    "lifecycle_event": _infer_event(msg_type, exec_type, ord_status),
                }
            )

    return _rows_to_table(rows)
