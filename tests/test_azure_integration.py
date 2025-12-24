# tests/test_azure_integration.py
"""
Azure AD (Entra) Integration Tests with VCR.py

Recording real interactions with Microsoft Entra ID, with automatic secret scrubbing.

RECORDING MODE (First run with credentials):
1. Export environment variables:
   export AZURE_TENANT_ID="your-tenant-id"
   export AZURE_CLIENT_ID="your-client-id"

2. Run tests:
   uv run pytest tests/test_azure_integration.py -v

3. Cassettes are recorded to tests/cassettes/ with secrets scrubbed

REPLAY MODE (CI or subsequent runs):
- Just run: uv run pytest tests/test_azure_integration.py -v
- No credentials needed, uses cassettes
"""

import pytest
import os
from pathlib import Path

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


@pytest.mark.integration
@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
@pytest.mark.vcr()
def test_real_azure_device_code_flow(tmp_path):
    """
    Integration test: AzureProvider initialization with real OIDC discovery.

    - First run: Records OIDC discovery HTTP calls to Microsoft servers
    - Subsequent runs: Replays from cassette
    """
    tenant = os.getenv("AZURE_TENANT_ID")
    client = os.getenv("AZURE_CLIENT_ID")
    cassette_path = Path("tests/cassettes/test_real_azure_device_code_flow.yaml")

    # Use real values for recording, dummy for replay
    tenant_id = tenant or "test-tenant-id"
    client_id = client or "test-client-id"

    # Initialize Provider (triggers OIDC discovery HTTP call)
    provider = AzureProvider(
        tenant_id=tenant_id,
        client_id=client_id,
        keys_dir=tmp_path
    )

    # Verify provider initialization
    assert provider.tenant_id == tenant_id
    assert provider.client_id == client_id
    assert provider._msal_app is not None
    assert provider._jwks_client is not None
    assert provider._private_key is not None
    assert provider._public_key_hex is not None


@pytest.mark.integration
@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
@pytest.mark.vcr()
def test_azure_signing_and_verification(tmp_path):
    """
    Integration test: Ed25519 signing with AzureProvider.

    Tests artifact signing and verification (no network calls after init).
    """
    tenant = os.getenv("AZURE_TENANT_ID", "test-tenant-id")
    client = os.getenv("AZURE_CLIENT_ID", "test-client-id")

    provider = AzureProvider(
        tenant_id=tenant,
        client_id=client,
        keys_dir=tmp_path
    )

    # Test signing
    test_artifact_hash = "abcdef1234567890" * 4  # 64 char hash
    signed = provider.sign_artifact(test_artifact_hash)

    assert signed.artifact_hash == test_artifact_hash
    assert signed.signature is not None
    assert len(signed.signature) == 128  # Ed25519 signature = 64 bytes = 128 hex
    assert signed.public_key == provider._public_key_hex

    # Test verification
    is_valid = provider.verify_signature(test_artifact_hash, signed.signature)
    assert is_valid is True

    # Test invalid signature
    bad_signature = "0" * 128
    is_invalid = provider.verify_signature(test_artifact_hash, bad_signature)
    assert is_invalid is False


# =============================================================================
# Unit Tests (with mocks for configuration logic)
# =============================================================================


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureProviderConfiguration:
    """Test configuration logic (uses mocks to avoid real HTTP calls)."""

    def test_provider_initialization_with_env_vars(self, tmp_path, monkeypatch):
        """Test provider picks up config from environment variables."""
        from unittest.mock import patch

        monkeypatch.setenv("AZURE_TENANT_ID", "env-tenant-123")
        monkeypatch.setenv("AZURE_CLIENT_ID", "env-client-456")

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(keys_dir=tmp_path)

                assert provider.tenant_id == "env-tenant-123"
                assert provider.client_id == "env-client-456"

    def test_provider_initialization_with_explicit_params(self, tmp_path):
        """Test provider uses explicit parameters over env vars."""
        from unittest.mock import patch

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider = AzureProvider(
                    tenant_id="explicit-tenant",
                    client_id="explicit-client",
                    keys_dir=tmp_path
                )

                assert provider.tenant_id == "explicit-tenant"
                assert provider.client_id == "explicit-client"

    def test_missing_credentials_raises_error(self, tmp_path, monkeypatch):
        """Verify missing credentials raise ValueError."""
        monkeypatch.delenv("AZURE_TENANT_ID", raising=False)
        monkeypatch.delenv("AZURE_CLIENT_ID", raising=False)

        with pytest.raises(ValueError, match="tenant_id and client_id required"):
            AzureProvider(keys_dir=tmp_path)

    def test_signing_key_persistence(self, tmp_path):
        """Test that signing keys persist across provider instances."""
        from unittest.mock import patch

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient"):
                provider1 = AzureProvider(
                    tenant_id="test-tenant",
                    client_id="test-client",
                    keys_dir=tmp_path
                )
                pubkey1 = provider1._public_key_hex

                # Create new provider instance with same keys_dir
                provider2 = AzureProvider(
                    tenant_id="test-tenant",
                    client_id="test-client",
                    keys_dir=tmp_path
                )
                pubkey2 = provider2._public_key_hex

                # Same key should be loaded
                assert pubkey1 == pubkey2

                # Verify key file exists
                key_file = tmp_path / "azure_signing.key"
                assert key_file.exists()


@pytest.mark.skipif(not AZURE_DEPS_AVAILABLE, reason="msal/PyJWT not installed")
class TestAzureErrorHandling:
    """Test error handling (uses mocks)."""

    def test_expired_token_handling(self, tmp_path):
        """Test handling of expired tokens."""
        from unittest.mock import patch, MagicMock
        import jwt

        mock_jwks_client = MagicMock()
        mock_signing_key = MagicMock()
        mock_signing_key.key = "mock-key"
        mock_jwks_client.get_signing_key_from_jwt.return_value = mock_signing_key

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
                        provider.authenticate("expired.token.here")

    def test_invalid_token_handling(self, tmp_path):
        """Test handling of invalid tokens."""
        from unittest.mock import patch, MagicMock
        import jwt

        mock_jwks_client = MagicMock()
        mock_signing_key = MagicMock()
        mock_signing_key.key = "mock-key"
        mock_jwks_client.get_signing_key_from_jwt.return_value = mock_signing_key

        with patch("casparian_flow.security.azure_provider.msal.PublicClientApplication"):
            with patch("casparian_flow.security.azure_provider.PyJWKClient", return_value=mock_jwks_client):
                with patch("casparian_flow.security.azure_provider.jwt.decode") as mock_decode:
                    provider = AzureProvider(
                        tenant_id="test-tenant",
                        client_id="test-client",
                        keys_dir=tmp_path,
                    )

                    mock_decode.side_effect = jwt.InvalidTokenError("Invalid token")

                    with pytest.raises(AuthenticationError, match="Invalid token"):
                        provider.authenticate("invalid.token.here")
