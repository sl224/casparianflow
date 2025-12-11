# src/casparian_flow/main.py
import logging
import sys
import multiprocessing
from casparian_flow.config import settings
from casparian_flow.db import access as sql_io
from casparian_flow.engine.config import WorkerConfig, DatabaseConfig

# Toggle this to switch architectures
USE_ZMQ_ARCHITECTURE = True

logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

if __name__ == "__main__":
    # Windows support for spawning subprocesses (if sidecars are managed here later)
    multiprocessing.freeze_support()

    logger.info("Starting Casparian Flow Node...")

    try:
        # Build configuration
        worker_config = WorkerConfig(
            database=DatabaseConfig(
                connection_string=str(sql_io.get_engine(settings.database).url)
            )
        )

        if USE_ZMQ_ARCHITECTURE:
            from casparian_flow.engine.zmq_worker import ZmqWorker

            # For local dev, use TCP. For prod linux, use 'ipc:///tmp/casparian.sock'
            ZMQ_ADDR = "tcp://127.0.0.1:5555"

            logger.info(f"Initializing ZMQ Worker (Router) on {ZMQ_ADDR}...")
            worker = ZmqWorker(worker_config, zmq_addr=ZMQ_ADDR)

            logger.warning("REMINDER: Ensure you start sidecars manually!")
            logger.warning(
                f"Run: uv run -m casparian_flow.sidecar --plugin src/casparian_flow/plugins/test_plugin.py --connect {ZMQ_ADDR}"
            )
        else:
            from casparian_flow.engine.worker import CasparianWorker

            logger.info("Initializing Legacy In-Process Worker...")
            worker = CasparianWorker(worker_config)

        worker.run()

    except KeyboardInterrupt:
        logger.info("Worker stopped by user.")
        if "worker" in locals() and hasattr(worker, "stop"):
            worker.stop()
    except Exception as e:
        logger.critical(f"Worker failed: {e}", exc_info=True)
        sys.exit(1)
