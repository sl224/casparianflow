# src/casparian_flow/engine/worker.py
import time
import json
import logging
import traceback
from pathlib import Path
from sqlalchemy.orm import Session
from sqlalchemy import create_engine

from casparian_flow.engine.queue import JobQueue
from casparian_flow.plugins.loader import PluginRegistry
from casparian_flow.engine.context import WorkerContext, InspectionInterrupt
from casparian_flow.engine.config import WorkerConfig
from casparian_flow.db.models import FileVersion, FileLocation, PluginConfig, TopicConfig
from casparian_flow.db import access as sql_io
from casparian_flow.config import settings

logger = logging.getLogger(__name__)

class CasparianWorker:
    def __init__(self, config: WorkerConfig):
        # FIX: Use the connection string from config if provided, else fallback to settings
        if config.database and config.database.connection_string:
            self.engine = create_engine(config.database.connection_string)
        else:
            self.engine = sql_io.get_engine(settings.database)
            
        self.parquet_root = config.storage.parquet_root
        self.queue = JobQueue(self.engine)
        self.plugins = PluginRegistry(config.plugins.dir)
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
        # Resolve path and IDs for Lineage
        full_path, location_id = self._resolve_file_details(job.file_version_id)
        
        # 1. Fetch Persistent Config & Params
        topic_config = {}
        plugin_params = {}
        
        with Session(self.engine) as session:
            p_conf = session.get(PluginConfig, job.plugin_name)
            if p_conf:
                plugin_params = json.loads(p_conf.default_parameters)
                
                # Query TopicConfig table for this plugin
                topics = session.query(TopicConfig).filter_by(plugin_name=job.plugin_name).all()
                for topic in topics:
                    topic_config[topic.topic_name] = {
                        "uri": topic.uri,
                        "mode": topic.mode,
                        "schema": json.loads(topic.schema_json) if topic.schema_json else None
                    }

        # 2. Check for Job-Specific Overrides (e.g. "Inspect Mode")
        # For MVP, let's assume if plugin_params has "mode": "inspect", we inspect.
        is_inspection = plugin_params.get("mode") == "inspect"

        # 3. Init Context with Lineage Info
        ctx = WorkerContext(
            sql_engine=self.engine, 
            parquet_root=self.parquet_root, 
            topic_config=topic_config,
            inspect_mode=is_inspection,
            job_id=job.id,
            file_version_id=job.file_version_id,
            file_location_id=location_id
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
            else:
                # GOVERNANCE: Atomic Promotion (Blue/Green Deployment)
                # Only commit (swap staging -> prod) if execution succeeded
                ctx.commit()
                
        except InspectionInterrupt:
            logger.info(f"Inspection Halt for Job {job.id}")
            self.queue.complete_job(job.id, summary=json.dumps(ctx.captured_schemas))
        finally:
            ctx.close_all()

    def _resolve_file_details(self, file_version_id):
        """Resolve the full file path and location ID from a FileVersion ID."""
        with Session(self.engine) as session:
            # Navigate FileVersion -> FileLocation -> SourceRoot
            file_version = session.get(FileVersion, file_version_id)
            if not file_version:
                raise ValueError(f"FileVersion {file_version_id} not found!")
            
            file_location = session.get(FileLocation, file_version.location_id)
            if not file_location:
                raise ValueError(f"FileLocation {file_version.location_id} not found!")
            
            return Path(file_location.source_root.path) / file_location.rel_path, file_location.id