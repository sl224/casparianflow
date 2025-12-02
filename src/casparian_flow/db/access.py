import logging
import numpy as np
import pandas as pd
from sqlalchemy import URL, Connection, Table, create_engine
from tqdm import tqdm

logger = logging.getLogger(__name__)


class BadParameter(ValueError):
    pass


# from sqlalchemy import create_engine
# from sqlalchemy.engine import URL

def get_engine(db_settings, fast_executemany: bool = True, echo: bool = False, pool_size=10, pool_pre_ping=True):
    """
    Creates and returns a SQLAlchemy engine based on the loaded pydantic settings.
    Supports both Windows Auth (Trusted) and SQL Auth (User/Pass).
    """
    engine_args = {"echo": echo}
    engine_args["pool_size"] = pool_size
    engine_args["pool_pre_ping"] = pool_pre_ping

    url_object = None

    match db_settings.type:
        case "mssql":
            # 1. Base query parameters always require the driver
            query_params = {"driver": db_settings.driver}
            
            # 2. Determine Authentication Method
            # Check if trusted_connection is explicitly "yes" (Windows behavior)
            is_trusted = str(getattr(db_settings, "trusted_connection", "no")).lower() == "yes"

            if is_trusted:
                # Windows Authentication
                query_params["trusted_connection"] = "yes"
                db_user = None
                db_pass = None
            else:
                # SQL Authentication (Mac/Linux)
                # Ensure we DO NOT pass 'trusted_connection' in query params here
                db_user = db_settings.username
                db_pass = db_settings.password

            # 3. Create URL Object
            url_object = URL.create(
                drivername="mssql+pyodbc",
                username=db_user,      # SQL Alchemy handles None gracefully here
                password=db_pass,      # SQL Alchemy handles None gracefully here
                host=db_settings.server_name,
                database=db_settings.db_name,
                query=query_params,
            )
            
            engine_args["fast_executemany"] = fast_executemany

        case "sqlite3":
            if db_settings.in_memory:
                url_object = "sqlite:///:memory:"
            else:
                url_object = f"sqlite:///{db_settings.db_location}"

        case _:
            raise ValueError(
                f"Unsupported DB type: {db_settings.type}"
            )

    if url_object is None:
        raise ValueError("Database URL object was not created. Check configuration.")

    return create_engine(url_object, **engine_args)


def bulk_upload(
    df: pd.DataFrame,
    conn: Connection,
    sa_table: Table,
    chunksize: int = 2000,
    tqdm_description: str = "Uploading",
    show_progress: bool = False,
    leave: bool = True,
):
    """
    Uploads a DataFrame to a database table in chunks.
    """
    if df.empty:
        return

    total_rows = len(df)

    with tqdm(
        total=total_rows,
        desc=tqdm_description,
        unit="rows",
        leave=leave,
        disable=not show_progress,
    ) as pbar:
        # Slice via `iloc` for memory-efficient chunking.
        for start_idx in range(0, total_rows, chunksize):
            df_chunk = df.iloc[start_idx : start_idx + chunksize]

            # Sanitize chunk just before upload, converting pandas/numpy nulls to SQL NULL.
            clean_chunk = df_chunk.replace({np.nan: None, pd.NA: None})

            conn.execute(
                sa_table.insert(),
                clean_chunk.to_dict(orient="records"),
            )
            pbar.update(len(df_chunk))
