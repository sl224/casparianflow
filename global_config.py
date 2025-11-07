import sys
from typing import Union, Literal
from pydantic import BaseModel, Field
from pydantic_settings import BaseSettings, SettingsConfigDict

# --- 1. Define the "Blueprints" (BaseModel) ---
# These are just like your dataclasses, but with validation.

class LoggingConfig(BaseModel):
    level: str = "INFO"
    log_to_file: bool = False
    log_file: str = "scan_job.log"
    rotation_size_mb: int = 10
    rotation_backup_count: int = 5
    format: str = "%(asctime)s - %(name)s - %(levelname)s - %(message)s"

class ScanConfig(BaseModel):
    dir: str = ""

# --- 2. Define the Discriminated Union for the Database ---
# This is the magic. We add a 'type' field to each model
# so Pydantic can tell them apart.

class SQLiteConfig(BaseModel):
    type: Literal["sqlite3"] = "sqlite3"
    db_location: str = "./caspary.sqlite3"
    in_memory: bool = False

class MSSQLConfig(BaseModel):
    type: Literal["mssql"] = "mssql"
    server_name: str = "YOUR_SERVER_NAME"
    db_name: str = "YOUR_DATABASE_NAME"
    driver: str = "{ODBC Driver 17 for SQL Server}"
    trusted_connection: str = "yes"

class DuckDBConfig(BaseModel):
    type: Literal["duckdb"] = "duckdb"
    db_location: str = "./caspary.duckdb"
    in_memory: bool = False

# Create a new type that can be ANY of the above configs
DatabaseConfig = Union[SQLiteConfig, MSSQLConfig, DuckDBConfig]


# --- 3. Define the Main "Loader" (BaseSettings) ---
# This class automatically finds and loads the TOML file.

class AppSettings(BaseSettings):
    """
    Loads all application settings from the TOML file.
    Uses defaults if the file or keys are missing.
    """
    
    # Pydantic will look for a [logging] section in the TOML
    logging: LoggingConfig = LoggingConfig()
    
    # Pydantic will look for a [scan] section in the TOML
    scan: ScanConfig = ScanConfig()
    
    # Pydantic will look for a [database] section.
    # The `discriminator` tells it to read the 'type' field
    # first, then pick the correct model from the Union.
    database: DatabaseConfig = Field(default=SQLiteConfig(), discriminator="type")

    # This tells Pydantic to find "global_config.toml"
    # and use it to populate the fields above.
    model_config = SettingsConfigDict(
        toml_file="global_config.toml",
        # This will print warnings for unknown keys, just like your code
        # You can change to 'strict' to raise an error
        extra='ignore' 
    )

# --- 4. Create the Singleton Instance ---
# You just create it once here and import it everywhere else.
# All the loading logic from your __init__ and load() is
# automatically run right here.
try:
    settings = AppSettings()
except Exception as e:
    print(f"Failed to load configuration: {e}")
    # Handle critical error, as the app can't run
    sys.exit(1)