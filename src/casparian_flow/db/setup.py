import logging
import hashlib
import sys
from pathlib import Path
from typing import List
import sqlalchemy as sa
from sqlalchemy import inspect
from sqlalchemy.schema import CreateSchema

from casparian_flow.config import settings
from casparian_flow.db.base_session import Base, DEFAULT_SCHEMA
import casparian_flow.db.models  # noqa: F401
from casparian_flow.db.models import SourceRoot

logger = logging.getLogger(__name__)

# --- Schema Fingerprinting ---


def compute_schema_fingerprint(eng: sa.Engine) -> str:
    """
    Compute a fingerprint of the database schema by inspecting live structure.

    Industry approach: Query the actual database structure and hash a canonical
    representation. This is immune to code formatting/whitespace changes.

    Returns a SHA-256 hash of: sorted(table_name:column_name:column_type)
    """
    inspector = inspect(eng)

    # Get schema for MSSQL, None for SQLite
    schema = DEFAULT_SCHEMA if settings.database.type == "mssql" else None

    # Build canonical representation
    schema_parts = []

    for table_name in sorted(inspector.get_table_names(schema=schema)):
        columns = inspector.get_columns(table_name, schema=schema)
        for col in sorted(columns, key=lambda c: c["name"]):
            # Normalize type to string for cross-db compatibility
            col_type = str(col["type"]).upper()
            # Format: table:column:TYPE
            schema_parts.append(f"{table_name}:{col['name']}:{col_type}")

    # Join and hash
    canonical = "\n".join(schema_parts)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def verify_schema_integrity(eng: sa.Engine, expected_fingerprint: str = None):
    """
    Verify the database schema matches expected structure.

    Two modes:
    1. If expected_fingerprint is provided: strict validation (production)
    2. If None: logs current fingerprint for recording (development)
    """
    current_fingerprint = compute_schema_fingerprint(eng)

    if expected_fingerprint is None:
        # Development mode - just log the fingerprint
        logger.info(f"Current schema fingerprint: {current_fingerprint}")
        return current_fingerprint

    if current_fingerprint != expected_fingerprint:
        logger.critical(
            f"SCHEMA DRIFT DETECTED!\n"
            f"  Expected: {expected_fingerprint}\n"
            f"  Current:  {current_fingerprint}"
        )
        raise RuntimeError("Database schema fingerprint mismatch. Schema has drifted.")

    logger.info(f"Schema fingerprint verified: {current_fingerprint[:16]}...")
    return current_fingerprint


def verify_database_state(eng: sa.Engine):
    """
    Check that all expected tables and columns exist in the database.
    This validates code models match the live database.
    """
    inspector = inspect(eng)

    # Get schema for MSSQL, None for SQLite
    schema = DEFAULT_SCHEMA if settings.database.type == "mssql" else None
    db_tables = set(inspector.get_table_names(schema=schema))

    # Check Model Definitions vs Database
    missing_tables = []
    missing_columns = []

    for name, table in Base.metadata.tables.items():
        table_name = table.name

        if table_name not in db_tables:
            missing_tables.append(table_name)
            continue

        # Check Columns
        db_cols = {c["name"] for c in inspector.get_columns(table_name, schema=schema)}

        for column in table.columns:
            if column.name not in db_cols:
                missing_columns.append(f"{table_name}.{column.name}")

    if missing_tables or missing_columns:
        logger.critical("CRITICAL: Database schema drift detected!")
        if missing_tables:
            logger.critical(f"Missing Tables: {missing_tables}")
        if missing_columns:
            logger.critical(f"Missing Columns: {missing_columns}")

        raise RuntimeError(
            "Database integrity violation. Schema does not match application version."
        )
    else:
        logger.info("Database schema validated successfully.")


# --- Initialization ---


def initialize_database(eng: sa.Engine, reset_tables: bool = False):
    """
    Ensures the necessary database schema exists (for MSSQL)
    and optionally resets all tables.
    """
    # 1. Schema Creation (MSSQL-specific)
    if settings.database.type == "mssql":
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

    # 3. Check Database Integrity (Post-Creation)
    verify_database_state(eng)

    # 4. Log Schema Fingerprint (for development/auditing)
    fingerprint = compute_schema_fingerprint(eng)
    logger.info(f"Schema fingerprint: {fingerprint[:16]}...")


def seed_library_whitelist(eng: sa.Engine):
    """
    Seed the LibraryWhitelist table with commonly used Python libraries.
    Safe to call multiple times - uses INSERT OR IGNORE for SQLite.
    """
    from casparian_flow.db.models import LibraryWhitelist
    from sqlalchemy.orm import Session

    initial_libraries = [
        ("pandas", ">=2.0.0", "DataFrame processing"),
        ("pyarrow", ">=22.0.0", "Parquet and Arrow operations"),
        ("numpy", ">=1.26.0", "Numerical computing"),
        ("sqlalchemy", ">=2.0.0", "Database operations"),
        ("pydantic", ">=2.0.0", "Data validation"),
        ("openpyxl", ">=3.0.0", "Excel file handling"),
        ("pypdf", ">=5.0.0", "PDF file processing"),
    ]

    with Session(eng) as session:
        for lib_name, version, desc in initial_libraries:
            # Check if library already exists
            existing = (
                session.query(LibraryWhitelist).filter_by(library_name=lib_name).first()
            )
            if not existing:
                lib = LibraryWhitelist(
                    library_name=lib_name, version_constraint=version, description=desc
                )
                session.add(lib)
                logger.info(f"Seeded library: {lib_name} {version}")

        session.commit()
        logger.info(
            f"LibraryWhitelist seeding complete. Total libraries: {len(initial_libraries)}"
        )


def get_or_create_sourceroot(eng: sa.Engine, path: str, type: str = "local") -> int:
    from sqlalchemy import select

    with eng.connect() as conn:
        stmt = select(SourceRoot.id).where(SourceRoot.path == path)
        existing_id = conn.execute(stmt).scalar()

        if existing_id:
            return existing_id

        try:
            result = conn.execute(
                SourceRoot.__table__.insert().values(path=path, type=type, active=1)
            )
            conn.commit()
            return conn.execute(stmt).scalar()
        except sa.exc.IntegrityError:
            conn.rollback()
            return conn.execute(stmt).scalar()
