"""
Worker configuration models.
"""

from pydantic import BaseModel, Field, ConfigDict
from pathlib import Path


class DatabaseConfig(BaseModel):
    """Database connection configuration."""

    connection_string: str = Field(..., description="SQLAlchemy connection string")


class StorageConfig(BaseModel):
    """Storage configuration for sinks."""

    parquet_root: Path = Field(
        default=Path("data/parquet"),
        description="Root directory for parquet output files",
    )


class PluginsConfig(BaseModel):
    """Plugin discovery configuration."""

    dir: Path = Field(
        default=Path("tests/fixtures/plugins"),
        description="Directory containing plugin files",
    )


class WorkerConfig(BaseModel):
    """
    Complete configuration for a Casparian Worker node.
    """

    database: DatabaseConfig
    storage: StorageConfig = Field(default_factory=StorageConfig)
    plugins: PluginsConfig = Field(default_factory=PluginsConfig)

    model_config = ConfigDict(arbitrary_types_allowed=True)
