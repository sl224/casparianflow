from sqlalchemy.orm import declarative_base, sessionmaker
from casparian_flow.config import settings

# Conditionally set a default schema for MSSQL to keep tables organized.
if settings.database.type == "mssql":
    DEFAULT_SCHEMA = "casp"

    class CaspCoreBase:
        __table_args__ = {"schema": DEFAULT_SCHEMA}

    Base = declarative_base(cls=CaspCoreBase)
else:
    DEFAULT_SCHEMA = None
    Base = declarative_base()


def schema_fkey(key: str) -> str:
    """
    Returns a schema-qualified foreign key string if a schema is defined,
    otherwise returns the simple key.

    - MSSQL: "casparian_flow.table.column"
    - SQLite: "table.column"
    """
    if DEFAULT_SCHEMA:
        return f"{DEFAULT_SCHEMA}.{key}"
    return key


# A factory for creating new Session objects.
SessionLocal = sessionmaker(autocommit=False, autoflush=False)


def make_table_args(*constraints):
    """
    Helper to build __table_args__ with optional schema.
    Only includes schema key when DEFAULT_SCHEMA is not None.

    Usage:
        __table_args__ = make_table_args(Index(...), UniqueConstraint(...))
    Or:
        __table_args__ = make_table_args()  # Just schema, if applicable
    """
    if DEFAULT_SCHEMA:
        # For MSSQL: return tuple of constraints + dict with schema
        if constraints:
            return (*constraints, {"schema": DEFAULT_SCHEMA})
        else:
            return ({"schema": DEFAULT_SCHEMA},)
    else:
        # For SQLite: return just the constraints (no schema dict)
        if constraints:
            return constraints
        else:
            return {}  # Return empty dict instead of tuple for SQLite
