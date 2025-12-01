import threading
import time
import socket
import json
import logging
from sqlalchemy import create_engine, text
from casparian_flow.context import EtlContext

logger = logging.getLogger(__name__)

class HeartbeatThread(threading.Thread):
    def __init__(self, db_url: str, interval: int = 30):
        super().__init__(daemon=True) # Daemon dies when main process dies
        self.db_url = db_url
        self.interval = interval
        self.active = True
        
        # Identity
        self.ctx = EtlContext.capture()
        self.hostname = socket.gethostname()
        self.ip = socket.gethostbyname(self.hostname)
        self.env_sig = self.ctx.git_hash # Or a specific env string passed in config

    def run(self):
        engine = create_engine(self.db_url)
        logger.info(f"Heartbeat started for {self.hostname} ({self.env_sig})")
        
        while self.active:
            try:
                self._pulse(engine)
            except Exception as e:
                logger.error(f"Heartbeat failed: {e}")
            
            time.sleep(self.interval)
        
        engine.dispose()

    def _pulse(self, engine):
        # UPSERT logic (MSSQL Syntax)
        # Updates last_heartbeat if exists, inserts if new
        sql = """
        MERGE cf_worker_registry AS target
        USING (SELECT :host AS hostname) AS source
        ON (target.hostname = source.hostname)
        WHEN MATCHED THEN
            UPDATE SET last_heartbeat = GETDATE(), status = 'ONLINE', 
                       env_signature = :env, ip_address = :ip
        WHEN NOT MATCHED THEN
            INSERT (hostname, ip_address, env_signature, status, last_heartbeat)
            VALUES (:host, :ip, :env, 'ONLINE', GETDATE());
        """
        with engine.begin() as conn:
            conn.execute(text(sql), {
                "host": self.hostname,
                "ip": self.ip,
                "env": self.env_sig
            })