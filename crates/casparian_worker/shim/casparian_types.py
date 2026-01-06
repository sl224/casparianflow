"""
Casparian Types: Parser Contract Definitions

This module defines the Output NamedTuple that parsers use to declare
their outputs with associated sink destinations.

This file is injected into the parser execution environment by the host,
ensuring the contract is always in sync with the host version.

Usage in parsers:

    # Single output (common case) - bare DataFrame return
    TOPIC = "transactions"
    SINK = "parquet"

    def parse(file_path: str) -> pl.DataFrame:
        return df  # Host wraps with Output(TOPIC, df, SINK)

    # Multi-output (explicit Output objects)
    TOPIC = "MCDATA"

    def parse(file_path: str) -> list[Output]:
        return [
            Output("events", events_df, "parquet"),
            Output("metrics", metrics_df, "sqlite", table="mcdata_metrics"),
        ]
"""

from typing import NamedTuple, Union, Any

# Type alias for supported data types
# Bridge converts all to PyArrow Table before IPC serialization
DataType = Any  # pl.DataFrame, pd.DataFrame, or pa.Table


class Output(NamedTuple):
    """
    Represents a single output from a parser.

    Attributes:
        name: Output identifier (topic name). Must be lowercase, alphanumeric + underscore.
        data: The data to output (polars DataFrame, pandas DataFrame, or pyarrow Table).
        sink: Destination type - "parquet", "sqlite", or "csv".
        table: For sqlite sink: custom table name. Defaults to output name.
        compression: For parquet sink: compression algorithm. Default "snappy".
    """

    name: str
    data: DataType
    sink: str  # "parquet" | "sqlite" | "csv"
    table: str | None = None
    compression: str = "snappy"


# Valid sink types
VALID_SINKS = frozenset(["parquet", "sqlite", "csv"])


def validate_output(output: Output) -> None:
    """
    Validate an Output object.

    Raises:
        ValueError: If output name or sink is invalid.
        TypeError: If data is not a supported type.
    """
    # Validate name
    if not output.name:
        raise ValueError("Output name cannot be empty")

    if not output.name[0].isalpha():
        raise ValueError(f"Output name must start with a letter: {output.name}")

    if not all(c.isalnum() or c == "_" for c in output.name):
        raise ValueError(
            f"Output name must be alphanumeric + underscore only: {output.name}"
        )

    if output.name != output.name.lower():
        raise ValueError(f"Output name must be lowercase: {output.name}")

    # Validate sink
    if output.sink not in VALID_SINKS:
        raise ValueError(
            f"Invalid sink '{output.sink}'. Must be one of: {', '.join(sorted(VALID_SINKS))}"
        )

    # Validate data type (basic check - full validation happens in bridge)
    if output.data is None:
        raise ValueError(f"Output '{output.name}' has None data")
