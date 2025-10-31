import global_config
from sqlalchemy import create_engine
from sqlalchemy import URL

def get_engine(fast_executemany=True, echo=False):
    match global_config.use_db_type:
        case global_config.supported_databases.mssql:
            url_object = URL.create(
                drivername="mssql+pyodbc",
                host=global_config.server_name,
                database=global_config.db_name,
                query=dict(
                    driver=global_config.driver,
                    trusted_connection=global_config.trusted_connection
                )
            )
        case global_config.supported_databases.sqlite:
            url_object = f"sqlite://{global_config.db_location}"
    return create_engine(url_object, fast_executemany=fast_executemany, echo=echo)
