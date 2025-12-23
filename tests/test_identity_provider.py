# tests/test_identity_provider.py
"""
Tests for v5.0 Bridge Mode identity providers.

Tests:
- LocalProvider Ed25519 signing and verification
- LocalProvider HMAC fallback
- Key rotation and ephemeral mode
- Factory function selection
"""

import pytest
import os
from pathlib import Path
from unittest.mock import patch

from casparian_flow.security.identity import (
    IdentityProvider,
    User,
    SignedArtifact,
    AuthenticationError,
    compute_artifact_hash,
    compute_env_hash,
)
from casparian_flow.security.local_provider import LocalProvider, HAS_CRYPTOGRAPHY
from casparian_flow.security.factory import (
    get_identity_provider,
    get_auth_mode,
    reset_provider,
)


class TestLocalProviderAuth:
    """Tests for LocalProvider authentication."""

    def test_implicit_trust_returns_local_user(self, tmp_path: Path):
        """Without API key, returns current system user."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        user = provider.authenticate()

        assert isinstance(user, User)
        assert user.id == 0  # Local user ID
        assert user.name  # Should be current username
        assert "@localhost" in user.email

    def test_api_key_auth_success(self, tmp_path: Path):
        """Valid API key authenticates successfully."""
        api_key = "test-secret-key-12345"
        provider = LocalProvider(keys_dir=tmp_path, api_key=api_key, ephemeral=True)

        user = provider.authenticate(api_key)
        assert user.name == "api_user"
        assert user.id == 1

    def test_api_key_auth_failure(self, tmp_path: Path):
        """Invalid API key raises AuthenticationError."""
        provider = LocalProvider(
            keys_dir=tmp_path, api_key="correct-key", ephemeral=True
        )

        with pytest.raises(AuthenticationError, match="Invalid API key"):
            provider.authenticate("wrong-key")

    def test_api_key_required_but_missing(self, tmp_path: Path):
        """Missing API key when required raises AuthenticationError."""
        provider = LocalProvider(
            keys_dir=tmp_path, api_key="required-key", ephemeral=True
        )

        with pytest.raises(AuthenticationError, match="API key required"):
            provider.authenticate(None)


@pytest.mark.skipif(not HAS_CRYPTOGRAPHY, reason="cryptography not installed")
class TestLocalProviderEd25519:
    """Tests for Ed25519 signing with cryptography package."""

    def test_sign_and_verify_artifact(self, tmp_path: Path):
        """Sign artifact hash and verify signature."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        artifact_hash = compute_artifact_hash("source code", "lockfile")

        signed = provider.sign_artifact(artifact_hash)

        assert isinstance(signed, SignedArtifact)
        assert signed.artifact_hash == artifact_hash
        assert len(signed.signature) == 128  # Ed25519 signature hex
        assert len(signed.public_key) == 64  # Ed25519 public key hex

    def test_verify_valid_signature(self, tmp_path: Path):
        """Verification returns True for valid signature."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        artifact_hash = "a" * 64

        signed = provider.sign_artifact(artifact_hash)
        assert provider.verify_signature(artifact_hash, signed.signature) is True

    def test_verify_invalid_signature(self, tmp_path: Path):
        """Verification returns False for tampered signature."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        artifact_hash = "a" * 64

        signed = provider.sign_artifact(artifact_hash)
        # Tamper with signature
        bad_sig = "0" * 128
        assert provider.verify_signature(artifact_hash, bad_sig) is False

    def test_verify_wrong_hash(self, tmp_path: Path):
        """Verification fails if hash doesn't match signature."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)

        signed = provider.sign_artifact("original_hash")
        assert provider.verify_signature("different_hash", signed.signature) is False

    def test_ephemeral_keys_different_each_time(self, tmp_path: Path):
        """Ephemeral mode generates new keys each instantiation."""
        provider1 = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        provider2 = LocalProvider(keys_dir=tmp_path, ephemeral=True)

        assert provider1.get_public_key_hex() != provider2.get_public_key_hex()

    def test_persistent_keys_same_across_instances(self, tmp_path: Path):
        """Persistent mode loads same keys from disk."""
        provider1 = LocalProvider(keys_dir=tmp_path, ephemeral=False)
        key1 = provider1.get_public_key_hex()

        provider2 = LocalProvider(keys_dir=tmp_path, ephemeral=False)
        key2 = provider2.get_public_key_hex()

        assert key1 == key2

    def test_key_rotation_invalidates_old_signatures(self, tmp_path: Path):
        """After key rotation, old signatures no longer verify."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)
        artifact_hash = "test_hash"

        signed = provider.sign_artifact(artifact_hash)
        assert provider.verify_signature(artifact_hash, signed.signature) is True

        # Rotate keys
        provider.rotate_keys()

        # Old signature no longer valid
        assert provider.verify_signature(artifact_hash, signed.signature) is False

    def test_keys_saved_to_disk(self, tmp_path: Path):
        """Persistent mode saves keys to specified directory."""
        provider = LocalProvider(keys_dir=tmp_path, ephemeral=False)
        provider.sign_artifact("test")  # Trigger key usage

        assert (tmp_path / "server.key").exists()
        assert (tmp_path / "server.pub").exists()

        # Private key should have restricted permissions (Unix only)
        if os.name != "nt":
            mode = (tmp_path / "server.key").stat().st_mode & 0o777
            assert mode == 0o600


