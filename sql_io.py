from global_config import settings
from sqlalchemy import create_engine
from sqlalchemy import URL


def get_engine(db_settings, fast_executemany=True, echo=False):
    """
    Creates and returns a SQLAlchemy engine based on the loaded pydantic settings.
    """
    engine_args = {"echo": echo}
    url_object = None  # Ensure url_object is defined

    # We match on the 'type' *string* attribute insidedb_settings
    match db_settings.type:
        case "mssql":
            url_object = URL.create(
                drivername="mssql+pyodbc",
                host=db_settings.server_name,
                database=db_settings.db_name,
                query={
                    "driver": db_settings.driver,
                    "trusted_connection": db_settings.trusted_connection,
                },
            )
            engine_args["fast_executemany"] = fast_executemany

        case "sqlite3":
            if db_settings.in_memory:
                url_object = "sqlite:///:memory:"
            else:
                url_object = f"sqlite:///{db_settings.db_location}"

        case "duckdb":
            if db_settings.in_memory:
                url_object = "duckdb:///:memory:"
            else:
                url_object = f"duckdb:///{db_settings.db_location}"

        case _:
            raise ValueError(
                f"Pydantic should have thrown an error for unsupported DB types"
            )

    if url_object is None:
        raise ValueError("Database URL object was not created. Check configuration.")

    return create_engine(url_object, **engine_args)
