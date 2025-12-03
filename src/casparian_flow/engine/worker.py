import time
import json
import logging
import traceback
from pathlib import Path
from sqlalchemy import create_engine
from sqlalchemy.orm import Session

from casparian_flow.engine.queue import JobQueue
from casparian_flow.plugins.loader import PluginRegistry
from casparian_flow.engine.context import WorkerContext
from casparian_flow.db.models import FileMetadata
from typing import Dict
from casparian_flow.db import access as sql_io
logger = logging.getLogger(__name__)

from casparian_flow.config import settings

class CasparianWorker:
    def __init__(self, config: Dict):
        # 1. Setup Infrastructure
        # self.db_url = config["database"]["connection_string"]
        self.engine = sql_io.get_engine(settings.database)
        
        # 2. Setup Storage Roots
        self.parquet_root = Path(config.get("storage", {}).get("parquet_root", "data/parquet"))
        
        # 3. Setup Components
        self.queue = JobQueue(self.engine)
        self.plugins = PluginRegistry(Path(config["plugins"]["dir"]))
        self.plugins.discover()
        
        self.active = True

    def run(self):
        logger.info("Worker Online. Waiting for jobs...")
        
        while self.active:
            # Atomic Pop
            job = self.queue.pop_job('test_signature')
            
            if not job:
                time.sleep(1)
                continue

            logger.info(f"Processing Job {job.id} | Plugin: {job.plugin_name}")
            
            try:
                self._execute_job(job)
                self.queue.complete_job(job.id, summary="Success")
            except Exception as e:
                logger.error(f"Job {job.id} Failed: {e}", exc_info=True)
                self.queue.fail_job(job.id, error=str(e) + "\n" + traceback.format_exc())

    def _execute_job(self, job):
        # 1. Re-hydrate File Path
        # We need a fresh session because 'job' might be detached
        full_path = None
        with Session(self.engine) as session:
            fmeta = session.get(FileMetadata, job.file_id)
            if not fmeta:
                raise ValueError(f"FileMetadata {job.file_id} not found!")
            
            # Assuming the worker has access to the same path as the scout
            # In a real swarm, you might need path re-mapping here.
            full_path = Path(fmeta.root.path) / fmeta.relative_path

        if not full_path.exists():
            raise FileNotFoundError(f"File inaccessible: {full_path}")

        # 2. Initialize Context (The Handle Manager)
        ctx = WorkerContext(self.engine, self.parquet_root)
        
        # 3. Load Plugin
        plugin_cls = self.plugins.get_plugin(job.plugin_name)
        
        # 4. Plugin Init (Cold Path - Allocations allowed here)
        # The plugin registers its sinks and gets integer handles back.
        job_config = json.loads(job.plugin_config) if job.plugin_config else {}
        plugin = plugin_cls(ctx, job_config)
        try:
            # 5. Plugin Execute (Hot Path - Speed is key)
            # The plugin calls ctx.push(handle, df) internally.
            plugin.execute(str(full_path))
        finally:
            # 6. Cleanup
            ctx.close_all()