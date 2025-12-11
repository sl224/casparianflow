# src/casparian_flow/engine/sinks.py
import logging
import shutil
from typing import Any, Protocol, Dict
from pathlib import Path
from urllib.parse import urlparse, parse_qs
import pandas as pd
from sqlalchemy import Engine, create_engine, text

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
    def promote(self): ...


class MssqlSink:
    def __init__(self, engine: Engine, table_path: str, options: Dict, job_id: int):
        self.engine = engine
        self.job_id = job_id

        if "." in table_path:
            self.schema, self.table_name = table_path.split(".", 1)
        else:
            self.schema = "dbo"
            self.table_name = table_path

        self.if_exists = options.get("mode", ["append"])[0]
        self.chunksize = int(options.get("chunksize", [10000])[0])

        # Staging Table Name
        self.staging_table = f"stg_{self.table_name}_{self.job_id}"

    def write(self, data: Any):
        if HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            data = data.to_pandas()

        if not isinstance(data, pd.DataFrame):
            return

        with self.engine.begin() as conn:
            data.to_sql(
                name=self.staging_table,
                schema=self.schema,
                con=conn,
                if_exists="append",
                index=False,
                chunksize=self.chunksize,
            )

    def promote(self):
        logger.info(f"Promoting {self.staging_table} to {self.table_name}")
        with self.engine.begin() as conn:
            # 1. Create Target if not exists (Lazy init usually handled by first to_sql if we wrote directly)
            # Here we assume target might exist.

            # Simple Append Strategy: INSERT INTO Target SELECT * FROM Staging
            try:
                insert_sql = f"""
                INSERT INTO {self.schema}.{self.table_name} 
                SELECT * FROM {self.schema}.{self.staging_table};
                """
                conn.execute(text(insert_sql))
                conn.execute(text(f"DROP TABLE {self.schema}.{self.staging_table}"))
            except Exception as e:
                logger.error(f"Promotion failed: {e}")
                raise

    def close(self):
        pass


class SqliteSink:
    def __init__(self, db_path: str, table_name: str, options: Dict, job_id: int):
        self.engine = create_engine(f"sqlite:///{db_path}")
        self.table_name = table_name
        self.staging_table = f"{table_name}_stg_{job_id}"
        self.if_exists = options.get("mode", ["append"])[0]
        self.chunksize = int(options.get("chunksize", [10000])[0])

    def write(self, data: Any):
        if HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            data = data.to_pandas()

        with self.engine.begin() as conn:
            data.to_sql(
                name=self.staging_table,
                con=conn,
                if_exists="append",
                index=False,
                chunksize=self.chunksize,
            )

    def promote(self):
        with self.engine.begin() as conn:
            # Check if target table exists
            try:
                conn.execute(text(f"SELECT 1 FROM {self.table_name} LIMIT 1"))
                exists = True
            except:
                exists = False

            if not exists:
                conn.execute(
                    text(
                        f"ALTER TABLE {self.staging_table} RENAME TO {self.table_name}"
                    )
                )
            else:
                conn.execute(
                    text(
                        f"INSERT INTO {self.table_name} SELECT * FROM {self.staging_table}"
                    )
                )
                conn.execute(text(f"DROP TABLE {self.staging_table}"))

    def close(self):
        self.engine.dispose()


class ParquetSink:
    def __init__(self, root_path: Path, relative_path: str, options: Dict, job_id: int):
        self.final_path = root_path / relative_path
        # Staging: Write to a distinct file
        self.staging_path = root_path / f"{relative_path}.stg.{job_id}"

        self.staging_path.parent.mkdir(parents=True, exist_ok=True)
        self.compression = options.get("compression", ["snappy"])[0]
        self.writer = None

    def write(self, data: Any):
        table = None
        if isinstance(data, pd.DataFrame):
            if HAS_ARROW:
                table = pa.Table.from_pandas(data)
            else:
                data.to_parquet(
                    self.staging_path, compression=self.compression, index=False
                )
                return
        elif HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            if isinstance(data, pa.RecordBatch):
                table = pa.Table.from_batches([data])
            else:
                table = data

        if table:
            if self.writer is None:
                self.writer = pq.ParquetWriter(
                    self.staging_path, table.schema, compression=self.compression
                )
            self.writer.write_table(table)

    def promote(self):
        if self.writer:
            self.writer.close()
            self.writer = None

        if self.staging_path.exists():
            # Move staging file into the target directory
            filename = self.staging_path.name.replace(".stg.", ".")

            # Treat final_path as a directory for datasets
            target_dir = self.final_path
            if target_dir.suffix != "":
                # If user specified a file-like path, assume parent is dir
                target_dir = target_dir.parent

            target_dir.mkdir(parents=True, exist_ok=True)
            target = target_dir / filename

            shutil.move(str(self.staging_path), str(target))

    def close(self):
        if self.writer:
            self.writer.close()


class SinkFactory:
    @staticmethod
    def create(
        uri: str, sql_engine: Engine, parquet_root: Path, job_id: int = 0
    ) -> DataSink:
        parsed = urlparse(uri)
        scheme = parsed.scheme
        options = parse_qs(parsed.query)

        if scheme == "mssql":
            path = parsed.netloc + parsed.path
            return MssqlSink(sql_engine, path, options, job_id)

        elif scheme == "sqlite":
            db_path = parsed.netloc
            table_name = parsed.path.lstrip("/")
            return SqliteSink(db_path, table_name, options, job_id)

        elif scheme == "parquet":
            path = parsed.netloc + parsed.path
            clean_path = path.lstrip("/")
            return ParquetSink(parquet_root, clean_path, options, job_id)

        else:
            raise ValueError(f"Unsupported sink scheme: {scheme}")
