# src/casparian_flow/security/__init__.py
"""
Security and validation modules for Casparian Flow.

v5.0 Bridge Mode additions:
- Identity providers (Local, Azure) for authentication and signing
- Factory function for provider selection based on AUTH_MODE
"""

from casparian_flow.security.gatekeeper import (
    validate_plugin_safety,
    verify_signature,
    compute_source_hash,
)

from casparian_flow.security.identity import (
    IdentityProvider,
    User,
    SignedArtifact,
    AuthenticationError,
    compute_artifact_hash,
    compute_env_hash,
)

from casparian_flow.security.factory import (
    get_identity_provider,
    get_provider,
    get_auth_mode,
    reset_provider,
)

__all__ = [
    # Gatekeeper (v4.0)
    "validate_plugin_safety",
    "verify_signature",
    "compute_source_hash",
    # Identity (v5.0)
    "IdentityProvider",
    "User",
    "SignedArtifact",
    "AuthenticationError",
    "compute_artifact_hash",
    "compute_env_hash",
    # Factory (v5.0)
    "get_identity_provider",
    "get_provider",
    "get_auth_mode",
    "reset_provider",
]
