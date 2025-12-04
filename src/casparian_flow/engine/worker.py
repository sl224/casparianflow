# src/casparian_flow/engine/worker.py
import time
import json
import logging
import traceback
from pathlib import Path
from typing import Dict
from sqlalchemy.orm import Session

from casparian_flow.engine.queue import JobQueue
from casparian_flow.plugins.loader import PluginRegistry
from casparian_flow.engine.context import WorkerContext, InspectionInterrupt
from casparian_flow.db.models import FileMetadata, PluginConfig
from casparian_flow.db import access as sql_io
from casparian_flow.config import settings

logger = logging.getLogger(__name__)

class CasparianWorker:
    def __init__(self, config: Dict):
        self.engine = sql_io.get_engine(settings.database)
        self.parquet_root = Path(config.get("storage", {}).get("parquet_root", "data/parquet"))
        self.queue = JobQueue(self.engine)
        self.plugins = PluginRegistry(Path(config["plugins"]["dir"]))
        self.plugins.discover()
        self.active = True

    def run(self):
        logger.info("Worker Online. Waiting for jobs...")
        while self.active:
            job = self.queue.pop_job('test_signature')
            if not job:
                time.sleep(1)
                continue

            logger.info(f"Processing Job {job.id} | Plugin: {job.plugin_name}")
            try:
                self._execute_job(job)
                # Note: If _execute_job returns specialized status (like inference result),
                # we should handle it. For now, basic success:
                if job.result_summary is None:
                    self.queue.complete_job(job.id, summary="Success")
            except Exception as e:
                logger.error(f"Job {job.id} Failed: {e}", exc_info=True)
                self.queue.fail_job(job.id, error=str(e) + "\n" + traceback.format_exc())

    def _execute_job(self, job):
        full_path = self._resolve_file_path(job.file_id)
        
        # 1. Fetch Persistent Config & Params
        topic_config = {}
        plugin_params = {}
        
        with Session(self.engine) as session:
            p_conf = session.get(PluginConfig, job.plugin_name)
            if p_conf:
                topic_config = json.loads(p_conf.topic_config)
                plugin_params = json.loads(p_conf.default_parameters)

        # 2. Check for Job-Specific Overrides (e.g. "Inspect Mode")
        # The job table might contain overrides in a column, or we can use 'result_summary' 
        # or a temp column to flag 'inspection'. Assuming job.priority or similar implies it for now,
        # or we add a 'mode' column to ProcessingJob later.
        # For MVP, let's assume if plugin_params has "mode": "inspect", we inspect.
        is_inspection = plugin_params.get("mode") == "inspect"

        # 3. Init Context
        ctx = WorkerContext(
            self.engine, 
            self.parquet_root, 
            topic_config=topic_config,
            inspect_mode=is_inspection
        )
        
        # 4. Load & Configure Plugin
        plugin_cls = self.plugins.get_plugin(job.plugin_name)
        plugin = plugin_cls()
        
        # SYSTEM HOOK: Inject dependencies
        plugin.configure(ctx, plugin_params)

        try:
            plugin.execute(str(full_path))
            
            # If we finish execution in inspection mode without interrupt (small file)
            if is_inspection:
                self.queue.complete_job(job.id, summary=json.dumps(ctx.captured_schemas))
                
        except InspectionInterrupt:
            logger.info(f"Inspection Halt for Job {job.id}")
            self.queue.complete_job(job.id, summary=json.dumps(ctx.captured_schemas))
        finally:
            ctx.close_all()

    def _resolve_file_path(self, file_id):
        with Session(self.engine) as session:
            fmeta = session.get(FileMetadata, file_id)
            if not fmeta:
                raise ValueError(f"FileMetadata {file_id} not found!")
            return Path(fmeta.source_root.path) / fmeta.rel_path