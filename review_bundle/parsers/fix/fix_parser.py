"""
FIX Protocol Parser v1.2

First-class FIX parser for Casparian Flow. Produces up to four outputs:
- fix_order_lifecycle: Order messages (D, 8, F, G, 9, H)
- fix_session_events: Session messages (A, 0, 1, 2, 3, 4, 5, j)
- fix_tags: Raw tag-value pairs (optional, when FIX_TAGS_ALLOWLIST is set)
- fix_parse_errors: Parse errors for malformed lines

Environment Variables:
- FIX_TZ: Required. Timezone for timestamp parsing (e.g., UTC, America/New_York)
- FIX_TAGS_ALLOWLIST: Optional. Comma-separated numeric tags to capture

Features:
- DECIMAL(38,10) for all numeric fields (qty, price, avg_px, etc.) with rounding logged
- Log prefix handling: finds 8=FIX anywhere in line
- Multiple FIX messages per line (split by begin_string)
- SOH (\x01) and pipe (|) delimiter support
- source_fingerprint for file identity/lineage
- message_index for multi-message line disambiguation
"""

from __future__ import annotations

import hashlib
import os
from datetime import datetime
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from typing import Any, Dict, List, Optional, Set, Tuple

try:
    import pyarrow as pa
except ImportError:
    pa = None

from zoneinfo import ZoneInfo  # Python 3.9+ required


# Parser metadata
TOPIC = "fix_order_lifecycle"  # Primary topic for routing
name = "fix_parser"
version = "1.2.0"
topics = ["fix"]

# Message type routing
ORDER_MSG_TYPES = {"D", "8", "F", "G", "9", "H"}
SESSION_MSG_TYPES = {"A", "0", "1", "2", "3", "4", "5", "j"}

# FIX timestamp formats
FIX_TIMESTAMP_PATTERNS = [
    "%Y%m%d-%H:%M:%S.%f",  # 20240102-09:30:00.000
    "%Y%m%d-%H:%M:%S",     # 20240102-09:30:00
]

DECIMAL_SCALE = 10
DECIMAL_PRECISION = 38
DECIMAL_QUANT = Decimal("1").scaleb(-DECIMAL_SCALE)


def _get_timezone() -> ZoneInfo:
    """Get timezone from FIX_TZ environment variable. Fails if not set."""
    tz_str = os.environ.get("FIX_TZ", "").strip()
    if not tz_str:
        raise ValueError(
            "FIX_TZ environment variable is required. "
            "Set it to a valid timezone (e.g., FIX_TZ=UTC)"
        )
    try:
        return ZoneInfo(tz_str)
    except Exception as e:
        raise ValueError(f"Invalid timezone '{tz_str}': {e}")


def _get_tags_allowlist() -> Optional[Set[str]]:
    """Get optional tags allowlist from FIX_TAGS_ALLOWLIST environment variable."""
    allowlist_str = os.environ.get("FIX_TAGS_ALLOWLIST", "").strip()
    if not allowlist_str:
        return None
    tags = set()
    for tag in allowlist_str.split(","):
        tag = tag.strip()
        if tag and tag.isdigit():
            tags.add(tag)
    return tags if tags else None


def _compute_line_hash(line: bytes) -> str:
    """Compute SHA256 hash of raw line bytes (excluding trailing newline)."""
    return hashlib.sha256(line.rstrip(b"\n\r")).hexdigest()


def _compute_source_fingerprint(file_path: str) -> str:
    """Compute source fingerprint from file size and mtime for lineage."""
    try:
        st = os.stat(file_path)
        data = f"{st.st_size}:{st.st_mtime_ns}"
        return hashlib.sha256(data.encode()).hexdigest()[:16]
    except OSError:
        return "unknown"


