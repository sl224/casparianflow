# FIX Protocol Schema v1.2

This document defines the schema for FIX (Financial Information eXchange) protocol parsing outputs.

## Parser Outputs

The FIX parser produces up to four outputs:

| Output | Message Types | Description |
|--------|--------------|-------------|
| `fix_order_lifecycle` | D, 8, F, G, 9, H | Order messages and execution reports |
| `fix_session_events` | A, 0, 1, 2, 3, 4, 5, j | Session-level messages (logon, heartbeat, etc.) |
| `fix_parse_errors` | - | Parse errors for malformed lines (always created) |
| `fix_tags` | All (optional) | Raw tag-value pairs when `FIX_TAGS_ALLOWLIST` is set |

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `FIX_TZ` | Yes | Timezone for timestamp parsing (e.g., `UTC`, `America/New_York`). Parser fails if missing. |
| `FIX_TAGS_ALLOWLIST` | No | Comma-separated numeric tags to capture in `fix_tags` output. If unset, `fix_tags` is not emitted. |

## Parsing Rules

1. **Timestamps**: Parsed to RFC3339 with explicit timezone offset from `FIX_TZ`. Invalid timestamps become null.
2. **Numeric precision**: Price and quantity fields use `decimal(38,10)`. Parsed via Python `Decimal`. Never use floats.
3. **Decimal rounding**: Values with more than 10 fractional digits are rounded and recorded in `fix_parse_errors`.
4. **Delimiters**: Parser handles `\x01` (SOH), `|`, `^A`, and space-delimited messages.
5. **Log prefixes**: Parser finds `8=FIX` anywhere in line, supporting log files with timestamps or gateway headers.
6. **Multi-message lines**: Lines with multiple FIX messages are split and each is processed separately.
7. **Message index**: `message_index` is a 0-based index within the line, used to disambiguate multi-message lines.
8. **Line hashing**: `raw_line_hash` is SHA256 hex of the raw line bytes (excluding trailing newline).
9. **Source fingerprint**: `source_fingerprint` is a 16-char hash of file size + mtime for lineage tracking.
10. **Row ID**: `__cf_row_id` equals `line_number` for quarantine mapping and lineage tracking.
11. **Error tracking**: Malformed lines are recorded in `fix_parse_errors` with reason and preview.

---

## fix_order_lifecycle (v1)

Order lifecycle messages: New Order Single (D), Execution Reports (8), Order Cancel Request (F), Order Cancel/Replace Request (G), Order Cancel Reject (9), Order Status Request (H).

### Context Columns (Required)

| Column | Type | Tag | Notes |
|--------|------|-----|-------|
| source_path | string | - | Absolute or relative file path |
| line_number | int64 | - | 1-based line index |
| message_index | int64 | - | 0-based message index within the line |
| raw_line_hash | string | - | SHA256 hex of raw line (no trailing newline) |
| source_fingerprint | string | - | 16-char hash of file size + mtime for lineage |
| __cf_row_id | int64 | - | Equals line_number; for lineage/quarantine |

### Header Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| begin_string | string | 8 | Yes | FIX version (e.g., FIX.4.2, FIX.4.4) |
| body_length | int64 | 9 | No | Message body length |
| msg_type | string | 35 | Yes | D, 8, F, G, 9, H |
| msg_seq_num | int64 | 34 | Yes | Message sequence number |
| sending_time | timestamp_tz | 52 | Yes | RFC3339 with offset from FIX_TZ |
| sender_comp_id | string | 49 | Yes | Sender identifier |
| target_comp_id | string | 56 | Yes | Target identifier |
| poss_dup_flag | string | 43 | No | Y/N |
| orig_sending_time | timestamp_tz | 122 | No | Original sending time (resends) |
| sender_sub_id | string | 50 | No | Sender sub-identifier |
| target_sub_id | string | 57 | No | Target sub-identifier |
| sender_location_id | string | 142 | No | Sender location |
| target_location_id | string | 143 | No | Target location |

### Order Identification

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| cl_ord_id | string | 11 | No | Client order ID |
| orig_cl_ord_id | string | 41 | No | Original client order ID (cancel/replace) |
| order_id | string | 37 | No | Broker-assigned order ID |
| exec_id | string | 17 | No | Execution ID |
| secondary_cl_ord_id | string | 526 | No | Secondary client order ID |
| cl_ord_link_id | string | 583 | No | Client order link ID |

### Instrument Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| symbol | string | 55 | No | Ticker symbol |
| side | string | 54 | No | 1=Buy, 2=Sell, etc. |
| security_id | string | 48 | No | Security identifier |
| security_id_source | string | 22 | No | ID source (1=CUSIP, 4=ISIN, etc.) |
| currency | string | 15 | No | ISO currency code |

### Order Parameters

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| ord_type | string | 40 | No | 1=Market, 2=Limit, etc. |
| time_in_force | string | 59 | No | 0=Day, 1=GTC, etc. |
| order_qty | decimal(38,10) | 38 | No | Order quantity |
| price | decimal(38,10) | 44 | No | Limit price |
| transact_time | timestamp_tz | 60 | No | Transaction time |

### Execution Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| exec_type | string | 150 | No | 0=New, 1=PartialFill, 2=Fill, etc. |
| ord_status | string | 39 | No | 0=New, 1=PartialFill, 2=Filled, etc. |
| cum_qty | decimal(38,10) | 14 | No | Cumulative executed quantity |
| leaves_qty | decimal(38,10) | 151 | No | Remaining quantity |
| last_qty | decimal(38,10) | 32 | No | Last fill quantity |
| last_px | decimal(38,10) | 31 | No | Last fill price |
| avg_px | decimal(38,10) | 6 | No | Average price |

