# FIX Demo Quickstart

This walkthrough uses the tiny demo log at `docs/demo/fix/fix_demo.fix` and the
first-class parser at `parsers/fix/fix_parser.py`. It produces DuckDB tables
named `fix_order_lifecycle`, `fix_session_events`, and `fix_parse_errors`
(plus optional `fix_tags` when `FIX_TAGS_ALLOWLIST` is set).

Prereqs:
- Build the CLI: `cargo build --release` (or use `cargo build` and adjust paths).
- `python3` with `pandas` or `pyarrow`: `python3 -m pip install pandas pyarrow`
- DuckDB CLI for queries: `brew install duckdb` (or use Python with `duckdb`).

1) Scan the demo folder

```bash
./target/release/casparian scan docs/demo/fix --type fix
```

2) Preview the log

```bash
./target/release/casparian preview docs/demo/fix/fix_demo.fix --head 3
```

3) Run the parser (creates `fix_order_lifecycle`, `fix_session_events`, `fix_parse_errors`)

```bash
FIX_TZ=UTC ./target/release/casparian run \
  parsers/fix/fix_parser.py \
  docs/demo/fix/fix_demo.fix \
  --sink duckdb://./output/fix_demo.duckdb
```

4) Query by ClOrdID

```bash
duckdb ./output/fix_demo.duckdb \
  "SELECT cl_ord_id, msg_type, exec_type, ord_status, symbol, order_qty, price, sending_time \
   FROM fix_order_lifecycle WHERE cl_ord_id = 'CLORD1' ORDER BY msg_seq_num;"
```

Expected: 4 rows for `CLORD1` (new, ack, partial fill, fill).