def _parse_int(value: Optional[str]) -> Optional[int]:
    """Parse string to integer."""
    if value is None:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def _parse_decimal(value: Optional[str], field_name: str) -> Tuple[Optional[Decimal], Optional[str]]:
    """Parse string to Decimal with scale enforcement. Returns (value, warning)."""
    if value is None:
        return None, None
    try:
        dec = Decimal(value)
    except (InvalidOperation, ValueError):
        return None, f"Invalid decimal: field={field_name} value={value}"

    if not dec.is_finite():
        return None, f"Invalid decimal: field={field_name} value={value}"

    try:
        quantized = dec.quantize(DECIMAL_QUANT, rounding=ROUND_HALF_UP)
    except (InvalidOperation, ValueError):
        return None, f"Invalid decimal: field={field_name} value={value}"

    if len(quantized.as_tuple().digits) > DECIMAL_PRECISION:
        return None, f"Decimal overflow: field={field_name} value={value}"

    if quantized != dec:
        return quantized, (
            f"Decimal rounded: field={field_name} value={value} rounded={quantized}"
        )

    return quantized, None


def _parse_timestamp(value: Optional[str], tz: ZoneInfo) -> Optional[datetime]:
    """Parse FIX timestamp to timezone-aware datetime."""
    if value is None:
        return None
    for pattern in FIX_TIMESTAMP_PATTERNS:
        try:
            dt = datetime.strptime(value, pattern)
            return dt.replace(tzinfo=tz)
        except ValueError:
            continue
    return None


def _detect_delimiter(line: str) -> str:
    """Detect FIX field delimiter from message line."""
    if "\x01" in line:
        return "\x01"
    if "|" in line:
        return "|"
    if "^A" in line:
        return "^A"
    # Try space-delimited (rare)
    if " " in line and "=" in line:
        # Check if it looks like tag=value pairs
        parts = line.split(" ")
        if all("=" in p for p in parts[:3] if p):
            return " "
    return "|"  # Default fallback


def _find_fix_start(line: str) -> int:
    """Find the start of FIX message (8=FIX) in line with possible prefix."""
    return line.find("8=FIX")


def _split_multi_message_line(line: str) -> List[str]:
    """Split a line that may contain multiple FIX messages.

    Only splits on "8=FIX" pattern to avoid false positives with tags like 38=, 48=, etc.
    """
    messages = []
    remaining = line

    while remaining:
        start = _find_fix_start(remaining)
        if start < 0:
            break

        # Find start of next message (if any) - only split on 8=FIX to avoid
        # false positives with tags like 38=, 48=, 58=, 108=, 148=, etc.
        next_start = remaining.find("8=FIX", start + 5)
        if next_start < 0:
            # No more messages, take the rest
            messages.append(remaining[start:])
            break

        # Extract this message
        messages.append(remaining[start:next_start])
        remaining = remaining[next_start:]

    return messages


def _parse_fix_message(line: str) -> Tuple[Dict[str, str], Optional[str]]:
    """
    Parse FIX message line into tag-value dictionary.

    Returns:
        Tuple of (tags dict, error message or None)
    """
    # Handle log prefix - find start of FIX message
    start_idx = _find_fix_start(line)
    if start_idx < 0:
        return {}, "No FIX message found (missing 8=FIX)"

    fix_part = line[start_idx:]
    delimiter = _detect_delimiter(fix_part)
    fields = fix_part.split(delimiter)
    tags: Dict[str, str] = {}

    for field in fields:
        if not field or "=" not in field:
            continue
        tag, _, value = field.partition("=")
        tag = tag.strip()
        if tag:
            tags[tag] = value

    if not tags.get("35"):
        return tags, "Missing msg_type (tag 35)"

    return tags, None


def _infer_lifecycle_event(
    msg_type: Optional[str],
    exec_type: Optional[str],
    ord_status: Optional[str],
) -> Optional[str]:
    """Derive lifecycle_event from message type and execution fields."""
    if msg_type == "D":
        return "new_order"
    if msg_type == "F":
        return "cancel_request"
    if msg_type == "G":
        return "replace_request"
    if msg_type == "9":
        return "cancel_reject"
    if msg_type == "H":
        return "status_request"
    if msg_type != "8":
        return None

    # Execution report - determine event from exec_type/ord_status
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
    if exec_type == "8" or ord_status == "8":
        return "reject"

    return "exec_report"


