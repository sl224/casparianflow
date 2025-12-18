# src/casparian_flow/main.py
import logging
import sys
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig
from casparian_flow.engine.sentinel import Sentinel

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

if __name__ == "__main__":
    logger.info("Starting Casparian Sentinel (Broker)...")
    
    config = WorkerConfig(
        database=DatabaseConfig(
            connection_string=str(sql_io.get_engine(settings.database).url)
        )
    )
    
    sentinel = Sentinel(config, bind_addr="tcp://127.0.0.1:5555")
    
    try:
        sentinel.run()
    except KeyboardInterrupt:
        logger.info("Sentinel shutting down.")