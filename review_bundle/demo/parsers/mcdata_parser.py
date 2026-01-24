"""
MCData Parser - Parses E2D MCData files into structured records

This parser handles the headerless CSV format used in MCData files,
extracting configuration and fault data into queryable records.
"""
import csv
import duckdb
from datetime import datetime
from pathlib import Path


# Column definitions based on observed structure
COLUMNS = [
    "record_type",      # Column 0: Always "1"
    "event_name",       # Column 1: e.g., "ACT_FCNS1_SW:", "CONFIG_FLTS"
    "field_2",          # Column 2: Usually empty
    "timestamp",        # Column 3: Date/time
    "subsystem",        # Column 4: e.g., "MUX_FCNS1", "MIDS"
    "component_type",   # Column 5: "SW", "HW", "FW"
    "status",           # Column 6: "CLEARED" or numeric
    # Remaining columns are variable data fields
]


def parse_mcdata(file_path):
    """Parse MCData file into list of records."""
    records = []

    with open(file_path, 'r') as f:
        reader = csv.reader(f)
        for row_num, row in enumerate(reader, 1):
            if not row or len(row) < 4:
                continue

            record = {
                "row_number": row_num,
                "record_type": row[0] if len(row) > 0 else None,
                "event_name": row[1] if len(row) > 1 else None,
                "timestamp": parse_timestamp(row[3]) if len(row) > 3 else None,
                "subsystem": row[4] if len(row) > 4 and row[4] else None,
                "component_type": row[5] if len(row) > 5 and row[5] else None,
                "status": row[6] if len(row) > 6 and row[6] else None,
                "raw_data": ",".join(row[7:50]) if len(row) > 7 else None,
            }
            records.append(record)

    return records


def parse_timestamp(ts_str):
    """Parse timestamp string to ISO format."""
    if not ts_str:
        return None
    try:
        # Format: 02/03/2025 01:08:00
        dt = datetime.strptime(ts_str.strip(), "%m/%d/%Y %H:%M:%S")
        return dt.isoformat()
    except ValueError:
        return ts_str  # Return original if can't parse


def to_duckdb(records, db_path, table_name="mcdata"):
    """Write records to DuckDB database."""
    conn = duckdb.connect(db_path)

    # Create table
    conn.execute(f"""
        CREATE TABLE IF NOT EXISTS {table_name} (
            id BIGINT PRIMARY KEY,
            row_number INTEGER,
            record_type TEXT,
            event_name TEXT,
            timestamp TEXT,
            subsystem TEXT,
            component_type TEXT,
            status TEXT,
            raw_data TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    """)

    # Insert records
    rows = [
        (
            i + 1,
            record["row_number"],
            record["record_type"],
            record["event_name"],
            record["timestamp"],
            record["subsystem"],
            record["component_type"],
            record["status"],
            record["raw_data"],
        )
        for i, record in enumerate(records)
    ]
    conn.executemany(
        f"""
        INSERT INTO {table_name}
        (id, row_number, record_type, event_name, timestamp, subsystem, component_type, status, raw_data)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        rows,
    )
    print(f"Inserted {len(records)} records into {db_path}:{table_name}")

    # Show sample
    count = conn.execute(f"SELECT COUNT(*) FROM {table_name}").fetchone()[0]
    print(f"Total records in table: {count}")

    events = [r[0] for r in conn.execute(f"SELECT DISTINCT event_name FROM {table_name} LIMIT 10").fetchall()]
    print(f"Sample event types: {events}")

    conn.close()
    return count


# For use with casparian bridge
def transform(file_path):
    """Transform function for casparian plugin system."""
    records = parse_mcdata(file_path)
    for record in records:
        yield record


if __name__ == "__main__":
    import sys

    if len(sys.argv) < 2:
        print("Usage: python mcdata_parser.py <mcdata_file> [output.db]")
        sys.exit(1)

    input_file = sys.argv[1]
    output_db = sys.argv[2] if len(sys.argv) > 2 else "mcdata_output.db"

    print(f"Parsing: {input_file}")
    records = parse_mcdata(input_file)
    print(f"Parsed {len(records)} records")

    to_duckdb(records, output_db)
    print(f"Output: {output_db}")
