"""
Casparian Flow Configuration.

v5.0 Bridge Mode additions:
- SecurityConfig: AUTH_MODE and identity provider settings
- BridgeConfig: Venv caching and execution settings
"""
import sys
from typing import Union, Literal, Tuple, Callable, Type
from pathlib import Path

from pydantic import BaseModel, Field
from typing import Optional

from pydantic_settings import (
    BaseSettings,
    SettingsConfigDict,
    TomlConfigSettingsSource,
    InitSettingsSource,
    EnvSettingsSource,
    DotEnvSettingsSource,
    SecretsSettingsSource,
)

# Define the absolute path to the config file.
# This finds the directory where this file lives, then points to `global_config.toml`.
BASE_DIR = Path(__file__).resolve().parent.parent.parent
TOML_PATH = BASE_DIR / "global_config.toml"

# Add a debug check to warn if the config file is missing.
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


# Define the discriminated union for database configurations.
# Pydantic uses the 'type' field to select the correct model.


class SQLiteConfig(BaseSettings):
    type: Literal["sqlite3"] = "sqlite3"
    db_location: str = "casparian_flow.sqlite3"
    in_memory: bool = False


class MSSQLConfig(BaseModel):
    type: Literal["mssql"] = "mssql"
    server_name: str = "YOUR_SERVER_NAME"
    db_name: str = "YOUR_DATABASE_NAME"
    driver: str = "{ODBC Driver 17 for SQL Server}"
    trusted_connection: str = "no"

    # New fields (Optional, because Windows users won't need them)
    username: Optional[str] = None
    password: Optional[str] = None


# A type that can be any of the supported database configurations.
DatabaseConfig = Union[SQLiteConfig, MSSQLConfig]


# =============================================================================
# v5.0 Bridge Mode Configuration
# =============================================================================


class SecurityConfig(BaseModel):
    """
    Security settings for v5.0 Bridge Mode.

    Dual-Mode Authentication:
    - local: Zero friction, auto-generated keys, implicit trust
    - entra: Zero trust, Azure AD integration
    """

    auth_mode: Literal["local", "entra"] = "local"

    # Local mode settings
    api_key: Optional[str] = None  # Optional shared secret for local auth
    keys_dir: Optional[str] = None  # Directory for signing keys

    # Enterprise (Entra) mode settings
    azure_tenant_id: Optional[str] = None
    azure_client_id: Optional[str] = None
    publisher_group_oid: Optional[str] = None  # Casparian_Publishers security group


class BridgeConfig(BaseModel):
    """
    Bridge Mode execution settings.

    Controls isolated venv execution and Arrow IPC communication.
    """

    enabled: bool = True  # Enable Bridge Mode (False = Legacy only)

    # Venv cache settings
    venvs_dir: Optional[str] = None  # Default: ~/.casparian_flow/venvs/
    max_cache_size_gb: float = 10.0  # Max disk usage for venv cache

    # Execution settings
    execution_timeout_seconds: int = 300  # 5 minute default
    socket_timeout_seconds: int = 30  # Connection timeout

    # Python version constraint (None = use system default)
    python_version: Optional[str] = None  # e.g., "3.11"


class AppSettings(BaseSettings):
    """
    Main settings class that loads configuration from various sources.
    Uses defaults if the file or keys are missing.

    v5.0 additions:
    - security: Authentication and signing settings
    - bridge: Isolated execution settings
    """

    logging: LoggingConfig = LoggingConfig()
    database: DatabaseConfig = Field(default=SQLiteConfig(), discriminator="type")

    # v5.0 Bridge Mode
    security: SecurityConfig = SecurityConfig()
    bridge: BridgeConfig = BridgeConfig()

    model_config = SettingsConfigDict(
        extra="forbid",
    )

    @classmethod
    def settings_customise_sources(
        cls,
        settings_cls: Type[BaseSettings],
        init_settings: InitSettingsSource,
        env_settings: EnvSettingsSource,
        dotenv_settings: DotEnvSettingsSource,
        file_secret_settings: SecretsSettingsSource,
    ) -> Tuple[Callable, ...]:
        """
        Define the priority order for loading settings sources.
        Our custom TOML file is inserted with high priority.
        """
        return (
            init_settings,
            TomlConfigSettingsSource(settings_cls, toml_file=TOML_PATH),
            # The rest are the default sources
            env_settings,
            dotenv_settings,
            file_secret_settings,
        )


# Create a singleton instance of the settings to be used throughout the app.
try:
    settings = AppSettings()
except Exception as e:
    print(f"Failed to load configuration: {e}")
