import logging
import sys
from pathlib import Path

# FIX: New Imports
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.db.setup import initialize_database
from casparian_flow.engine.worker import CasparianWorker

logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

if __name__ == "__main__":
    logger.info("Starting Casparian Flow Worker Node...")
    
    # 1. Initialize DB Connection
    try:
        # We need the connection string from settings
        # (Assuming settings.database has a connection_string property or we build it)
        # For now, we reuse the existing get_engine logic implicitly inside the Worker?
        # Actually, the Worker expects a 'config' dict. Let's provide it.
        
        # Build a config dict that the Worker expects
        worker_config = {
            "database": {
                "connection_string": str(sql_io.get_engine(settings.database).url) 
            },
            "storage": {
                "parquet_root": "data/parquet"
            },
            "plugins": {
                "dir": "src/casparian_flow/plugins" # Make sure this exists!
            }
        }
        
        # 2. Init DB Tables (Safe to run on startup)
        eng = sql_io.get_engine(settings.database)
        initialize_database(eng, reset_tables=True)
        eng.dispose() # Worker creates its own engine

        # 3. Run Worker
        worker = CasparianWorker(worker_config)
        worker.run()

    except KeyboardInterrupt:
        logger.info("Worker stopped by user.")
    except Exception as e:
        logger.critical(f"Worker failed: {e}", exc_info=True)
        sys.exit(1)