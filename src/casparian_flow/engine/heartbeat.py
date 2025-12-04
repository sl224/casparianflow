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
        self.engine = None
        
        # Identity
        import os
        self.ctx = EtlContext.capture()
        self.hostname = socket.gethostname()
        self.ip_address = socket.gethostbyname(self.hostname)
        self.pid = os.getpid()
        self.env_signature = self.ctx.git_hash # Or a specific env string passed in config

    def run(self):
        self.engine = create_engine(self.db_url)
        logger.info(f"Heartbeat started for {self.hostname} ({self.env_signature})")
        
        while self.active:
            try:
                self._send_heartbeat()
            except Exception as e:
                logger.error(f"Heartbeat failed: {e}")
            
            time.sleep(self.interval)
        
        if self.engine:
            self.engine.dispose()

    def _send_heartbeat(self):
        """Send heartbeat using database-agnostic SQLAlchemy ORM."""
        try:
            from sqlalchemy.orm import Session
            from casparian_flow.db.models import WorkerNode
            from datetime import datetime
            
            with Session(self.engine) as session:
                # Try to get existing worker record
                worker = session.query(WorkerNode).filter_by(
                    hostname=self.hostname
                ).first()
                
                if worker:
                    # Update existing record
                    worker.last_heartbeat = datetime.now()
                    worker.status = "ONLINE"
                    worker.env_signature = self.env_signature
                    worker.ip_address = self.ip_address
                    worker.pid = self.pid # Update pid as well
                else:
                    # Create new record
                    worker = WorkerNode(
                        hostname=self.hostname,
                        pid=self.pid,
                        ip_address=self.ip_address,
                        env_signature=self.env_signature,
                        status="ONLINE"
                    )
                    session.add(worker)
                
                session.commit()
                logger.debug(f"Heartbeat sent for {self.hostname}")
                
        except Exception as e:
            logger.error(f"Heartbeat failed: {e}")