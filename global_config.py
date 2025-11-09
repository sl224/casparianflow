import sys
from typing import Union, Literal, Tuple, Callable, Type
from pathlib import Path  # <-- Import Path

from pydantic import BaseModel, Field

from pydantic_settings import (
    BaseSettings,
    SettingsConfigDict,
    TomlConfigSettingsSource,
    InitSettingsSource,
    EnvSettingsSource,
    DotEnvSettingsSource,
    SecretsSettingsSource,
)

# --- Define the absolute path to the config file ---
# This finds the directory where 'global_config.py' lives
BASE_DIR = Path(__file__).resolve().parent
# This creates a full, absolute path to 'global_config.toml'
TOML_PATH = BASE_DIR / "global_config.toml"
# ---

# --- Add a debug check ---
if not TOML_PATH.is_file():
    print(f"WARNING: Config file not found at path: {TOML_PATH}", file=sys.stderr)
    print(f"WARNING: Current working directory: {Path.cwd()}", file=sys.stderr)
# ---


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
# ... existing code ...


class SQLiteConfig(BaseSettings):
    type: Literal["sqlite3"] = "sqlite3"
    db_location: str = "./caspary.sqlite3"
    in_memory: bool = False


class MSSQLConfig(BaseModel):
    type: Literal["mssql"] = "mssql"  # <-- FIX: Added missing discriminator field
    server_name: str = "YOUR_SERVER_NAME"
    db_name: str = "YOUR_DATABASE_NAME"
    driver: str = "{ODBC Driver 17 for SQL Server}"
    trusted_connection: str = "yes"


class DuckDBConfig(BaseModel):
    type: Literal["duckdb"] = "duckdb"  # <-- FIX: Added missing discriminator field
    db_location: str = "./caspary.duckdb"
    in_memory: bool = False


# Create a new type that can be ANY of the above configs
DatabaseConfig = Union[SQLiteConfig, MSSQLConfig, DuckDBConfig]


# --- 3. Define the Main "Loader" (BaseSettings) ---
# ... existing code ...


class AppSettings(BaseSettings):
    """
    Loads all application settings from the TOML file.
    Uses defaults if the file or keys are missing.
    """

    # --- Re-adding defaults to prevent crash ---
    # The TOML file is not loading, so we prevent the app
    # from crashing by providing defaults again.
    logging: LoggingConfig = LoggingConfig()
    scan: ScanConfig = ScanConfig()
    # ---

    database: DatabaseConfig = Field(default=SQLiteConfig(), discriminator="type")

    model_config = SettingsConfigDict(
        # toml_file=TOML_PATH,  <-- This key is deprecated and causes the warning
        extra="forbid",
    )

    # --- FIX: Update the method signature and return value ---
    @classmethod
    def settings_customise_sources(
        cls,
        settings_cls: Type[BaseSettings],
        init_settings: InitSettingsSource,
        env_settings: EnvSettingsSource,
        dotenv_settings: DotEnvSettingsSource,
        # FIX: The library is *calling* with 'file_secret_settings'
        # so we must *accept* that argument name.
        file_secret_settings: SecretsSettingsSource,
    ) -> Tuple[Callable, ...]:  # <-- FIX: Use 'Callable' from typing
        """
        Define the priority order for loading settings.
        We are inserting our TOML file source right after
        the initial settings.
        """
        return (
            init_settings,
            # Add our TOML file as a source
            TomlConfigSettingsSource(settings_cls, toml_file=TOML_PATH),
            # The rest are the default sources
            env_settings,
            dotenv_settings,
            file_secret_settings,  # <-- FIX: Return the same argument
        )


# --- 4. Create the Singleton Instance ---
# ... existing code ...
try:
    settings = AppSettings()
    # If you want to verify, uncomment this temporarily:
    # print(f"--- DEBUG: Loading config from {TOML_PATH} ---")
    # print(settings.model_dump_json(indent=2))
    # print("-----------------------------------------------")
except Exception as e:
    print(f"Failed to load configuration: {e}")