def _build_order_row(
    tags: Dict[str, str],
    source_path: str,
    line_number: int,
    message_index: int,
    raw_line_hash: str,
    source_fingerprint: str,
    tz: ZoneInfo,
) -> Tuple[Dict[str, Any], List[str]]:
    """Build fix_order_lifecycle row from parsed tags."""
    msg_type = tags.get("35")
    exec_type = tags.get("150")
    ord_status = tags.get("39")

    warnings: List[str] = []
    order_qty, warn = _parse_decimal(tags.get("38"), "order_qty")
    if warn:
        warnings.append(warn)
    price, warn = _parse_decimal(tags.get("44"), "price")
    if warn:
        warnings.append(warn)
    cum_qty, warn = _parse_decimal(tags.get("14"), "cum_qty")
    if warn:
        warnings.append(warn)
    leaves_qty, warn = _parse_decimal(tags.get("151"), "leaves_qty")
    if warn:
        warnings.append(warn)
    last_qty, warn = _parse_decimal(tags.get("32"), "last_qty")
    if warn:
        warnings.append(warn)
    last_px, warn = _parse_decimal(tags.get("31"), "last_px")
    if warn:
        warnings.append(warn)
    avg_px, warn = _parse_decimal(tags.get("6"), "avg_px")
    if warn:
        warnings.append(warn)

    row = {
        # Context columns
        "source_path": source_path,
        "line_number": line_number,
        "message_index": message_index,
        "raw_line_hash": raw_line_hash,
        "source_fingerprint": source_fingerprint,
        "__cf_row_id": line_number,

        # Header fields
        "begin_string": tags.get("8"),
        "body_length": _parse_int(tags.get("9")),
        "msg_type": msg_type,
        "msg_seq_num": _parse_int(tags.get("34")),
        "sending_time": _parse_timestamp(tags.get("52"), tz),
        "sender_comp_id": tags.get("49"),
        "target_comp_id": tags.get("56"),
        "poss_dup_flag": tags.get("43"),
        "orig_sending_time": _parse_timestamp(tags.get("122"), tz),
        "sender_sub_id": tags.get("50"),
        "target_sub_id": tags.get("57"),
        "sender_location_id": tags.get("142"),
        "target_location_id": tags.get("143"),

        # Order identification
        "cl_ord_id": tags.get("11"),
        "orig_cl_ord_id": tags.get("41"),
        "order_id": tags.get("37"),
        "exec_id": tags.get("17"),
        "secondary_cl_ord_id": tags.get("526"),
        "cl_ord_link_id": tags.get("583"),

        # Instrument
        "symbol": tags.get("55"),
        "side": tags.get("54"),
        "security_id": tags.get("48"),
        "security_id_source": tags.get("22"),
        "currency": tags.get("15"),

        # Order parameters - DECIMAL types
        "ord_type": tags.get("40"),
        "time_in_force": tags.get("59"),
        "order_qty": order_qty,
        "price": price,
        "transact_time": _parse_timestamp(tags.get("60"), tz),

        # Execution fields - DECIMAL types
        "exec_type": exec_type,
        "ord_status": ord_status,
        "cum_qty": cum_qty,
        "leaves_qty": leaves_qty,
        "last_qty": last_qty,
        "last_px": last_px,
        "avg_px": avg_px,

        # Derived
        "lifecycle_event": _infer_lifecycle_event(msg_type, exec_type, ord_status),

        # Account
        "account": tags.get("1"),
        "acct_id_source": tags.get("660"),
        "account_type": tags.get("581"),

        # Reject/diagnostic
        "text": tags.get("58"),
        "ord_rej_reason": tags.get("103"),
        "cxl_rej_reason": tags.get("102"),
        "business_reject_reason": tags.get("380"),
        "ref_msg_type": tags.get("372"),
        "ref_seq_num": _parse_int(tags.get("45")),
        "session_reject_reason": tags.get("373"),
    }

    return row, warnings


