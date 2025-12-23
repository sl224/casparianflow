# src/casparian_flow/engine/sinks.py
import logging
import shutil
import os
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
    def __init__(
        self,
        engine: Engine,
        table_path: str,
        options: Dict,
        job_id: int,
        file_version_id: int,
    ):
        self.engine = engine
        self.job_id = job_id
        self.file_version_id = file_version_id

        if "." in table_path:
            self.schema, self.table_name = table_path.split(".", 1)
        else:
            self.schema = "dbo"
            self.table_name = table_path

        self.if_exists = options.get("mode", ["append"])[0]
        self.chunksize = int(options.get("chunksize", [10000])[0])

        # Staging Table Name
        self.staging_table = f"stg_{self.table_name}_{self.job_id}"

        # Application-side buffering to reduce transaction spam
        self.buffer = []
        self.buffer_row_count = 0

    def write(self, data: Any):
        if HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            data = data.to_pandas()

        if not isinstance(data, pd.DataFrame):
            return

        # Inject lineage columns
        data = data.copy()
        data["_job_id"] = self.job_id
        data["_file_version_id"] = self.file_version_id

        # Add to buffer
        self.buffer.append(data)
        self.buffer_row_count += len(data)

        # Flush if buffer exceeds chunksize
        if self.buffer_row_count >= self.chunksize:
            self._flush()

    def _flush(self):
        """Flush buffered data to database in a single transaction."""
        if not self.buffer:
            return

        # Concatenate all buffered DataFrames
        combined = pd.concat(self.buffer, ignore_index=True)
        self.buffer.clear()
        self.buffer_row_count = 0

        with self.engine.begin() as conn:
            combined.to_sql(
                name=self.staging_table,
                schema=self.schema,
                con=conn,
                if_exists="append",
                index=False,
                chunksize=self.chunksize,
            )

    def promote(self):
        # Flush any remaining buffered data before promoting
        self._flush()

        logger.info(f"Promoting {self.staging_table} to {self.table_name}")
        with self.engine.begin() as conn:
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
        # Flush any remaining buffered data on close
        self._flush()


class SqliteSink:
    def __init__(
        self,
        db_path: str,
        table_name: str,
        options: Dict,
        job_id: int,
        file_version_id: int,
    ):
        # Handle absolute windows paths correctly for sqlite url
        # If it's a file path, we need 3 slashes for relative, 4 for absolute?
        # Actually sqlalchemy create_engine handles sqlite:///C:/path fine.
        # We just need to ensure db_path is passed correctly.

        # If db_path comes from urlparse.netloc it might be empty for absolute paths
        # If we reconstruct it, we need to be careful.
        if os.name == "nt" and ":" in db_path and not db_path.startswith("/"):
            # It's a windows absolute path C:\..., sqlalchemy needs sqlite:///C:\...
            self.engine = create_engine(f"sqlite:///{db_path}")
        else:
            # Standard handling
            self.engine = create_engine(f"sqlite:///{db_path}")

        self.job_id = job_id
        self.file_version_id = file_version_id
        self.table_name = table_name
        self.staging_table = f"{table_name}_stg_{job_id}"
        self.if_exists = options.get("mode", ["append"])[0]
        self.chunksize = int(options.get("chunksize", [10000])[0])

        # Application-side buffering to reduce transaction spam
        self.buffer = []
        self.buffer_row_count = 0

    def write(self, data: Any):
        if HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            data = data.to_pandas()

        # Inject lineage columns
        data = data.copy()
        data["_job_id"] = self.job_id
        data["_file_version_id"] = self.file_version_id

        # Add to buffer
        self.buffer.append(data)
        self.buffer_row_count += len(data)

        # Flush if buffer exceeds chunksize
        if self.buffer_row_count >= self.chunksize:
            self._flush()

    def _flush(self):
        """Flush buffered data to database in a single transaction."""
        if not self.buffer:
            return

        # Concatenate all buffered DataFrames
        combined = pd.concat(self.buffer, ignore_index=True)
        self.buffer.clear()
        self.buffer_row_count = 0

        with self.engine.begin() as conn:
            combined.to_sql(
                name=self.staging_table,
                con=conn,
                if_exists="append",
                index=False,
                chunksize=self.chunksize,
            )

    def promote(self):
        # Flush any remaining buffered data before promoting
        self._flush()

        with self.engine.begin() as conn:
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
        # Flush any remaining buffered data on close
        self._flush()
        self.engine.dispose()


