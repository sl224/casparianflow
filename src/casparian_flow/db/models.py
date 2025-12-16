# src/casparian_flow/db/models.py
from enum import Enum as PyEnum
from sqlalchemy import (
    Column,
    Integer,
    String,
    ForeignKey,
    DateTime,
    Enum,
    Index,
    Text,
    func,
)
from sqlalchemy.orm import relationship
from casparian_flow.db.base_session import Base, DEFAULT_SCHEMA


class StatusEnum(PyEnum):
    PENDING = "PENDING"
    QUEUED = "QUEUED"
    RUNNING = "RUNNING"
    COMPLETED = "COMPLETED"
    FAILED = "FAILED"
    SKIPPED = "SKIPPED"


class PluginStatusEnum(PyEnum):
    """Plugin deployment status lifecycle."""

    PENDING = "PENDING"  # Newly submitted, awaiting validation
    STAGING = "STAGING"  # Passed validation, ready for sandbox testing
    ACTIVE = "ACTIVE"  # Deployed and actively serving jobs
    REJECTED = "REJECTED"  # Failed validation or sandbox test


class RoutingRule(Base):
    __tablename__ = "cf_routing_rule"
    id = Column(Integer, primary_key=True)
    pattern = Column(String(500), nullable=False)
    tag = Column(String(50), nullable=False)
    priority = Column(Integer, default=0)


class PluginConfig(Base):
    __tablename__ = "cf_plugin_config"
    plugin_name = Column(String(100), primary_key=True)
    subscription_tags = Column(String(1000), default="")
    default_parameters = Column(Text, nullable=True, default="{}")
    last_updated = Column(DateTime, server_default=func.now(), onupdate=func.now())
    topics = relationship(
        "TopicConfig", back_populates="plugin", cascade="all, delete-orphan"
    )


class TopicConfig(Base):
    __tablename__ = "cf_topic_config"
    id = Column(Integer, primary_key=True)
    plugin_name = Column(
        String(100), ForeignKey("cf_plugin_config.plugin_name"), nullable=False
    )
    topic_name = Column(String(100), nullable=False)
    uri = Column(String(1000), nullable=False)
    mode = Column(String(50), default="append")
    schema_json = Column(Text, nullable=True)
    plugin = relationship("PluginConfig", back_populates="topics")

    __table_args__ = (
        Index("ix_topic_lookup", "plugin_name", "topic_name"),
        {"schema": DEFAULT_SCHEMA},
    )


class SourceRoot(Base):
    __tablename__ = "cf_source_root"
    id = Column(Integer, primary_key=True)
    path = Column(String(1000), unique=True, nullable=False)
    type = Column(String(50), default="local")
    active = Column(Integer, default=1)


class FileHashRegistry(Base):
    __tablename__ = "cf_file_hash_registry"
    content_hash = Column(String(64), primary_key=True)
    first_seen = Column(DateTime, server_default=func.now())
    size_bytes = Column(Integer, nullable=False)


class FileLocation(Base):
    __tablename__ = "cf_file_location"
    id = Column(Integer, primary_key=True)
    source_root_id = Column(Integer, ForeignKey("cf_source_root.id"), nullable=False)

    # CRITICAL: 850 chars is safe for MSSQL Index (Limit 900 bytes).
    # Do NOT use Text here or lookups become O(N).
    rel_path = Column(String(850), nullable=False)
    filename = Column(String(255), nullable=False)

    current_version_id = Column(
        Integer, ForeignKey("cf_file_version.id"), nullable=True
    )
    discovered_time = Column(DateTime, server_default=func.now())
    last_seen_time = Column(DateTime, server_default=func.now())

    source_root = relationship("SourceRoot")

    __table_args__ = (
        # This index is mandatory for Scout performance
        Index("ix_file_location_lookup", "source_root_id", "rel_path"),
        {"schema": DEFAULT_SCHEMA},
    )


class FileVersion(Base):
    __tablename__ = "cf_file_version"
    id = Column(Integer, primary_key=True)
    location_id = Column(Integer, ForeignKey("cf_file_location.id"), nullable=False)
    content_hash = Column(
        String(64), ForeignKey("cf_file_hash_registry.content_hash"), nullable=False
    )
    size_bytes = Column(Integer, nullable=False)
    modified_time = Column(DateTime, nullable=False)
    detected_at = Column(DateTime, server_default=func.now())
    applied_tags = Column(String(1000), default="")

    location = relationship("FileLocation", foreign_keys=[location_id])
    hash_registry = relationship("FileHashRegistry")

    __table_args__ = (
        Index("ix_file_version_lookup", "location_id", "content_hash"),
        {"schema": DEFAULT_SCHEMA},
    )


class ProcessingJob(Base):
    __tablename__ = "cf_processing_queue"
    id = Column(Integer, primary_key=True)
    file_version_id = Column(Integer, ForeignKey("cf_file_version.id"), nullable=False)
    plugin_name = Column(
        String(100), ForeignKey(PluginConfig.plugin_name), nullable=False
    )
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
        {"schema": DEFAULT_SCHEMA},
    )


class WorkerNode(Base):
    __tablename__ = "cf_worker_node"
    hostname = Column(String(100), primary_key=True)
    pid = Column(Integer, primary_key=True)
    ip_address = Column(String(50), nullable=True)
    env_signature = Column(String(100), nullable=True)
    started_at = Column(DateTime, server_default=func.now())
    last_heartbeat = Column(DateTime, server_default=func.now())
    status = Column(String(50), default="ACTIVE")
    current_job_id = Column(Integer, nullable=True)


class PluginManifest(Base):
    """
    Plugin code registry for AI-generated plugins.

    Supports the Architect workflow: DEPLOY → Validate → Sandbox → ACTIVE
    """

    __tablename__ = "cf_plugin_manifest"
    id = Column(Integer, primary_key=True)
    plugin_name = Column(String(100), nullable=False, index=True)
    version = Column(String(50), nullable=False)
    source_code = Column(Text, nullable=False)
    source_hash = Column(String(64), nullable=False, unique=True)
    status = Column(Enum(PluginStatusEnum), default=PluginStatusEnum.PENDING, index=True)
    signature = Column(String(128), nullable=True)
    validation_error = Column(Text, nullable=True)
    created_at = Column(DateTime, server_default=func.now())
    deployed_at = Column(DateTime, nullable=True)

    __table_args__ = (
        Index("ix_plugin_active_lookup", "plugin_name", "status"),
        {"schema": DEFAULT_SCHEMA},
    )



