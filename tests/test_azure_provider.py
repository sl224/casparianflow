# tests/test_azure_provider.py
"""
Tests for v5.0 Bridge Mode Azure AD (Entra) identity provider.

Tests:
- Token validation logic
- Device code flow
- Group membership checks
- Ed25519 signing

Note: MSAL and PyJWT interactions are mocked. These tests verify
the integration logic, not Microsoft's libraries.
"""

import pytest
from pathlib import Path
from unittest.mock import patch, MagicMock
from datetime import datetime, timedelta

# Check if Azure dependencies are available
try:
    from casparian_flow.security.azure_provider import (
        AzureProvider,
        HAS_MSAL,
        HAS_JWT,
    )

    AZURE_DEPS_AVAILABLE = HAS_MSAL and HAS_JWT
except ImportError:
    AZURE_DEPS_AVAILABLE = False

from casparian_flow.security.identity import AuthenticationError


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureProviderInit:
    """Tests for AzureProvider initialization."""

    def test_requires_tenant_and_client_id(self, tmp_path: Path, monkeypatch):
        """Raises ValueError without required Azure config."""
        monkeypatch.delenv("AZURE_TENANT_ID", raising=False)
        monkeypatch.delenv("AZURE_CLIENT_ID", raising=False)

        with pytest.raises(ValueError, match="tenant_id and client_id required"):
            AzureProvider(keys_dir=tmp_path)

    def test_accepts_config_from_env(self, tmp_path: Path, monkeypatch):
        """Reads configuration from environment variables."""
        monkeypatch.setenv("AZURE_TENANT_ID", "test-tenant")
        monkeypatch.setenv("AZURE_CLIENT_ID", "test-client")

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(keys_dir=tmp_path)
                assert provider.tenant_id == "test-tenant"
                assert provider.client_id == "test-client"

    def test_accepts_config_from_params(self, tmp_path: Path):
        """Accepts configuration via constructor parameters."""
        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(
                    tenant_id="param-tenant",
                    client_id="param-client",
                    keys_dir=tmp_path,
                )
                assert provider.tenant_id == "param-tenant"


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureTokenValidation:
    """Tests for JWT token validation."""

    def test_validates_token_with_jwks(self, tmp_path: Path):
        """Validates token using Microsoft JWKS."""
        mock_jwks_client = MagicMock()
        mock_key = MagicMock()
        mock_jwks_client.get_signing_key_from_jwt.return_value = mock_key

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        keys_dir=tmp_path,
                    )

                    mock_decode.return_value = {
                        "oid": "user-oid-123",
                        "name": "Test User",
                        "email": "test@example.com",
                    }

                    user = provider.authenticate("fake.jwt.token")

                    mock_decode.assert_called_once()
                    assert user.name == "Test User"
                    assert user.azure_oid == "user-oid-123"

    def test_rejects_expired_token(self, tmp_path: Path):
        """Raises AuthenticationError for expired tokens."""
        import jwt

        mock_jwks_client = MagicMock()
        mock_jwks_client.get_signing_key_from_jwt.return_value = MagicMock()

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        keys_dir=tmp_path,
                    )

                    mock_decode.side_effect = jwt.ExpiredSignatureError("Token expired")

                    with pytest.raises(AuthenticationError, match="expired"):
                        provider.authenticate("expired.token")

    def test_rejects_invalid_token(self, tmp_path: Path):
        """Raises AuthenticationError for invalid tokens."""
        import jwt

        mock_jwks_client = MagicMock()
        mock_jwks_client.get_signing_key_from_jwt.return_value = MagicMock()

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        keys_dir=tmp_path,
                    )

                    mock_decode.side_effect = jwt.InvalidTokenError("Invalid")

                    with pytest.raises(AuthenticationError, match="Invalid"):
                        provider.authenticate("bad.token")


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureGroupMembership:
    """Tests for security group RBAC."""

    def test_allows_user_in_group(self, tmp_path: Path):
        """Allows authentication when user is in required group."""
        mock_jwks_client = MagicMock()
        mock_jwks_client.get_signing_key_from_jwt.return_value = MagicMock()

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        publisher_group_oid="publishers-group-oid",
                        keys_dir=tmp_path,
                    )

                    mock_decode.return_value = {
                        "oid": "user-oid",
                        "name": "Publisher User",
                        "groups": ["other-group", "publishers-group-oid"],
                    }

                    user = provider.authenticate("valid.token")
                    assert user.name == "Publisher User"

    def test_rejects_user_not_in_group(self, tmp_path: Path):
        """Rejects authentication when user not in required group."""
        mock_jwks_client = MagicMock()
        mock_jwks_client.get_signing_key_from_jwt.return_value = MagicMock()

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        publisher_group_oid="publishers-group-oid",
                        keys_dir=tmp_path,
                    )

                    mock_decode.return_value = {
                        "oid": "user-oid",
                        "name": "Non-Publisher",
                        "groups": ["other-group"],  # Missing required group
                    }

                    with pytest.raises(AuthenticationError, match="not in Casparian_Publishers"):
                        provider.authenticate("valid.token")


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureDeviceCodeFlow:
    """Tests for device code authentication flow."""

    @pytest.fixture
    def provider(self, tmp_path: Path):
        """Create provider with mocked MSAL app."""
        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication") as mock_msal:
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                mock_app = MagicMock()
                mock_msal.return_value = mock_app

                provider = AzureProvider(
                    tenant_id="test-tenant",
                    client_id="test-client",
                    keys_dir=tmp_path,
                )
                provider._msal_app = mock_app
                return provider

    def test_initiates_device_flow(self, provider, capsys):
        """Starts device code flow and displays instructions."""
        provider._msal_app.initiate_device_flow.return_value = {
            "user_code": "ABC123",
            "verification_uri": "https://microsoft.com/devicelogin",
        }
        provider._msal_app.acquire_token_by_device_flow.return_value = {
            "access_token": "access-token",
            "id_token_claims": {
                "oid": "user-oid",
                "name": "Device User",
            },
        }

        user = provider.authenticate()  # No token = device flow

        assert user.name == "Device User"

        # Verify instructions were printed
        captured = capsys.readouterr()
        assert "ABC123" in captured.out
        assert "microsoft.com/devicelogin" in captured.out

    def test_handles_device_flow_failure(self, provider):
        """Raises AuthenticationError when device flow fails."""
        provider._msal_app.initiate_device_flow.return_value = {
            "error": "authorization_pending",
            "error_description": "User cancelled",
        }

        with pytest.raises(AuthenticationError, match="Failed to initiate"):
            provider.authenticate()


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureSigning:
    """Tests for Azure provider signing (uses same Ed25519 as local)."""

    @pytest.fixture
    def provider(self, tmp_path: Path):
        """Create provider for signing tests."""
        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(
                    tenant_id="test-tenant",
                    client_id="test-client",
                    keys_dir=tmp_path,
                )
                return provider

    def test_signs_artifact(self, provider):
        """Signs artifact hash with Ed25519."""
        signed = provider.sign_artifact("test-artifact-hash")

        assert signed.artifact_hash == "test-artifact-hash"
        assert len(signed.signature) == 128  # Ed25519 signature hex
        assert len(signed.public_key) == 64  # Ed25519 public key hex

    def test_verifies_own_signature(self, provider):
        """Verifies signatures it created."""
        signed = provider.sign_artifact("artifact-to-verify")

        assert provider.verify_signature(
            "artifact-to-verify", signed.signature
        ) is True

    def test_rejects_tampered_signature(self, provider):
        """Rejects modified signatures."""
        signed = provider.sign_artifact("original-artifact")

        assert provider.verify_signature(
            "different-artifact", signed.signature
        ) is False

    def test_loads_key_from_env_var(self, tmp_path: Path, monkeypatch):
        """Loads signing key from environment variable."""
        # Generate a valid Ed25519 private key bytes (32 bytes)
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

        key = Ed25519PrivateKey.generate()
        key_bytes = key.private_bytes_raw()
        monkeypatch.setenv("CASPARIAN_SIGNING_KEY", key_bytes.hex())

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(
                    tenant_id="test-tenant",
                    client_id="test-client",
                    keys_dir=tmp_path,
                )

                # Should have loaded the key
                assert provider._private_key is not None