class ParquetSink:
    def __init__(
        self,
        root_path: Path,
        relative_path: str,
        options: Dict,
        job_id: int,
        file_version_id: int,
    ):
        self.job_id = job_id
        self.file_version_id = file_version_id

        # Inject job_id into filename for concurrency safety
        # Example: "output.parquet" -> "output_{job_id}.parquet"
        path_obj = Path(relative_path)
        if path_obj.suffix:
            # Has extension (e.g., "output.parquet")
            stem = path_obj.stem  # "output"
            suffix = path_obj.suffix  # ".parquet"
            parent = path_obj.parent  # Path('.')
            unique_filename = f"{stem}_{job_id}{suffix}"
            final_relative_path = parent / unique_filename
        else:
            # No extension (directory path)
            final_relative_path = path_obj / f"data_{job_id}.parquet"

        self.final_path = root_path / final_relative_path
        self.staging_path = root_path / f"{final_relative_path}.stg.{job_id}"

        self.staging_path.parent.mkdir(parents=True, exist_ok=True)
        self.compression = options.get("compression", ["snappy"])[0]
        self.writer = None

    def write(self, data: Any):
        table = None
        if isinstance(data, pd.DataFrame):
            # Inject lineage columns for pandas DataFrame
            data = data.copy()
            data["_job_id"] = self.job_id
            data["_file_version_id"] = self.file_version_id

            if HAS_ARROW:
                table = pa.Table.from_pandas(data)
            else:
                data.to_parquet(
                    self.staging_path, compression=self.compression, index=False
                )
                return
        elif HAS_ARROW and isinstance(data, (pa.Table, pa.RecordBatch)):
            # Convert to table first if it's a RecordBatch
            if isinstance(data, pa.RecordBatch):
                table = pa.Table.from_batches([data])
            else:
                table = data

            # Inject lineage columns for PyArrow Table
            table = table.append_column(
                "_job_id", pa.array([self.job_id] * len(table), type=pa.int64())
            )
            table = table.append_column(
                "_file_version_id",
                pa.array([self.file_version_id] * len(table), type=pa.int64()),
            )

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
            # Move staging file to final location
            # final_path might be a file or directory
            if self.final_path.suffix:
                # final_path is a file (e.g., output.parquet)
                target = self.final_path
                target.parent.mkdir(parents=True, exist_ok=True)
            else:
                # final_path is a directory (e.g., output/)
                target = self.final_path / self.staging_path.stem.split('.stg')[0]
                target.mkdir(parents=True, exist_ok=True)

            shutil.move(str(self.staging_path), str(target))

    def close(self):
        if self.writer:
            self.writer.close()


class SinkFactory:
    @staticmethod
    def create(
        uri: str,
        sql_engine: Engine,
        parquet_root: Path,
        job_id: int = 0,
        file_version_id: int = 0,
    ) -> DataSink:
        parsed = urlparse(uri)
        scheme = parsed.scheme
        options = parse_qs(parsed.query)

        if scheme == "mssql":
            path = parsed.netloc + parsed.path
            return MssqlSink(sql_engine, path, options, job_id, file_version_id)

        elif scheme == "sqlite":
            # Parsing logic for sqlite:///path/to/db/table
            # If netloc is empty (absolute path), path is /C:/.../db/table
            if not parsed.netloc:
                full_path = parsed.path
                # Split at last slash to separate DB file from table name
                # This assumes table name cannot contain slashes, which is true for SQL tables
                db_path_str, table_name = full_path.rsplit("/", 1)

                # Cleanup leading slash if on Windows and it looks like /C:/...
                if (
                    os.name == "nt"
                    and db_path_str.startswith("/")
                    and ":" in db_path_str
                ):
                    db_path_str = db_path_str.lstrip("/")

                return SqliteSink(db_path_str, table_name, options, job_id, file_version_id)
            else:
                # Relative path: sqlite://file.db/table
                db_path = parsed.netloc
                table_name = parsed.path.lstrip("/")
                return SqliteSink(db_path, table_name, options, job_id, file_version_id)

        elif scheme == "parquet":
            path = parsed.netloc + parsed.path
            clean_path = path.lstrip("/")
            return ParquetSink(parquet_root, clean_path, options, job_id, file_version_id)

        else:
            raise ValueError(f"Unsupported sink scheme: {scheme}")
