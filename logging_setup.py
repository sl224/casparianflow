# logging_setup.py
import logging
import sys
from logging.handlers import RotatingFileHandler
from pathlib import Path


def setup_logging(config):
    """
    Configures the root logger based on the settings object.
    """
    try:
        log_settings = config.logging
        print(log_settings)
    except AttributeError:
        print("Warning: 'logging' section not in config. Using basic logging.")
        logging.basicConfig(level=logging.INFO)
        return

    # Create logger
    # Get the root logger
    logger = logging.getLogger()
    logger.setLevel(log_settings.level.upper())  # Set the lowest level to process

    # Clear existing handlers
    if logger.hasHandlers():
        logger.handlers.clear()

    # Create formatter
    formatter = logging.Formatter(log_settings.format)

    # 1. Console Handler (StreamHandler)
    # This always logs to the console
    console_handler = logging.StreamHandler(sys.stdout)
    console_handler.setLevel(log_settings.level.upper())
    console_handler.setFormatter(formatter)
    logger.addHandler(console_handler)

    # 2. File Handler (RotatingFileHandler)
    # This logs to a file if configured
    if log_settings.log_to_file:
        log_file_path = Path(log_settings.log_file)

        # Ensure the log directory exists
        log_file_path.parent.mkdir(parents=True, exist_ok=True)

        # Use RotatingFileHandler for log rotation
        file_handler = RotatingFileHandler(
            log_file_path,
            maxBytes=log_settings.rotation_size_mb * 1024 * 1024,  # in bytes
            backupCount=log_settings.rotation_backup_count,
        )
        file_handler.setLevel(log_settings.level.upper())
        file_handler.setFormatter(formatter)
        logger.addHandler(file_handler)

    logger.debug("Logging configured.")
