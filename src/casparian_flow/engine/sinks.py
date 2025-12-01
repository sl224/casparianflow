import logging
from typing import Any, Protocol, Dict
from pathlib import Path
import pandas as pd
from sqlalchemy import create_engine, Engine

logger = logging.getLogger(__name__)

class DataSink(Protocol):
    def write(self, data: Any): ...
    def close(self): ...

class MssqlSink:
    """
    Writes DataFrames to a specific MSSQL Table.
    """
    def __init__(self, engine: Engine, table_name: str, options: Dict):
        self.engine = engine
        self.table_name = table_name
        self.schema = options.get("schema", "dbo")
        self.if_exists = options.get("mode", "append")
        self.chunksize = options.get("chunksize", 10000)

    def write(self, data: Any):
        if not isinstance(data, pd.DataFrame):
            logger.warning(f"MssqlSink expected DataFrame, got {type(data)}")
            return

        # Optimization: We use the connection pool from the engine
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
        # We don't close the engine here because it's shared
        pass

class ParquetSink:
    """
    Writes DataFrames to a generic Parquet file.
    """
    def __init__(self, root_path: Path, relative_path: str, options: Dict):
        self.full_path = root_path / relative_path
        self.full_path.parent.mkdir(parents=True, exist_ok=True)
        self.compression = options.get("compression", "snappy")
        
        # If we wanted to support streaming appending, we'd open a ParquetWriter here.
        # For this version, we assume batch writes or one-shot writes.

    def write(self, data: Any):
        if not isinstance(data, pd.DataFrame):
            return
            
        # Append logic for Parquet is complex (requires reading schema). 
        # For simplicity in this iteration, we overwrite or assume unique filenames per batch.
        # In a real "Swarm", plugins usually write 1 parquet file per input file.
        data.to_parquet(self.full_path, compression=self.compression, index=False)

    def close(self):
        pass