### Derived Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| lifecycle_event | string | - | No | Derived: new_order, ack, partial_fill, fill, cancel, replace, reject, exec_report |

### Account Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| account | string | 1 | No | Account identifier |
| acct_id_source | string | 660 | No | Account ID source |
| account_type | string | 581 | No | Account type |

### Reject/Diagnostic Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| text | string | 58 | No | Free-form text/reject reason |
| ord_rej_reason | string | 103 | No | Order reject reason code |
| cxl_rej_reason | string | 102 | No | Cancel reject reason code |
| business_reject_reason | string | 380 | No | Business reject reason |
| ref_msg_type | string | 372 | No | Referenced message type |
| ref_seq_num | int64 | 45 | No | Referenced sequence number |
| session_reject_reason | string | 373 | No | Session reject reason |

---

## fix_session_events (v1)

Session-level messages: Logon (A), Heartbeat (0), Test Request (1), Resend Request (2), Reject (3), Sequence Reset (4), Logout (5), Business Message Reject (j).

### Context Columns (Required)

| Column | Type | Tag | Notes |
|--------|------|-----|-------|
| source_path | string | - | Absolute or relative file path |
| line_number | int64 | - | 1-based line index |
| message_index | int64 | - | 0-based message index within the line |
| raw_line_hash | string | - | SHA256 hex of raw line |
| source_fingerprint | string | - | 16-char hash of file size + mtime |
| __cf_row_id | int64 | - | Equals line_number |

### Header Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| begin_string | string | 8 | Yes | FIX version |
| body_length | int64 | 9 | No | Message body length |
| msg_type | string | 35 | Yes | A, 0, 1, 2, 3, 4, 5, j |
| msg_seq_num | int64 | 34 | Yes | Message sequence number |
| sending_time | timestamp_tz | 52 | Yes | RFC3339 with offset |
| sender_comp_id | string | 49 | Yes | Sender identifier |
| target_comp_id | string | 56 | Yes | Target identifier |
| poss_dup_flag | string | 43 | No | Y/N |
| orig_sending_time | timestamp_tz | 122 | No | Original sending time |
| sender_sub_id | string | 50 | No | Sender sub-identifier |
| target_sub_id | string | 57 | No | Target sub-identifier |

### Reference/Diagnostic Fields

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| ref_msg_type | string | 372 | No | Referenced message type |
| ref_seq_num | int64 | 45 | No | Referenced sequence number |
| business_reject_ref_id | string | 379 | No | Business reject reference ID |
| business_reject_reason | string | 380 | No | Business reject reason |
| session_reject_reason | string | 373 | No | Session reject reason |
| text | string | 58 | No | Free-form text |

### Context Linkage Fields (Optional)

These fields provide context when session messages relate to specific orders:

| Column | Type | Tag | Required | Notes |
|--------|------|-----|----------|-------|
| cl_ord_id | string | 11 | No | Related client order ID |
| order_id | string | 37 | No | Related broker order ID |
| exec_id | string | 17 | No | Related execution ID |
| account | string | 1 | No | Related account |
| symbol | string | 55 | No | Related symbol |
| side | string | 54 | No | Related side |

---

## fix_tags (Optional)

Raw tag-value pairs for flexible analysis. Only emitted when `FIX_TAGS_ALLOWLIST` environment variable is set.

| Column | Type | Notes |
|--------|------|-------|
| source_path | string | File path |
| line_number | int64 | 1-based line index |
| message_index | int64 | 0-based message index within the line |
| raw_line_hash | string | SHA256 hex of raw line |
| source_fingerprint | string | 16-char hash of file size + mtime |
| msg_type | string | Message type (tag 35) |
| tag | string | Numeric tag as string |
| value | string | Raw tag value |
| position | int64 | 1-based position within message |

---

## fix_parse_errors (v1.2)

Parse errors for malformed lines. Always created (may be empty).

| Column | Type | Notes |
|--------|------|-------|
| source_path | string | File path |
| line_number | int64 | 1-based line index |
| message_index | int64 | 0-based message index within the line |
| raw_line_hash | string | SHA256 hex of raw line |
| source_fingerprint | string | 16-char hash of file size + mtime |
| error_reason | string | Description of the parse failure |
| raw_line_preview | string | First 500 chars of the raw line |

---

## Data Type Reference

| Type | Arrow Type | Notes |
|------|------------|-------|
| string | utf8 | UTF-8 encoded |
| int64 | int64 | 64-bit signed integer |
| decimal(38,10) | decimal128(38,10) | Fixed precision; no floats |
| timestamp_tz | timestamp[us, tz=...] | Microsecond precision with timezone |

---

## Lifecycle Event Derivation

The `lifecycle_event` column is derived from `msg_type`, `exec_type`, and `ord_status`:

| msg_type | exec_type | ord_status | lifecycle_event |
|----------|-----------|------------|-----------------|
| D | - | - | new_order |
| 8 | 0 | - | ack |
| 8 | 1 | - | partial_fill |
| 8 | 2 | - | fill |
| 8 | 4 | - | cancel |
| 8 | 5 | - | replace |
| 8 | - | 8 | reject |
| 8 | (other) | - | exec_report |
| F | - | - | cancel_request |
| G | - | - | replace_request |
| 9 | - | - | cancel_reject |
| H | - | - | status_request |

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.2 | 2025-01 | Add message_index, decimal rounding warnings, fix_tags/fix_parse_errors lineage fields |
| 1.1 | 2025-01 | Add fix_parse_errors, source_fingerprint, DECIMAL types, prefix/multi-message support |
| 1.0 | 2025-01 | Initial schema for FIX 4.2/4.4 support |
