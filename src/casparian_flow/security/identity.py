# src/casparian_flow/security/identity.py
"""
v5.0 Bridge Mode: Identity Provider Abstraction.

Dual-Mode Security Strategy:
- Local Mode (AUTH_MODE="local"): Zero friction, auto-generated keys
- Enterprise Mode (AUTH_MODE="entra"): Zero trust, Azure AD integration

Design Principles:
- Interface segregation for testability
- Ed25519 for artifact signing (fast, secure, small signatures)
- Content-addressable hashing for artifact identity
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Optional, Tuple
from pathlib import Path
import hashlib
import secrets
import logging

logger = logging.getLogger(__name__)


@dataclass
class User:
    """Authenticated user identity."""
    id: int
    name: str
    email: Optional[str] = None
    azure_oid: Optional[str] = None  # Microsoft Object ID (Enterprise Mode)


@dataclass
class SignedArtifact:
    """Result of artifact signing."""
    artifact_hash: str  # SHA256(source_code + lockfile_content)
    signature: str  # Ed25519 signature (hex-encoded)
    public_key: str  # For verification (hex-encoded)


class IdentityProvider(ABC):
    """
    Abstract interface for identity and signing operations.

    Implementations:
    - LocalProvider: Auto-generated keys, implicit trust
    - AzureProvider: MSAL authentication, JWKS validation
    """

    @abstractmethod
    def authenticate(self, token: Optional[str] = None) -> User:
        """
        Authenticate a user.

        Args:
            token: Authentication token (API key for Local, JWT for Entra)

        Returns:
            Authenticated User object

        Raises:
            AuthenticationError: If authentication fails
        """
        pass

    @abstractmethod
    def sign_artifact(self, artifact_hash: str) -> SignedArtifact:
        """
        Sign an artifact hash using the provider's signing key.

        Args:
            artifact_hash: SHA256 hash of (source_code + lockfile_content)

        Returns:
            SignedArtifact with signature and public key
        """
        pass

    @abstractmethod
    def verify_signature(self, artifact_hash: str, signature: str) -> bool:
        """
        Verify an artifact signature.

        Args:
            artifact_hash: The hash that was signed
            signature: The signature to verify (hex-encoded)

        Returns:
            True if signature is valid
        """
        pass


class AuthenticationError(Exception):
    """Raised when authentication fails."""
    pass


def compute_artifact_hash(source_code: str, lockfile_content: str = "") -> str:
    """
    Compute the artifact hash from source code and lockfile.

    This creates an immutable identity for the artifact tuple.

    Args:
        source_code: Python source code
        lockfile_content: uv.lock content (empty string for legacy plugins)

    Returns:
        64-character hex digest (SHA-256)
    """
    combined = source_code.encode("utf-8") + lockfile_content.encode("utf-8")
    return hashlib.sha256(combined).hexdigest()


def compute_env_hash(lockfile_content: str) -> str:
    """
    Compute the environment hash from lockfile content.

    This enables deduplication of venvs with identical dependencies.

    Args:
        lockfile_content: uv.lock content

    Returns:
        64-character hex digest (SHA-256)
    """
    return hashlib.sha256(lockfile_content.encode("utf-8")).hexdigest()
