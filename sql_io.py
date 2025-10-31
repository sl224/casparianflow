from global_config import settings, supported_databases
from sqlalchemy import create_engine
from sqlalchemy import URL

def get_engine(fast_executemany=True, echo=False):
    engine_args = {"echo": echo}
    
    match settings.db_type:
        case supported_databases.mssql:
            # FIX: Use settings.db, not settings.mssql
            mssql_settings = settings.db 
            url_object = URL.create(
                drivername="mssql+pyodbc",
                host=mssql_settings.server_name,
                database=mssql_settings.db_name,
                query={
                    "driver": mssql_settings.driver,
                    "trusted_connection": mssql_settings.trusted_connection,
                },
            )
            engine_args["fast_executemany"] = fast_executemany
            
        case supported_databases.sqlite3:
            # FIX: Use settings.db, not settings.sqlite
            sqlite_settings = settings.db
            if sqlite_settings.in_memory:
                url_object = "sqlite:///:memory:"
            else:
                url_object = f"sqlite:///{sqlite_settings.db_location}"

        case supported_databases.duckdb: # <--- ADDED BLOCK
            duckdb_settings = settings.db
            if duckdb_settings.in_memory:
                url_object = "duckdb:///:memory:"
            else:
                # DuckDB dialect uses '///' for file paths
                url_object = f"duckdb:///{duckdb_settings.db_location}"
            # fast_executemany is not applicable
    print("using {settings.db_type} ")
    return create_engine(url_object, **engine_args)