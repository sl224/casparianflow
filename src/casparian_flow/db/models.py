# src/casparian_flow/db/models.py
from enum import Enum as PyEnum
from sqlalchemy import (
    Column, Integer, String, ForeignKey, DateTime, Enum, 
    Index, VARBINARY, Text, func
)
from sqlalchemy.orm import relationship
from sqlalchemy.dialects.mssql import DATETIME2
from casparian_flow.db.base_session import Base, DEFAULT_SCHEMA

class StatusEnum(PyEnum):
    PENDING = "PENDING"
    QUEUED = "QUEUED"
    RUNNING = "RUNNING"
    COMPLETED = "COMPLETED"
    FAILED = "FAILED"
    SKIPPED = "SKIPPED"

class PluginConfig(Base):
    """
    Persistent configuration for a Plugin.
    Defines the 'Wiring' of topics to physical URIs.
    """
    __tablename__ = "cf_plugin_config"
    
    plugin_name = Column(String(100), primary_key=True)
    
    # Stores the wiring map: 
    # {"sales": {"uri": "mssql://...", "mode": "strict", "schema": {...}}}
    topic_config = Column(Text, nullable=False, default="{}") 
    
    # Operational toggles (e.g., {"skip_rows": 2})
    default_parameters = Column(Text, nullable=True, default="{}")
    
    last_updated = Column(DateTime, server_default=func.now(), onupdate=func.now())

class ProcessingJob(Base):
    __tablename__ = "cf_processing_queue"

    id = Column(Integer, primary_key=True)
    file_id = Column(Integer, ForeignKey("cf_file_metadata.id"), nullable=False)
    
    # Link to the Config table
    plugin_name = Column(String(100), ForeignKey(PluginConfig.plugin_name), nullable=False)

    status = Column(Enum(StatusEnum), default=StatusEnum.PENDING, index=True)
    priority = Column(Integer, default=0, index=True)
    
    worker_host = Column(String(100), nullable=True)
    worker_pid = Column(Integer, nullable=True)
    claim_time = Column(DATETIME2(3), nullable=True)
    end_time = Column(DATETIME2(3), nullable=True)
    
    result_summary = Column(Text, nullable=True) 
    error_message = Column(Text, nullable=True)
    retry_count = Column(Integer, default=0)

    file = relationship("FileMetadata")

    __table_args__ = (
        Index("ix_queue_pop", "status", "priority", "id"),
        {'schema': DEFAULT_SCHEMA}
    )

# ... (Keep FileMetadata, FileHashRegistry, SourceRoot, WorkerNode as they were) ...
# Note: Ensure you import FileMetadata etc. if you split this file, 
# or keep them in the same file as you had before.