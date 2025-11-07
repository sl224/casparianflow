import sys
from enum import Enum, auto
from pathlib import Path
from dataclasses import dataclass, fields
from typing import Set, Union
import tomllib


class supported_databases(Enum):
    """Enumeration for supported database types."""

    sqlite3 = auto()
    mssql = auto()
    duckdb = auto()


@dataclass
class SQLiteConfig:
    """Configuration specific to SQLite."""

    db_location: str = "./caspary.sqlite3"
    in_memory: bool = False


@dataclass
class MSSQLConfig:
    """Configuration specific to MSSQL."""

    server_name: str = "YOUR_SERVER_NAME"
    db_name: str = "YOUR_DATABASE_NAME"
    driver: str = "{ODBC Driver 17 for SQL Server}"
    trusted_connection: str = "yes"


@dataclass
class DuckDBConfig:
    """Configuration specific to DuckDB."""

    db_location: str = "./caspary.duckdb"
    in_memory: bool = False


@dataclass
class LoggingConfig:
    """Configuration for application logging."""

    level: str = "INFO"
    log_to_file: bool = False
    log_file: str = "scan_job.log"
    rotation_size_mb: int = 10
    rotation_backup_count: int = 5
    format: str = "%(asctime)s - %(name)s - %(levelname)s - %(message)s"


class Config:
    """
    A singleton class to manage application configuration.
    It loads settings from a TOML file, falling back to defaults.
    """

    _instance = None

    def __new__(cls):
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._initialized = False
        return cls._instance

    def __init__(self):
        """Initializes the config with default values."""
        if self._initialized:
            return

        self.db_type = supported_databases.sqlite3
        self.db = SQLiteConfig()
        self.logging = LoggingConfig()

        self._initialized = True

    def load(self, config_path: str | Path = "global_config.toml"):
        """Loads configuration from a TOML file, overriding defaults."""
        try:
            # --- MODIFIED: Use tomllib and read in binary 'rb' mode ---
            with open(config_path, "rb") as f:
                config_data = tomllib.load(f)
        except FileNotFoundError:
            print(
                f"Warning: Config file '{config_path}' not found. Using default settings."
            )
            return  # Defaults from __init__ are already set
        except Exception as e:
            print(
                f"Error reading config file '{config_path}': {e}. Using default settings."
            )
            return  # Defaults from __init__ are already set

        if not config_data:
            print(
                f"Warning: Config file '{config_path}' is empty. Using default settings."
            )
            return  # Defaults from __init__ are already set

        # --- Override Logging ---
        logging_dict = config_data.get("logging", {})
        self._populate_config_object(logging_dict, self.logging)

        # --- Override Database ---
        db_config_section = config_data.get("database", {})
        self._load_database_settings(db_config_section)

    def _load_database_settings(self, db_section: dict):
        """
        Helper to parse the 'database' section and override defaults.
        """
        # 1. Determine which database type is being used
        # Use the existing self.db_type.name as default if 'type' isn't in config
        db_type_str = db_section.get("type", self.db_type.name).lower()

        try:
            new_db_type = supported_databases[db_type_str]
        except KeyError:
            print(
                f"Warning: Invalid database 'type' '{db_type_str}' in config. Using default '{self.db_type.name}'."
            )
            return  # Keep the default db object from __init__

        # 2. If type is different from default, create a new config object
        #    This ensures we have the correct default values for the new type
        if new_db_type != self.db_type:
            self.db_type = new_db_type
            if self.db_type == supported_databases.sqlite3:
                self.db = SQLiteConfig()
            elif self.db_type == supported_databases.mssql:
                self.db = MSSQLConfig()
            elif self.db_type == supported_databases.duckdb:
                self.db = DuckDBConfig()
            else:
                # This should be unreachable if enum is correct
                print(
                    f"Error: Unhandled database type '{self.db_type}'. Reverting to defaults."
                )
                self.db_type = supported_databases.sqlite3
                self.db = SQLiteConfig()

        # 3. Populate the *active* config object with its specific section
        #    e.g., if type is 'sqlite3', populate from [database.sqlite3]
        active_db_config_dict = db_section.get(db_type_str, {})
        self._populate_config_object(active_db_config_dict, self.db)

    def _populate_config_object(self, config_data: dict, config_obj):
        """
        Helper to populate a dataclass object from a dictionary.
        """
        if not config_data:
            return

        valid_fields: Set[str] = {f.name for f in fields(config_obj)}

        for key, value in config_data.items():
            if key in valid_fields:
                setattr(config_obj, key, value)
            else:
                print(
                    f"Warning: Unknown config key '{key}' in section '{config_obj.__class__.__name__}'. Ignoring."
                )


# Singleton instance
settings = Config()
