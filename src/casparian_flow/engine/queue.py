import logging
import socket
import os
import json
from datetime import datetime
from typing import Optional, Dict, Any
from sqlalchemy import text, Engine
from sqlalchemy.orm import Session

# Note: This imports the NEW model structure.
# You must complete the Phase 1 Model Refactor for this to import correctly.
from casparian_flow.db.models import ProcessingJob, StatusEnum

logger = logging.getLogger(__name__)

class JobQueue:
    """
    MSSQL-backed Distributed Job Queue using atomic Skip-Locked pattern.
    """
    def __init__(self, engine: Engine):
        self.engine = engine
        self.hostname = socket.gethostname()
        self.pid = os.getpid()

    def pop_job(self, my_env: str) -> Optional[ProcessingJob]:
        """
        Pop a job, respecting Environment requirements.
        """
        if self.engine.dialect.name == "sqlite":
            return self._pop_job_sqlite()
        else:
            return self._pop_job_mssql()

    def _pop_job_sqlite(self) -> Optional[ProcessingJob]:
        with Session(self.engine) as session:
            # Simple lock-free pop for SQLite (single worker assumed or race conditions accepted for dev)
            job = session.query(ProcessingJob).filter(
                ProcessingJob.status == StatusEnum.QUEUED
            ).order_by(ProcessingJob.priority.desc(), ProcessingJob.id.asc()).first()
            
            if job:
                job.status = StatusEnum.RUNNING
                job.worker_host = self.hostname
                job.worker_pid = self.pid
                job.claim_time = datetime.now()
                session.commit()
                session.refresh(job)
                session.expunge(job)
                return job
        return None

    def _pop_job_mssql(self) -> Optional[ProcessingJob]:
        sql = """
        WITH cte AS (
            SELECT TOP(1) *
            FROM casp.cf_processing_queue WITH (ROWLOCK, READPAST, UPDLOCK)
            WHERE status = :pending_status
            ORDER BY priority DESC, id ASC
        )
        UPDATE cte
        SET 
            status = :running_status,
            worker_host = :host,
            worker_pid = :pid,
            claim_time = SYSDATETIME()
        OUTPUT inserted.id;
        """ 
        try:
            with self.engine.begin() as conn:
                result = conn.execute(
                    text(sql),
                    {
                        "pending_status": StatusEnum.QUEUED.name,
                        "running_status": StatusEnum.RUNNING.name,
                        "host": self.hostname,
                        "pid": self.pid,
                    }
                ).scalar()
                
                if result:
                    return self._fetch_job_orm(result)
                    
        except Exception as e:
            logger.error(f"Queue Pop Failed: {e}")
            return None
            
        return None

    def _fetch_job_orm(self, job_id: int) -> ProcessingJob:
        """Helper to get the full job object after locking."""
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            session.expunge(job) # Detach so it can be used outside this short session
            return job

    def complete_job(self, job_id: int, summary: str = None):
        """Marks a job as COMPLETED."""
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            if job:
                job.status = StatusEnum.COMPLETED
                job.end_time = datetime.now()
                job.result_summary = summary
                session.commit()
                logger.info(f"Job {job_id} marked COMPLETED.")

    def fail_job(self, job_id: int, error: str):
        """Marks a job as FAILED with error message."""
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            if job:
                job.status = StatusEnum.FAILED
                job.end_time = datetime.now()
                job.error_message = str(error)
                session.commit()
                logger.error(f"Job {job_id} marked FAILED.")

    def push_job(self, 
                 file_id: int, 
                 plugin_name: str, 
                 sink_type: str,
                 sink_config: Dict[str, Any],
                 priority: int = 0, 
                ):
        """
        Registers a new job into the queue (Used by The Scout).
        """
        with Session(self.engine) as session:
            new_job = ProcessingJob(
                file_id=file_id,
                plugin_name=plugin_name,
                sink_type=sink_type,
                status=StatusEnum.QUEUED,
                priority=priority
            )
            session.add(new_job)
            session.commit()