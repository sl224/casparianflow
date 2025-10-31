import yaml
from enum import Enum, auto
from pathlib import Path
from dataclasses import dataclass, fields, field
from typing import Set, Union

class supported_databases(Enum):
    """Enumeration for supported database types."""
    mssql = auto()
    sqlite3 = auto()
    duckdb = auto() # <--- ADDED

@dataclass
class SQLiteConfig:
    """Configuration specific to SQLite."""
    db_location: str = "./caspary.sqlite3"
    in_memory: bool = False

@dataclass
class MSSQLConfig:
    """Configuration specific to MS SQL Server."""
    server_name: str = "localhost"
    db_name: str = "master"
    driver: str = "{ODBC Driver 17 for SQL Server}"
    trusted_connection: str = "yes"

@dataclass
class DuckDBConfig: # <--- ADDED
    """Configuration specific to DuckDB."""
    db_location: str = "./caspary.duckdb"
    in_memory: bool = False

# Type hint for the active database config
ActiveDBConfig = Union[SQLiteConfig, MSSQLConfig, DuckDBConfig] # <--- UPDATED

@dataclass
class LoggingConfig:
    """Configuration for application logging."""
    level: str = "INFO"
    file: str | None = None # e.g., "app.log"

class Config:
    """
    A singleton class to manage application configuration.
    It loads settings from a YAML file, falling back to defaults.
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
            
        # --- Default Configuration Values (DuckDB is now default) ---
        self.db_type: supported_databases = None # <--- CHANGED
        self.db: ActiveDBConfig = None # <--- CHANGED
        self.logging: LoggingConfig = None 
        
        self._initialized = True

    def load(self, config_path: str | Path = 'global_config.yaml'):
        """Loads configuration from a YAML file, overriding defaults."""
        try:
            with open(config_path, 'r') as f:
                config_data = yaml.safe_load(f)
        except FileNotFoundError:
            print(f"Warning: Config file '{config_path}' not found. Using default settings.")
            self._load_database_settings({}) 
            return
        except Exception as e:
            print(f"Error reading config file '{config_path}': {e}. Using default settings.")
            self._load_database_settings({})
            return

        if not config_data:
            raise Exception("Empty configuration fiel")

        db_config = config_data.get('database', {})
        print(db_config)
        self._load_database_settings(db_config)
        
        # self._populate_config_object(config_data.get('logging', {}), self.logging)

    def _load_database_settings(self, db_config: dict):
        """
        Helper to parse the 'database' section.
        """
        # 1. Determine which database type is being used
        db_type_str = db_config['type'].lower()
        print("Type: ", db_type_str)
        try:
            self.db_type = supported_databases[db_type_str]
        except KeyError:
            print(f"Warning: Invalid database 'type' '{db_type_str}' in config. Using default '{self.db_type.name}'.")

        # 2. Instantiate and populate *only* the active config object
        if self.db_type == supported_databases.sqlite3:
            self.db = SQLiteConfig()
            self._populate_config_object(db_config.get('sqlite3', {}), self.db)
        
        elif self.db_type == supported_databases.mssql:
            self.db = MSSQLConfig()
            self._populate_config_object(db_config.get('mssql', {}), self.db)
            
        elif self.db_type == supported_databases.duckdb: # <--- ADDED BLOCK
            self.db = DuckDBConfig()
            self._populate_config_object(db_config.get('duckdb', {}), self.db)
            
        else:
            print(f"Error: Unhandled database type '{self.db_type}'. Using defaults.")
            self.db = DuckDBConfig() # <--- CHANGED FALLBACK
            self.db_type = supported_databases.duckdb


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
                print(f"Warning: Unknown config key '{key}' in section '{config_obj.__class__.__name__}'. Ignoring.")


settings = Config()