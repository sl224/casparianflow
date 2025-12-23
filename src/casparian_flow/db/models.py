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
    Float,
    Boolean,
    func,
    UniqueConstraint,
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

    PENDING = "PENDING"
    STAGING = "STAGING"
    ACTIVE = "ACTIVE"
    REJECTED = "REJECTED"


class RoutingRule(Base):
    __tablename__ = "cf_routing_rule"
    id = Column(Integer, primary_key=True)
    pattern = Column(String(500), nullable=False)
    tag = Column(String(50), nullable=False)
    priority = Column(Integer, default=0)


class IgnoreRule(Base):
    """
    Patterns to exclude from scanning (e.g., node_modules, *.tmp).
    Uses .gitignore syntax.
    """

    __tablename__ = "cf_ignore_rule"
    id = Column(Integer, primary_key=True)
    source_root_id = Column(
        Integer, ForeignKey("cf_source_root.id"), nullable=True
    )  # Null = Global rule
    pattern = Column(String(500), nullable=False)
    active = Column(Boolean, default=True)
    created_at = Column(DateTime, server_default=func.now())


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
    rel_path = Column(String(850), nullable=False)
    filename = Column(String(255), nullable=False)

    # Inventory State
    last_known_mtime = Column(Float, nullable=True)
    last_known_size = Column(Integer, nullable=True)

    current_version_id = Column(
        Integer, ForeignKey("cf_file_version.id"), nullable=True
    )
    discovered_time = Column(DateTime, server_default=func.now())
    last_seen_time = Column(DateTime, server_default=func.now())

    source_root = relationship("SourceRoot")
    tags = relationship("FileTag", cascade="all, delete-orphan")

    __table_args__ = (
        Index("ix_file_location_lookup", "source_root_id", "rel_path"),
        {"schema": DEFAULT_SCHEMA},
    )

class FileTag(Base):
    __tablename__ = "cf_file_tag"
    file_id = Column(Integer, ForeignKey("cf_file_location.id"), primary_key=True)
    tag = Column(String(50), primary_key=True, index=True)
    __table_args__ = ({"schema": DEFAULT_SCHEMA},)


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
    config_overrides = Column(Text, nullable=True)  # JSON string
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


class PluginSubscription(Base):
    __tablename__ = "cf_plugin_subscription"
    id = Column(Integer, primary_key=True)
    plugin_name = Column(
        String(100), ForeignKey("cf_plugin_config.plugin_name"), nullable=False
    )
    topic_name = Column(String(100), nullable=False, index=True)
    is_active = Column(Boolean, default=True)

    __table_args__ = (
        UniqueConstraint("plugin_name", "topic_name", name="uq_plugin_topic_sub"),
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
    __tablename__ = "cf_plugin_manifest"
    id = Column(Integer, primary_key=True)
    plugin_name = Column(String(100), nullable=False, index=True)
    version = Column(String(50), nullable=False)
    source_code = Column(Text, nullable=False)
    source_hash = Column(String(64), nullable=False, unique=True)
    status = Column(
        Enum(PluginStatusEnum), default=PluginStatusEnum.PENDING, index=True
    )
    signature = Column(String(128), nullable=True)
    validation_error = Column(Text, nullable=True)
    created_at = Column(DateTime, server_default=func.now())
    deployed_at = Column(DateTime, nullable=True)

    __table_args__ = (
        Index("ix_plugin_active_lookup", "plugin_name", "status"),
        {"schema": DEFAULT_SCHEMA},
    )


class PhaseEnum(PyEnum):
    IDLE = "IDLE"
    PHASE_1_RECONNAISSANCE = "PHASE_1_RECONNAISSANCE"
    PHASE_2_ENVIRONMENT = "PHASE_2_ENVIRONMENT"
    PHASE_3_CONSTRUCTION = "PHASE_3_CONSTRUCTION"
    PHASE_4_WIRING = "PHASE_4_WIRING"
    PHASE_5_VERIFICATION = "PHASE_5_VERIFICATION"
    PHASE_6_TEST_GENERATION = "PHASE_6_TEST_GENERATION"
    COMPLETED = "COMPLETED"
    FAILED = "FAILED"


class LibraryWhitelist(Base):
    __tablename__ = "cf_library_whitelist"
    id = Column(Integer, primary_key=True)
    library_name = Column(String(100), nullable=False, unique=True)
    version_constraint = Column(String(50), nullable=True)
    description = Column(Text, nullable=True)
    added_at = Column(DateTime, server_default=func.now())
    __table_args__ = ({"schema": DEFAULT_SCHEMA},)


class SurveyorSession(Base):
    __tablename__ = "cf_surveyor_session"
    id = Column(Integer, primary_key=True)
    source_root_id = Column(Integer, ForeignKey("cf_source_root.id"), nullable=False)
    current_phase = Column(Enum(PhaseEnum), default=PhaseEnum.IDLE)
    started_at = Column(DateTime, server_default=func.now())
    completed_at = Column(DateTime, nullable=True)
    error_message = Column(Text, nullable=True)
    phase_data = Column(Text, default="{}")
    source_root = relationship("SourceRoot")
    decisions = relationship(
        "SurveyorDecision", back_populates="session", cascade="all, delete-orphan"
    )
    __table_args__ = (
        Index("ix_surveyor_session_lookup", "source_root_id", "current_phase"),
        {"schema": DEFAULT_SCHEMA},
    )


class SurveyorDecision(Base):
    __tablename__ = "cf_surveyor_decision"
    id = Column(Integer, primary_key=True)
    session_id = Column(Integer, ForeignKey("cf_surveyor_session.id"), nullable=False)
    phase = Column(Enum(PhaseEnum), nullable=False)
    timestamp = Column(DateTime, server_default=func.now())
    decision_type = Column(String(50), nullable=False)
    decision_data = Column(Text, nullable=False)
    reasoning = Column(Text, nullable=True)
    session = relationship("SurveyorSession", back_populates="decisions")
    __table_args__ = (
        Index("ix_surveyor_decision_lookup", "session_id", "phase"),
        {"schema": DEFAULT_SCHEMA},
    )
