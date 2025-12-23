# src/casparian_flow/security/factory.py
"""
v5.0 Bridge Mode: Identity Provider Factory.

Configurable via AUTH_MODE environment variable or config:
- "local": LocalProvider (zero friction, auto-generated keys)
- "entra": AzureProvider (zero trust, Azure AD integration)

Usage:
    provider = get_identity_provider()
    user = provider.authenticate(token)
    signed = provider.sign_artifact(artifact_hash)
"""

import os
import logging
from typing import Optional, Literal
from pathlib import Path

from casparian_flow.security.identity import IdentityProvider

logger = logging.getLogger(__name__)

AuthMode = Literal["local", "entra"]


def get_auth_mode() -> AuthMode:
    """
    Get the configured authentication mode.

    Priority:
    1. AUTH_MODE environment variable
    2. Default to "local"
    """
    mode = os.environ.get("AUTH_MODE", "local").lower()
    if mode not in ("local", "entra"):
        logger.warning(f"Unknown AUTH_MODE '{mode}', defaulting to 'local'")
        return "local"
    return mode


def get_identity_provider(
    mode: Optional[AuthMode] = None,
    keys_dir: Optional[Path] = None,
    **kwargs,
) -> IdentityProvider:
    """
    Factory function to create the appropriate identity provider.

    Args:
        mode: Override the configured AUTH_MODE
        keys_dir: Override the default keys directory
        **kwargs: Additional arguments passed to the provider

    Returns:
        Configured IdentityProvider instance

    Raises:
        ImportError: If required packages are not installed for the mode
        ValueError: If configuration is invalid
    """
    if mode is None:
        mode = get_auth_mode()

    logger.info(f"Initializing identity provider: mode={mode}")

    if mode == "local":
        from casparian_flow.security.local_provider import LocalProvider
        return LocalProvider(keys_dir=keys_dir, **kwargs)

    elif mode == "entra":
        from casparian_flow.security.azure_provider import AzureProvider
        return AzureProvider(keys_dir=keys_dir, **kwargs)

    else:
        raise ValueError(f"Unknown auth mode: {mode}")


# Singleton instance (lazy-loaded)
_provider_instance: Optional[IdentityProvider] = None


def get_provider() -> IdentityProvider:
    """
    Get the singleton identity provider instance.

    This is the recommended way to access the provider in most cases.
    The instance is created on first access.
    """
    global _provider_instance
    if _provider_instance is None:
        _provider_instance = get_identity_provider()
    return _provider_instance


def reset_provider():
    """Reset the singleton provider (useful for testing)."""
    global _provider_instance
    _provider_instance = None