def _build_session_row(
    tags: Dict[str, str],
    source_path: str,
    line_number: int,
    message_index: int,
    raw_line_hash: str,
    source_fingerprint: str,
    tz: ZoneInfo,
) -> Dict[str, Any]:
    """Build fix_session_events row from parsed tags."""
    return {
        # Context columns
        "source_path": source_path,
        "line_number": line_number,
        "message_index": message_index,
        "raw_line_hash": raw_line_hash,
        "source_fingerprint": source_fingerprint,
        "__cf_row_id": line_number,

        # Header fields
        "begin_string": tags.get("8"),
        "body_length": _parse_int(tags.get("9")),
        "msg_type": tags.get("35"),
        "msg_seq_num": _parse_int(tags.get("34")),
        "sending_time": _parse_timestamp(tags.get("52"), tz),
        "sender_comp_id": tags.get("49"),
        "target_comp_id": tags.get("56"),
        "poss_dup_flag": tags.get("43"),
        "orig_sending_time": _parse_timestamp(tags.get("122"), tz),
        "sender_sub_id": tags.get("50"),
        "target_sub_id": tags.get("57"),

        # Reference/diagnostic
        "ref_msg_type": tags.get("372"),
        "ref_seq_num": _parse_int(tags.get("45")),
        "business_reject_ref_id": tags.get("379"),
        "business_reject_reason": tags.get("380"),
        "session_reject_reason": tags.get("373"),
        "text": tags.get("58"),

        # Context linkage
        "cl_ord_id": tags.get("11"),
        "order_id": tags.get("37"),
        "exec_id": tags.get("17"),
        "account": tags.get("1"),
        "symbol": tags.get("55"),
        "side": tags.get("54"),
    }


def _build_tag_rows(
    tags: Dict[str, str],
    source_path: str,
    line_number: int,
    message_index: int,
    raw_line_hash: str,
    source_fingerprint: str,
    allowlist: Set[str],
    raw_line: str,
) -> List[Dict[str, Any]]:
    """Build fix_tags rows for tags in the allowlist."""
    rows = []
    msg_type = tags.get("35")

    # Parse line to get tag positions
    start_idx = _find_fix_start(raw_line)
    if start_idx < 0:
        return rows

    fix_part = raw_line[start_idx:]
    delimiter = _detect_delimiter(fix_part)
    fields = fix_part.split(delimiter)

    position = 0
    for field in fields:
        if not field or "=" not in field:
            continue
        position += 1
        tag, _, value = field.partition("=")
        tag = tag.strip()

        if tag in allowlist:
            rows.append({
                "source_path": source_path,
                "line_number": line_number,
                "message_index": message_index,
                "raw_line_hash": raw_line_hash,
                "source_fingerprint": source_fingerprint,
                "msg_type": msg_type,
                "tag": tag,
                "value": value,
                "position": position,
            })

    return rows


def _build_error_row(
    source_path: str,
    line_number: int,
    message_index: Optional[int],
    raw_line_hash: str,
    source_fingerprint: str,
    error_reason: str,
    raw_line_preview: str,
) -> Dict[str, Any]:
    """Build fix_parse_errors row."""
    return {
        "source_path": source_path,
        "line_number": line_number,
        "message_index": message_index,
        "raw_line_hash": raw_line_hash,
        "source_fingerprint": source_fingerprint,
        "error_reason": error_reason,
        "raw_line_preview": raw_line_preview[:500],  # Truncate long lines
    }


def _rows_to_table(rows: List[Dict[str, Any]], schema: pa.Schema) -> pa.Table:
    """Convert list of row dicts to PyArrow table with explicit schema."""
    if not rows:
        return pa.Table.from_pylist([], schema=schema)
    return pa.Table.from_pylist(rows, schema=schema)


