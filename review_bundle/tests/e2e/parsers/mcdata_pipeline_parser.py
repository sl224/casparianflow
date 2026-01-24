"""
MCData Parser for E2E Pipeline Testing - Multi-Output SQLite Version

Parses MCData files (headerless CSV) into structured records.
Uses record type discriminators (column_2 values ending in `:`) to route
data to separate SQLite tables with type-specific schemas.

Record Types:
- CONFIG_FLTS: Configuration fault status (subsystem, component, status)
- RFC_DB: Readiness fault codes with confirmation tracking
- PFC_DB: Published fault codes with detailed metadata
- NFS2_SYS: NFS2 system status (temperatures, pressures)
- NAV_DATA: Navigation data (position, velocity)
- ROTOSCAN: Radar rotation data
- LCS_TEMP: LCS temperature readings
- TIM_SRC: Time source data
- CAW_DB: Caution/advisory/warning database
- Other ACT_* records: Activation/configuration records

Note: The Output class is injected by the bridge runtime, not imported.
"""
import polars as pl

# Parser metadata
TOPIC = "mcdata"


def parse(input_path: str) -> "list[Output]":
    """
    Parse MCData file into multiple SQLite tables.

    Discriminates by record_type (column_2) and creates a separate
    Output for each record type with an appropriate schema.
    """
    # Read raw CSV without headers - all as strings initially
    df = pl.read_csv(
        input_path,
        has_header=False,
        infer_schema_length=0,  # Read all as strings
        truncate_ragged_lines=True,
    )

    # Add common columns that exist in all records
    df = df.with_columns([
        pl.col("column_1").alias("record_num"),
        pl.col("column_2").str.strip_chars(":").alias("record_type"),
        pl.col("column_4").alias("timestamp"),
    ])

    outputs = []

    # ============================================================
    # CONFIG_FLTS: Configuration Faults
    # Columns: record_num, record_type, timestamp, subsystem, component_type, status
    # ============================================================
    config_flts = df.filter(pl.col("record_type") == "CONFIG_FLTS").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("subsystem"),
        pl.col("column_6").alias("component_type"),
        pl.col("column_7").alias("status"),
    ])
    if config_flts.height > 0:
        outputs.append(Output("config_flts", config_flts, "duckdb", table="config_flts"))

    # ============================================================
    # RFC_DB: Readiness Fault Codes
    # Columns: timestamp, source, fault_code, status, time, bit_type, stats...
    # ============================================================
    rfc_db = df.filter(pl.col("record_type") == "RFC_DB").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("source"),
        pl.col("column_6").alias("fault_code"),
        pl.col("column_7").alias("status"),
        pl.col("column_8").alias("status_time"),
        pl.col("column_9").alias("bit_type"),
        pl.col("column_10").alias("consec_true_label"),
        pl.col("column_11").alias("consec_true"),
        pl.col("column_12").alias("total_true_label"),
        pl.col("column_13").alias("total_true"),
        pl.col("column_14").alias("consec_false_label"),
        pl.col("column_15").alias("consec_false"),
        pl.col("column_16").alias("total_false_label"),
        pl.col("column_17").alias("total_false"),
        pl.col("column_18").alias("total_count_label"),
        pl.col("column_19").alias("total_count"),
        pl.col("column_20").alias("raw_fault_code"),
        pl.col("column_21").alias("qualifier"),
    ])
    if rfc_db.height > 0:
        outputs.append(Output("rfc_db", rfc_db, "duckdb", table="rfc_db"))

    # ============================================================
    # PFC_DB: Published Fault Codes
    # Columns: fault_code, description, timestamp, system, criticality, ...
    # ============================================================
    pfc_db = df.filter(pl.col("record_type") == "PFC_DB").select([
        pl.col("record_num"),
        pl.col("column_5").alias("fault_code"),
        pl.col("column_6").alias("description"),
        pl.col("column_7").alias("timestamp"),
        pl.col("column_8").alias("system"),
        pl.col("column_9").alias("criticality"),
        pl.col("column_10").alias("lru_reference"),
        pl.col("column_13").alias("status"),
        pl.col("column_18").alias("group_type"),
        pl.col("column_20").alias("false_count"),
        # Flags as raw text
        pl.concat_str([
            pl.col(f"column_{i}") for i in range(21, min(45, len(df.columns) + 1))
        ], separator=",", ignore_nulls=True).alias("flags"),
    ])
    if pfc_db.height > 0:
        outputs.append(Output("pfc_db", pfc_db, "duckdb", table="pfc_db"))

    # ============================================================
    # NFS2_SYS: NFS2 System Status
    # Columns: timestamp, temp1, temp2, rsm_*, crsm_*, ps_*, sys_*, dip_*
    # ============================================================
    nfs2_sys = df.filter(pl.col("record_type") == "NFS2_SYS").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_6").alias("temp_label"),
        pl.col("column_7").alias("temp1"),
        pl.col("column_8").alias("temp2"),
        pl.col("column_11").alias("rsm_label"),
        pl.col("column_12").alias("rsm_temp"),
        pl.col("column_13").alias("rsm_state1"),
        pl.col("column_14").alias("rsm_state2"),
        pl.col("column_15").alias("rsm_state3"),
        pl.col("column_16").alias("rsm_power1"),
        pl.col("column_17").alias("rsm_power2"),
        pl.col("column_18").alias("crsm_label"),
        pl.col("column_19").alias("crsm_temp"),
        pl.col("column_20").alias("crsm_state1"),
        pl.col("column_21").alias("crsm_state2"),
        pl.col("column_22").alias("crsm_state3"),
        pl.col("column_23").alias("crsm_power1"),
        pl.col("column_24").alias("crsm_power2"),
        pl.col("column_25").alias("ps_label"),
        pl.col("column_26").alias("ps_temp"),
        pl.col("column_27").alias("sys_label"),
        pl.col("column_28").alias("sys_state"),
        pl.col("column_29").alias("dip_label"),
        pl.col("column_30").alias("dip_value"),
    ])
    if nfs2_sys.height > 0:
        outputs.append(Output("nfs2_sys", nfs2_sys, "duckdb", table="nfs2_sys"))

    # ============================================================
    # NAV_DATA: Navigation Data
    # Columns: timestamp, mode, various navigation parameters
    # ============================================================
    nav_data = df.filter(pl.col("record_type") == "NAV_DATA").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("mode"),
        pl.col("column_6").alias("param1"),
        pl.col("column_7").alias("param2"),
        pl.col("column_8").alias("param3"),
        pl.col("column_9").alias("param4"),
        pl.col("column_10").alias("param5"),
        pl.col("column_11").alias("param6"),
        pl.col("column_12").alias("param7"),
        pl.col("column_13").alias("source_type"),
        pl.col("column_14").alias("param8"),
        pl.col("column_15").alias("param9"),
        pl.col("column_16").alias("flag1"),
        pl.col("column_17").alias("param10"),
        pl.col("column_18").alias("param11"),
        pl.col("column_19").alias("status_flags"),
        pl.col("column_20").alias("flag2"),
        pl.col("column_21").alias("flag3"),
    ])
    if nav_data.height > 0:
        outputs.append(Output("nav_data", nav_data, "duckdb", table="nav_data"))

    # ============================================================
    # ROTOSCAN: Radar Rotation Data
    # ============================================================
    rotoscan = df.filter(pl.col("record_type") == "ROTOSCAN").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("source"),
        pl.col("column_6").alias("rpm_value"),
        pl.col("column_7").alias("rpm_mode"),
        pl.col("column_8").alias("scan_value"),
    ])
    if rotoscan.height > 0:
        outputs.append(Output("rotoscan", rotoscan, "duckdb", table="rotoscan"))

    # ============================================================
    # LCS_TEMP: LCS Temperature Data
    # ============================================================
    lcs_temp = df.filter(pl.col("record_type") == "LCS_TEMP").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("temperature"),
        pl.col("column_6").alias("state"),
        pl.col("column_7").alias("state_time"),
    ])
    if lcs_temp.height > 0:
        outputs.append(Output("lcs_temp", lcs_temp, "duckdb", table="lcs_temp"))

    # ============================================================
    # TIM_SRC: Time Source Data
    # ============================================================
    tim_src = df.filter(pl.col("record_type") == "TIM_SRC").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.concat_str([
            pl.col(f"column_{i}") for i in range(5, min(20, len(df.columns) + 1))
        ], separator=",", ignore_nulls=True).alias("raw_data"),
    ])
    if tim_src.height > 0:
        outputs.append(Output("tim_src", tim_src, "duckdb", table="tim_src"))

    # ============================================================
    # CAW_DB: Caution/Advisory/Warning Database
    # ============================================================
    caw_db = df.filter(pl.col("record_type") == "CAW_DB").select([
        pl.col("record_num"),
        pl.col("timestamp"),
        pl.col("column_5").alias("caw_id"),
        pl.col("column_6").alias("priority"),
        pl.col("column_7").alias("category"),
        pl.col("column_8").alias("limit_type"),
        pl.col("column_9").alias("confirmed_state"),
        pl.col("column_10").alias("indicator"),
    ])
    if caw_db.height > 0:
        outputs.append(Output("caw_db", caw_db, "duckdb", table="caw_db"))

    # ============================================================
    # TEST_STATES_*: Test State Records
    # ============================================================
    test_states = df.filter(pl.col("record_type").str.starts_with("TEST_STATES")).select([
        pl.col("record_num"),
        pl.col("record_type"),
        pl.concat_str([
            pl.col(f"column_{i}") for i in range(5, min(40, len(df.columns) + 1))
        ], separator=",", ignore_nulls=True).alias("raw_data"),
    ])
    if test_states.height > 0:
        outputs.append(Output("test_states", test_states, "duckdb", table="test_states"))

    # ============================================================
    # ACT_* : Activation/Configuration Records (catch-all for remaining)
    # ============================================================
    act_records = df.filter(
        pl.col("record_type").str.starts_with("ACT_")
    ).select([
        pl.col("record_num"),
        pl.col("record_type"),
        pl.col("timestamp"),
        pl.concat_str([
            pl.col(f"column_{i}") for i in range(5, min(30, len(df.columns) + 1))
        ], separator=",", ignore_nulls=True).alias("raw_data"),
    ])
    if act_records.height > 0:
        outputs.append(Output("act_records", act_records, "duckdb", table="act_records"))

    # ============================================================
    # OTHER: Catch-all for unmatched record types
    # ============================================================
    known_types = [
        "CONFIG_FLTS", "RFC_DB", "PFC_DB", "NFS2_SYS", "NAV_DATA",
        "ROTOSCAN", "LCS_TEMP", "TIM_SRC", "CAW_DB",
    ]
    other_records = df.filter(
        ~pl.col("record_type").str.starts_with("ACT_") &
        ~pl.col("record_type").str.starts_with("TEST_STATES") &
        ~pl.col("record_type").is_in(known_types)
    ).select([
        pl.col("record_num"),
        pl.col("record_type"),
        pl.col("timestamp"),
        pl.concat_str([
            pl.col(f"column_{i}") for i in range(5, min(30, len(df.columns) + 1))
        ], separator=",", ignore_nulls=True).alias("raw_data"),
    ])
    if other_records.height > 0:
        outputs.append(Output("other_records", other_records, "duckdb", table="other_records"))

    # If no outputs were created (shouldn't happen), return empty parquet
    if not outputs:
        empty_df = pl.DataFrame({"error": ["No records parsed"]})
        outputs.append(Output("mcdata_error", empty_df, "parquet"))

    return outputs
