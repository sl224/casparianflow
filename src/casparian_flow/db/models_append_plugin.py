
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