class TestLocalProviderHMACFallback:
    """Tests for HMAC fallback when cryptography is not available."""

    def test_hmac_sign_and_verify(self, tmp_path: Path):
        """HMAC fallback works for signing and verification."""
        # Force HMAC mode by patching
        with patch("casparian_flow.security.local_provider.HAS_CRYPTOGRAPHY", False):
            provider = LocalProvider(keys_dir=tmp_path, ephemeral=True)

            signed = provider.sign_artifact("test_hash")
            assert signed.public_key == "hmac"  # Indicates HMAC mode

            assert provider.verify_signature("test_hash", signed.signature) is True
            assert provider.verify_signature("wrong_hash", signed.signature) is False


class TestIdentityProviderFactory:
    """Tests for the identity provider factory."""

    def test_default_mode_is_local(self, tmp_path: Path, monkeypatch):
        """Without AUTH_MODE env var, defaults to local."""
        monkeypatch.delenv("AUTH_MODE", raising=False)
        reset_provider()

        mode = get_auth_mode()
        assert mode == "local"

    def test_auth_mode_from_env(self, monkeypatch):
        """AUTH_MODE environment variable selects mode."""
        monkeypatch.setenv("AUTH_MODE", "local")
        assert get_auth_mode() == "local"

        monkeypatch.setenv("AUTH_MODE", "entra")
        assert get_auth_mode() == "entra"

    def test_invalid_auth_mode_defaults_to_local(self, monkeypatch):
        """Unknown AUTH_MODE falls back to local."""
        monkeypatch.setenv("AUTH_MODE", "unknown")
        assert get_auth_mode() == "local"

    def test_factory_creates_local_provider(self, tmp_path: Path):
        """Factory creates LocalProvider for 'local' mode."""
        provider = get_identity_provider(mode="local", keys_dir=tmp_path, ephemeral=True)
        assert isinstance(provider, LocalProvider)

    def test_factory_entra_requires_dependencies(self):
        """Factory raises ImportError if Azure deps missing for entra mode."""
        # This test verifies error handling - actual Azure tests are separate
        try:
            from casparian_flow.security.azure_provider import HAS_MSAL, HAS_JWT

            if not HAS_MSAL or not HAS_JWT:
                with pytest.raises(ImportError):
                    get_identity_provider(mode="entra")
        except ImportError:
            # Expected if msal/jwt not installed
            pass


class TestArtifactHashing:
    """Tests for artifact and environment hash functions."""

    def test_compute_artifact_hash_deterministic(self):
        """Same inputs produce same artifact hash."""
        source = "def foo(): pass"
        lockfile = "version = 1"

        hash1 = compute_artifact_hash(source, lockfile)
        hash2 = compute_artifact_hash(source, lockfile)
        assert hash1 == hash2

    def test_compute_artifact_hash_format(self):
        """Artifact hash is 64-char hex (SHA256)."""
        h = compute_artifact_hash("source", "lock")
        assert len(h) == 64
        assert all(c in "0123456789abcdef" for c in h)

    def test_compute_env_hash_empty_string(self):
        """Empty lockfile produces valid hash."""
        h = compute_env_hash("")
        assert len(h) == 64

    def test_artifact_hash_sensitive_to_whitespace(self):
        """Whitespace changes affect hash."""
        h1 = compute_artifact_hash("code", "lock")
        h2 = compute_artifact_hash("code ", "lock")
        h3 = compute_artifact_hash("code", "lock ")
        assert h1 != h2 != h3
