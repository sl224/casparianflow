# src/casparian_flow/db/models.py
from enum import Enum as PyEnum
from sqlalchemy import (
    Column, Integer, String, ForeignKey, DateTime, Enum, 
    Index, VARBINARY, Text, func
)
from sqlalchemy.orm import relationship, declarative_base
from sqlalchemy.dialects.mssql import DATETIME2

from casparian_flow.db.base_session import Base

class StatusEnum(PyEnum):
    PENDING = "PENDING"
    QUEUED = "QUEUED"
    RUNNING = "RUNNING"
    COMPLETED = "COMPLETED"
    FAILED = "FAILED"
    SKIPPED = "SKIPPED"

class FileHashRegistry(Base):
    """Canonical store of unique file content (HCD)."""
    __tablename__ = "cf_hash_registry"
    id = Column(Integer, primary_key=True)
    md5 = Column(VARBINARY(16), unique=True, nullable=False, index=True)
    first_seen_at = Column(DateTime, server_default=func.now())

class SourceRoot(Base):
    """A root location scan target (replaces FolderMetadata)."""
    __tablename__ = "cf_source_root"
    id = Column(Integer, primary_key=True)
    path = Column(String(500), unique=True, nullable=False)
    discovery_date = Column(DateTime, server_default=func.now())
    tags = Column(String(500), nullable=True) 
    files = relationship("FileMetadata", back_populates="root")

class FileMetadata(Base):
    """Instance of a file found during Discovery."""
    __tablename__ = "cf_file_metadata"
    id = Column(Integer, primary_key=True)
    root_id = Column(Integer, ForeignKey("cf_source_root.id"), nullable=False, index=True)
    hash_id = Column(Integer, ForeignKey("cf_hash_registry.id"), nullable=False, index=True)
    relative_path = Column(String(500), nullable=False)
    file_size_bytes = Column(Integer)
    file_type = Column(String(50), index=True)
    
    root = relationship("SourceRoot", back_populates="files")
    hash_info = relationship("FileHashRegistry")

class ProcessingJob(Base):
    """
    The Distributed Queue Unit.
    """
    __tablename__ = "cf_processing_queue"

    id = Column(Integer, primary_key=True)
    
    # Input
    file_id = Column(Integer, ForeignKey("cf_file_metadata.id"), nullable=False)
    plugin_name = Column(String(100), nullable=False)
    plugin_config = Column(Text, nullable=True) 

    # Output (Sink Pattern)
    sink_type = Column(String(50), nullable=False) 
    sink_config = Column(Text, nullable=True) 

    # State
    status = Column(Enum(StatusEnum), default=StatusEnum.PENDING, index=True)
    priority = Column(Integer, default=0, index=True)
    
    # Worker Claim
    worker_host = Column(String(100), nullable=True)
    worker_pid = Column(Integer, nullable=True)
    claim_time = Column(DATETIME2(3), nullable=True)
    
    # Result
    end_time = Column(DATETIME2(3), nullable=True)
    result_summary = Column(Text, nullable=True) 
    error_message = Column(Text, nullable=True)
    retry_count = Column(Integer, default=0)

    file = relationship("FileMetadata")

    __table_args__ = (
        Index("ix_queue_pop", "status", "priority", "id"),
    )