def _get_order_schema(tz_str: str) -> pa.Schema:
    """Get PyArrow schema for fix_order_lifecycle with DECIMAL types."""
    return pa.schema([
        ("source_path", pa.string()),
        ("line_number", pa.int64()),
        ("message_index", pa.int64()),
        ("raw_line_hash", pa.string()),
        ("source_fingerprint", pa.string()),
        ("__cf_row_id", pa.int64()),
        ("begin_string", pa.string()),
        ("body_length", pa.int64()),
        ("msg_type", pa.string()),
        ("msg_seq_num", pa.int64()),
        ("sending_time", pa.timestamp("us", tz=tz_str)),
        ("sender_comp_id", pa.string()),
        ("target_comp_id", pa.string()),
        ("poss_dup_flag", pa.string()),
        ("orig_sending_time", pa.timestamp("us", tz=tz_str)),
        ("sender_sub_id", pa.string()),
        ("target_sub_id", pa.string()),
        ("sender_location_id", pa.string()),
        ("target_location_id", pa.string()),
        ("cl_ord_id", pa.string()),
        ("orig_cl_ord_id", pa.string()),
        ("order_id", pa.string()),
        ("exec_id", pa.string()),
        ("secondary_cl_ord_id", pa.string()),
        ("cl_ord_link_id", pa.string()),
        ("symbol", pa.string()),
        ("side", pa.string()),
        ("security_id", pa.string()),
        ("security_id_source", pa.string()),
        ("currency", pa.string()),
        ("ord_type", pa.string()),
        ("time_in_force", pa.string()),
        ("order_qty", pa.decimal128(38, 10)),
        ("price", pa.decimal128(38, 10)),
        ("transact_time", pa.timestamp("us", tz=tz_str)),
        ("exec_type", pa.string()),
        ("ord_status", pa.string()),
        ("cum_qty", pa.decimal128(38, 10)),
        ("leaves_qty", pa.decimal128(38, 10)),
        ("last_qty", pa.decimal128(38, 10)),
        ("last_px", pa.decimal128(38, 10)),
        ("avg_px", pa.decimal128(38, 10)),
        ("lifecycle_event", pa.string()),
        ("account", pa.string()),
        ("acct_id_source", pa.string()),
        ("account_type", pa.string()),
        ("text", pa.string()),
        ("ord_rej_reason", pa.string()),
        ("cxl_rej_reason", pa.string()),
        ("business_reject_reason", pa.string()),
        ("ref_msg_type", pa.string()),
        ("ref_seq_num", pa.int64()),
        ("session_reject_reason", pa.string()),
    ])


def _get_session_schema(tz_str: str) -> pa.Schema:
    """Get PyArrow schema for fix_session_events."""
    return pa.schema([
        ("source_path", pa.string()),
        ("line_number", pa.int64()),
        ("message_index", pa.int64()),
        ("raw_line_hash", pa.string()),
        ("source_fingerprint", pa.string()),
        ("__cf_row_id", pa.int64()),
        ("begin_string", pa.string()),
        ("body_length", pa.int64()),
        ("msg_type", pa.string()),
        ("msg_seq_num", pa.int64()),
        ("sending_time", pa.timestamp("us", tz=tz_str)),
        ("sender_comp_id", pa.string()),
        ("target_comp_id", pa.string()),
        ("poss_dup_flag", pa.string()),
        ("orig_sending_time", pa.timestamp("us", tz=tz_str)),
        ("sender_sub_id", pa.string()),
        ("target_sub_id", pa.string()),
        ("ref_msg_type", pa.string()),
        ("ref_seq_num", pa.int64()),
        ("business_reject_ref_id", pa.string()),
        ("business_reject_reason", pa.string()),
        ("session_reject_reason", pa.string()),
        ("text", pa.string()),
        ("cl_ord_id", pa.string()),
        ("order_id", pa.string()),
        ("exec_id", pa.string()),
        ("account", pa.string()),
        ("symbol", pa.string()),
        ("side", pa.string()),
    ])


def _get_tags_schema() -> pa.Schema:
    """Get PyArrow schema for fix_tags."""
    return pa.schema([
        ("source_path", pa.string()),
        ("line_number", pa.int64()),
        ("message_index", pa.int64()),
        ("raw_line_hash", pa.string()),
        ("source_fingerprint", pa.string()),
        ("msg_type", pa.string()),
        ("tag", pa.string()),
        ("value", pa.string()),
        ("position", pa.int64()),
    ])


def _get_errors_schema() -> pa.Schema:
    """Get PyArrow schema for fix_parse_errors."""
    return pa.schema([
        ("source_path", pa.string()),
        ("line_number", pa.int64()),
        ("message_index", pa.int64()),
        ("raw_line_hash", pa.string()),
        ("source_fingerprint", pa.string()),
        ("error_reason", pa.string()),
        ("raw_line_preview", pa.string()),
    ])


