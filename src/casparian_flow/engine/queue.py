# src/casparian_flow/engine/queue.py
import logging
import socket
import os
from datetime import datetime
from typing import Optional, Dict, Any
from sqlalchemy import text, Engine
from sqlalchemy.orm import Session
from casparian_flow.db.models import ProcessingJob, StatusEnum

logger = logging.getLogger(__name__)


class JobQueue:
    def __init__(self, engine: Engine):
        self.engine = engine
        self.hostname = socket.gethostname()
        self.pid = os.getpid()

    def pop_job(self) -> Optional[ProcessingJob]:
        if self.engine.dialect.name == "sqlite":
            return self._pop_job_sqlite()
        else:
            return self._pop_job_mssql()

    def _pop_job_sqlite(self) -> Optional[ProcessingJob]:
        # Lock-free pop for SQLite (Dev/Test only)
        with Session(self.engine) as session:
            job = (
                session.query(ProcessingJob)
                .filter(ProcessingJob.status == StatusEnum.QUEUED)
                .order_by(ProcessingJob.priority.desc(), ProcessingJob.id.asc())
                .first()
            )

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
        # Optimized: Single Round-Trip Atomic Pop
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
        OUTPUT inserted.*; 
        """
        try:
            with self.engine.begin() as conn:
                # Returns the full row as a Row object
                row = conn.execute(
                    text(sql),
                    {
                        "pending_status": StatusEnum.QUEUED.name,
                        "running_status": StatusEnum.RUNNING.name,
                        "host": self.hostname,
                        "pid": self.pid,
                    },
                ).fetchone()

                if row:
                    # Manually construct object to avoid ORM overhead/round-trip
                    # Note: We map the row dictionary to the Model attributes
                    job_data = row._mapping
                    return ProcessingJob(
                        id=job_data["id"],
                        file_version_id=job_data["file_version_id"],
                        plugin_name=job_data["plugin_name"],
                        status=StatusEnum(job_data["status"]),
                        priority=job_data["priority"],
                        worker_host=job_data["worker_host"],
                        worker_pid=job_data["worker_pid"],
                        claim_time=job_data["claim_time"],
                    )

        except Exception as e:
            logger.error(f"Queue Pop Failed: {e}")
            return None

        return None

    def complete_job(self, job_id: int, summary: str = None):
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            if job:
                job.status = StatusEnum.COMPLETED
                job.end_time = datetime.now()
                job.result_summary = summary
                session.commit()
                logger.info(f"Job {job_id} marked COMPLETED.")

    def fail_job(self, job_id: int, error: str):
        with Session(self.engine) as session:
            job = session.get(ProcessingJob, job_id)
            if job:
                job.status = StatusEnum.FAILED
                job.end_time = datetime.now()
                job.error_message = str(error)
                session.commit()
                logger.error(f"Job {job_id} marked FAILED.")

    def push_job(self, file_id: int, plugin_name: str, priority: int = 0):
        with Session(self.engine) as session:
            new_job = ProcessingJob(
                file_version_id=file_id,
                plugin_name=plugin_name,
                status=StatusEnum.QUEUED,
                priority=priority,
            )
            session.add(new_job)
            session.commit()
