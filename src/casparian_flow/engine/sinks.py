# src/casparian_flow/engine/sinks.py
import logging
from typing import Any, Protocol, Dict, Union
from pathlib import Path
from urllib.parse import urlparse, parse_qs
import pandas as pd
from sqlalchemy import Engine

# Graceful degradation if pyarrow is missing
try:
    import pyarrow as pa
    from pyarrow import parquet as pq
    HAS_ARROW = True
except ImportError:
    HAS_ARROW = False

logger = logging.getLogger(__name__)

class DataSink(Protocol):
    def write(self, data: Any): ...
    def close(self): ...

class MssqlSink:
    """
    URI Example: mssql://schema.table?mode=append&chunksize=5000
    """
    def __init__(self, engine: Engine, table_path: str, options: Dict):
        self.engine = engine
        
        # Support "schema.table" or just "table" (defaulting to dbo)
        if "." in table_path:
            self.schema, self.table_name = table_path.split(".", 1)
        else:
            self.schema = "dbo"
            self.table_name = table_path
            
        self.if_exists = options.get("mode", ["append"])[0]
        self.chunksize = int(options.get("chunksize", [10000])[0])

    def write(self, data: Any):
        # Normalize Arrow -> Pandas for SQLAlchemy compatibility
        if HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            data = data.to_pandas()
            
        if not isinstance(data, pd.DataFrame):
            logger.warning(f"MssqlSink expected DataFrame, got {type(data)}")
            return

        with self.engine.begin() as conn:
            data.to_sql(
                name=self.table_name,
                schema=self.schema,
                con=conn,
                if_exists=self.if_exists,
                index=False,
                chunksize=self.chunksize
            )

    def close(self):
        pass

class ParquetSink:
    """
    URI Example: parquet://folder/subfolder?compression=snappy
    """
    def __init__(self, root_path: Path, relative_path: str, options: Dict):
        self.full_path = root_path / relative_path
        self.full_path.parent.mkdir(parents=True, exist_ok=True)
        self.compression = options.get("compression", ["snappy"])[0]
        self.writer = None

    def write(self, data: Any):
        # Normalize Pandas -> Arrow for strict schema enforcement
        table = None
        if isinstance(data, pd.DataFrame):
            if HAS_ARROW:
                table = pa.Table.from_pandas(data)
            else:
                # Fallback without pyarrow
                data.to_parquet(self.full_path, compression=self.compression, index=False)
                return
        elif HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
             if isinstance(data, pa.RecordBatch):
                 table = pa.Table.from_batches([data])
             else:
                 table = data
        
        if table:
            if self.writer is None:
                self.writer = pq.ParquetWriter(self.full_path, table.schema, compression=self.compression)
            self.writer.write_table(table)

    def close(self):
        if self.writer:
            self.writer.close()

class SinkFactory:
    @staticmethod
    def create(uri: str, sql_engine: Engine, parquet_root: Path) -> DataSink:
        """
        Creates the physical sink from a URI string.
        """
        parsed = urlparse(uri)
        scheme = parsed.scheme
        path = parsed.netloc + parsed.path 
        options = parse_qs(parsed.query)

        if scheme == "mssql":
            return MssqlSink(sql_engine, path, options)
        elif scheme == "parquet":
            clean_path = path.lstrip("/") 
            return ParquetSink(parquet_root, clean_path, options)
        else:
            raise ValueError(f"Unsupported sink scheme: {scheme}")