def parse(file_path: str) -> List:
    """
    Parse FIX log file and return multi-output list.

    Returns:
        List of Output tuples for fix_order_lifecycle, fix_session_events,
        fix_parse_errors, and optionally fix_tags.
    """
    from casparian_types import Output

    # Validate FIX_TZ is set
    tz = _get_timezone()

    # Check for optional tags allowlist
    tags_allowlist = _get_tags_allowlist()

    # Compute source fingerprint for lineage
    source_fingerprint = _compute_source_fingerprint(file_path)

    # Initialize row collectors
    order_rows: List[Dict[str, Any]] = []
    session_rows: List[Dict[str, Any]] = []
    tag_rows: List[Dict[str, Any]] = []
    error_rows: List[Dict[str, Any]] = []

    # Process file line by line
    with open(file_path, "rb") as f:
        for line_number, raw_line in enumerate(f, start=1):
            # Compute hash before any processing
            raw_line_hash = _compute_line_hash(raw_line)

            # Decode and strip
            try:
                line = raw_line.decode("utf-8", errors="replace").strip()
            except Exception as e:
                error_rows.append(_build_error_row(
                    file_path, line_number, None, raw_line_hash,
                    source_fingerprint,
                    f"Decode error: {e}",
                    raw_line[:100].decode("utf-8", errors="replace")
                ))
                continue

            if not line:
                continue

            # Handle multiple FIX messages per line
            messages = _split_multi_message_line(line)
            if not messages:
                # No FIX message found, but line was non-empty
                if "=" in line:  # Looks like it might be FIX but malformed
                    error_rows.append(_build_error_row(
                        file_path, line_number, None, raw_line_hash,
                        source_fingerprint,
                        "No FIX message found (missing 8=FIX prefix)",
                        line[:200]
                    ))
                continue

            for msg_idx, msg_line in enumerate(messages):
                # Parse FIX message
                tags, error = _parse_fix_message(msg_line)

                if error:
                    error_rows.append(_build_error_row(
                        file_path, line_number, msg_idx, raw_line_hash,
                        source_fingerprint,
                        error,
                        msg_line[:200]
                    ))
                    continue

                # Must have msg_type (tag 35)
                msg_type = tags.get("35")
                if not msg_type:
                    continue

                # Route to appropriate output
                if msg_type in ORDER_MSG_TYPES:
                    order_row, warnings = _build_order_row(
                        tags, file_path, line_number, msg_idx, raw_line_hash,
                        source_fingerprint, tz
                    )
                    order_rows.append(order_row)
                    for warning in warnings:
                        error_rows.append(_build_error_row(
                            file_path, line_number, msg_idx, raw_line_hash,
                            source_fingerprint,
                            warning,
                            msg_line[:200]
                        ))
                elif msg_type in SESSION_MSG_TYPES:
                    session_rows.append(_build_session_row(
                        tags, file_path, line_number, msg_idx, raw_line_hash,
                        source_fingerprint, tz
                    ))

                # Optionally capture tags
                if tags_allowlist:
                    tag_rows.extend(_build_tag_rows(
                        tags, file_path, line_number, msg_idx, raw_line_hash,
                        source_fingerprint, tags_allowlist, msg_line
                    ))

    # Get timezone string for schema
    tz_str = os.environ.get("FIX_TZ", "UTC")

    # Build outputs - always emit all configured outputs (even if empty)
    outputs = []

    # Always emit fix_order_lifecycle (even if empty for schema consistency)
    order_table = _rows_to_table(order_rows, _get_order_schema(tz_str))
    outputs.append(Output("fix_order_lifecycle", order_table))

    # Always emit fix_session_events
    session_table = _rows_to_table(session_rows, _get_session_schema(tz_str))
    outputs.append(Output("fix_session_events", session_table))

    # Always emit fix_parse_errors (even if empty)
    errors_table = _rows_to_table(error_rows, _get_errors_schema())
    outputs.append(Output("fix_parse_errors", errors_table))

    # Only emit fix_tags if allowlist was set
    if tags_allowlist is not None:
        tags_table = _rows_to_table(tag_rows, _get_tags_schema())
        outputs.append(Output("fix_tags", tags_table))

    return outputs


# Schema export for `casparian schema fix` command
def get_schemas() -> Dict[str, pa.Schema]:
    """Return all output schemas for documentation/introspection."""
    tz_str = os.environ.get("FIX_TZ", "UTC")
    return {
        "fix_order_lifecycle": _get_order_schema(tz_str),
        "fix_session_events": _get_session_schema(tz_str),
        "fix_parse_errors": _get_errors_schema(),
        "fix_tags": _get_tags_schema(),
    }
