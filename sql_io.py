# You import the settings *instance* from pydantic_settings.py
from global_config import settings
from sqlalchemy import create_engine
from sqlalchemy import URL


def get_engine(fast_executemany=True, echo=False):
    """
    Creates and returns a SQLAlchemy engine based on the loaded pydantic settings.
    """
    engine_args = {"echo": echo}
    url_object = None  # Ensure url_object is defined

    # We match on the 'type' *string* attribute inside settings.database
    match settings.database.type:
        case "mssql":
            # settings.database is ALREADY the MSSQLConfig object
            # We can access its attributes directly.
            url_object = URL.create(
                drivername="mssql+pyodbc",
                host=settings.database.server_name,
                database=settings.database.db_name,
                query={
                    "driver": settings.database.driver,
                    "trusted_connection": settings.database.trusted_connection,
                },
            )
            engine_args["fast_executemany"] = fast_executemany

        case "sqlite3":
            # settings.database is the SQLiteConfig object
            if settings.database.in_memory:
                url_object = "sqlite:///:memory:"
            else:
                url_object = f"sqlite:///{settings.database.db_location}"

        case "duckdb":
            # settings.database is the DuckDBConfig object
            if settings.database.in_memory:
                url_object = "duckdb:///:memory:"
            else:
                url_object = f"duckdb:///{settings.database.db_location}"
        
        case _:
            # Handle any unexpected database types
            raise ValueError(f"Unknown or unsupported database type: {settings.database.type}")

    if url_object is None:
        raise ValueError("Database URL object was not created. Check configuration.")

    return create_engine(url_object, **engine_args)