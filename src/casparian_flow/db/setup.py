import logging
import sqlalchemy as sa
from sqlalchemy.schema import CreateSchema

# FIX: Import from casparian_flow, not etude_core
from casparian_flow.config import settings
from casparian_flow.db.base_session import Base, DEFAULT_SCHEMA

# FIX: Import the NEW models to ensure they are registered with Base.metadata
import casparian_flow.db.models  # noqa: F401
from casparian_flow.db.models import SourceRoot

logger = logging.getLogger(__name__)

def initialize_database(eng: sa.Engine, reset_tables: bool = False):
    """
    Ensures the necessary database schema exists (for MSSQL)
    and optionally resets all tables.
    """
    # 1. Schema Creation (MSSQL-specific)
    if settings.database.type == "mssql":
        # FIX: Hardcode or read from config, don't rely on deleted constant if possible
        # Or ensure DEFAULT_SCHEMA is in base_session.py
        DEFAULT_SCHEMA = "casparian_core" 

        logger.info(f"Ensuring MSSQL schema '{DEFAULT_SCHEMA}' exists...")
        with eng.connect() as conn:
            if not conn.dialect.has_schema(conn, DEFAULT_SCHEMA):
                conn.execute(CreateSchema(DEFAULT_SCHEMA))
                logger.info(f"Schema '{DEFAULT_SCHEMA}' created.")
            conn.commit()

    # 2. Table Creation / Reset
    if reset_tables:
        logger.info("Resetting and creating database tables...")
        Base.metadata.drop_all(eng)
        Base.metadata.create_all(eng)
    else:
        logger.info("Ensuring all tables exist (create if not present)...")
        Base.metadata.create_all(eng)

def get_or_create_sourceroot(eng: sa.Engine, path: str, type: str = "local") -> int:
    """
    Idempotently registers a SourceRoot.
    Returns the ID.
    """
    from sqlalchemy import select
    
    with eng.connect() as conn:
        # Check exist
        stmt = select(SourceRoot.id).where(SourceRoot.path == path)
        existing_id = conn.execute(stmt).scalar()
        
        if existing_id:
            return existing_id
            
        # Create
        try:
            result = conn.execute(
                SourceRoot.__table__.insert().values(path=path, type=type, active=1)
            )
            conn.commit()
            # In MSSQL with pyodbc, result.inserted_primary_key might behave differently.
            # Safe fallback: re-query
            return conn.execute(stmt).scalar()
        except sa.exc.IntegrityError:
            conn.rollback()
            return conn.execute(stmt).scalar()