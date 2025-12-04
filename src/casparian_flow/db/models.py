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
    """
    __tablename__ = "cf_plugin_config"
    
    plugin_name = Column(String(100), primary_key=True)
    
    # Operational toggles (e.g., {"skip_rows": 2})
    default_parameters = Column(Text, nullable=True, default="{}")
    
    last_updated = Column(DateTime, server_default=func.now(), onupdate=func.now())
    
    # Relationship to topics
    topics = relationship("TopicConfig", back_populates="plugin", cascade="all, delete-orphan")

class TopicConfig(Base):
    """
    Configuration for a plugin's output topic/sink.
    Defines where and how data should be written.
    """
    __tablename__ = "cf_topic_config"
    
    id = Column(Integer, primary_key=True)
    plugin_name = Column(String(100), ForeignKey("cf_plugin_config.plugin_name"), nullable=False)
    topic_name = Column(String(100), nullable=False)
    
    # Sink configuration
    uri = Column(String(500), nullable=False)  # "parquet://./output", "mssql://table"
    mode = Column(String(50), default="append")  # "append", "strict", "infer"
    
    # Optional schema validation (JSON for complex schemas)
    schema_json = Column(Text, nullable=True)
    
    # Relationship back to plugin
    plugin = relationship("PluginConfig", back_populates="topics")
    
    __table_args__ = (
        Index("ix_topic_lookup", "plugin_name", "topic_name"),
        {'schema': DEFAULT_SCHEMA}
    )

class SourceRoot(Base):
    """
    A root directory that the Scout monitors.
    """
    __tablename__ = "cf_source_root"
    
    id = Column(Integer, primary_key=True)
    path = Column(String(500), unique=True, nullable=False)
    # e.g., "local", "smb", "s3"
    type = Column(String(50), default="local") 
    active = Column(Integer, default=1)

class FileHashRegistry(Base):
    """
    Global registry of unique file content.
    Used for deduplication.
    """
    __tablename__ = "cf_file_hash_registry"
    
    # SHA256 or MD5 hash
    content_hash = Column(String(64), primary_key=True) 
    first_seen = Column(DateTime, server_default=func.now())
    size_bytes = Column(Integer, nullable=False)

class FileLocation(Base):
    """
    Represents the persistent path/existence of a file.
    The container that holds different versions of content over time.
    """
    __tablename__ = "cf_file_location"
    
    id = Column(Integer, primary_key=True)
    source_root_id = Column(Integer, ForeignKey("cf_source_root.id"), nullable=False)
    
    # Relative path from the source root
    rel_path = Column(String(1000), nullable=False)
    filename = Column(String(255), nullable=False)
    
    # Pointer to the current/latest version
    current_version_id = Column(Integer, ForeignKey("cf_file_version.id"), nullable=True)
    
    # Tracking
    discovered_time = Column(DateTime, server_default=func.now())
    last_seen_time = Column(DateTime, server_default=func.now())
    
    source_root = relationship("SourceRoot")
    
    __table_args__ = (
        Index("ix_file_location_lookup", "source_root_id", "rel_path"),
        {'schema': DEFAULT_SCHEMA}
    )

class FileVersion(Base):
    """
    Immutable record of a file's state at a point in time.
    Each file modification creates a new version.
    """
    __tablename__ = "cf_file_version"
    
    id = Column(Integer, primary_key=True)
    location_id = Column(Integer, ForeignKey("cf_file_location.id"), nullable=False)
    
    # Content Identity
    content_hash = Column(String(64), ForeignKey("cf_file_hash_registry.content_hash"), nullable=False)
    
    # Physical attributes at time of detection
    size_bytes = Column(Integer, nullable=False)
    modified_time = Column(DateTime, nullable=False)
    
    # When this version was detected by Scout
    detected_at = Column(DateTime, server_default=func.now())
    
    location = relationship("FileLocation", foreign_keys=[location_id])
    hash_registry = relationship("FileHashRegistry")
    
    __table_args__ = (
        Index("ix_file_version_lookup", "location_id", "content_hash"),
        {'schema': DEFAULT_SCHEMA}
    )

class ProcessingJob(Base):
    __tablename__ = "cf_processing_queue"

    id = Column(Integer, primary_key=True)
    
    # Link to the SPECIFIC VERSION processed, not the mutable location
    # This freezes history: Job 101 processed Version 5 forever
    file_version_id = Column(Integer, ForeignKey("cf_file_version.id"), nullable=False)
    
    # Link to the Config table
    plugin_name = Column(String(100), ForeignKey(PluginConfig.plugin_name), nullable=False)

    status = Column(Enum(StatusEnum), default=StatusEnum.PENDING, index=True)
    priority = Column(Integer, default=0, index=True)
    
    worker_host = Column(String(100), nullable=True)
    worker_pid = Column(Integer, nullable=True)
    claim_time = Column(DateTime, nullable=True)
    end_time = Column(DateTime, nullable=True)
    
    result_summary = Column(Text, nullable=True) 
    error_message = Column(Text, nullable=True)
    retry_count = Column(Integer, default=0)

    file_version = relationship("FileVersion")

    __table_args__ = (
        Index("ix_queue_pop", "status", "priority", "id"),
        {'schema': DEFAULT_SCHEMA}
    )

class WorkerNode(Base):
    """
    Registry of active workers in the Swarm.
    """
    __tablename__ = "cf_worker_node"
    
    hostname = Column(String(100), primary_key=True)
    pid = Column(Integer, primary_key=True)
    
    ip_address = Column(String(50), nullable=True)
    env_signature = Column(String(100), nullable=True)
    
    started_at = Column(DateTime, server_default=func.now())
    last_heartbeat = Column(DateTime, server_default=func.now())
    
    status = Column(String(50), default="ACTIVE")
    current_job_id = Column(Integer, nullable=True)