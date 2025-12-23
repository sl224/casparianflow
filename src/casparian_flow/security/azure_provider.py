# src/casparian_flow/security/azure_provider.py
"""
v5.0 Bridge Mode: Azure AD Identity Provider.

Mode 2: Enterprise (AUTH_MODE="entra")
- Goal: Zero Trust
- Auth: MSAL for Azure AD token acquisition, PyJWT for validation
- RBAC: Checks if user is in Casparian_Publishers Security Group
- Signing: Uses persistent HSM-backed key or secure Env Var

Security Model:
- Device Code Flow for CLI authentication (headless-friendly)
- JWKS-based token validation (fetches Microsoft's public keys)
- Group membership validation for RBAC
- Ed25519 signing with persistent keys (or HSM)
"""

import os
import json
import logging
from pathlib import Path
from typing import Optional, Dict, Any
from datetime import datetime, timedelta

from casparian_flow.security.identity import (
    IdentityProvider,
    User,
    SignedArtifact,
    AuthenticationError,
)

logger = logging.getLogger(__name__)

# Try to import Azure dependencies
try:
    import msal
    HAS_MSAL = True
except ImportError:
    HAS_MSAL = False
    logger.debug("msal package not installed. Azure auth unavailable.")

try:
    import jwt
    from jwt import PyJWKClient
    HAS_JWT = True
except ImportError:
    HAS_JWT = False
    logger.debug("PyJWT package not installed. Token validation unavailable.")

try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import (
        Ed25519PrivateKey,
        Ed25519PublicKey,
    )
    from cryptography.hazmat.primitives import serialization
    HAS_CRYPTOGRAPHY = True
except ImportError:
    HAS_CRYPTOGRAPHY = False


class AzureProvider(IdentityProvider):
    """
    Azure AD identity provider for enterprise deployments.

    Features:
    - MSAL Device Code Flow for CLI authentication
    - JWKS-based JWT validation
    - Security group membership checks (RBAC)
    - Persistent Ed25519 signing keys
    """

    # Microsoft's JWKS endpoint for token validation
    AZURE_JWKS_URL = "https://login.microsoftonline.com/common/discovery/v2.0/keys"

    def __init__(
        self,
        tenant_id: Optional[str] = None,
        client_id: Optional[str] = None,
        publisher_group_oid: Optional[str] = None,
        keys_dir: Optional[Path] = None,
        signing_key_env: Optional[str] = None,
    ):
        """
        Initialize the Azure provider.

        Args:
            tenant_id: Azure AD tenant ID (or AZURE_TENANT_ID env var)
            client_id: Azure AD application client ID (or AZURE_CLIENT_ID env var)
            publisher_group_oid: Object ID of Casparian_Publishers security group
            keys_dir: Directory for signing key storage
            signing_key_env: Environment variable containing the signing key
        """
        self.tenant_id = tenant_id or os.environ.get("AZURE_TENANT_ID")
        self.client_id = client_id or os.environ.get("AZURE_CLIENT_ID")
        self.publisher_group_oid = publisher_group_oid or os.environ.get(
            "CASPARIAN_PUBLISHER_GROUP_OID"
        )
        self.signing_key_env = signing_key_env or "CASPARIAN_SIGNING_KEY"

        if keys_dir:
            self.keys_dir = keys_dir
        else:
            self.keys_dir = Path.home() / ".casparian_flow" / "keys"

        # Validate requirements
        if not HAS_MSAL:
            raise ImportError(
                "msal package required for Azure authentication. "
                "Install with: pip install msal"
            )
        if not HAS_JWT:
            raise ImportError(
                "PyJWT package required for token validation. "
                "Install with: pip install PyJWT[crypto]"
            )
        if not HAS_CRYPTOGRAPHY:
            raise ImportError(
                "cryptography package required for signing. "
                "Install with: pip install cryptography"
            )

        if not self.tenant_id or not self.client_id:
            raise ValueError(
                "Azure tenant_id and client_id required. "
                "Set AZURE_TENANT_ID and AZURE_CLIENT_ID environment variables."
            )

        # Initialize MSAL app
        self._msal_app = msal.PublicClientApplication(
            self.client_id,
            authority=f"https://login.microsoftonline.com/{self.tenant_id}",
        )

        # Initialize JWKS client for token validation
        self._jwks_client = PyJWKClient(self.AZURE_JWKS_URL)

        # Initialize signing keys
        self._private_key: Optional[Ed25519PrivateKey] = None
        self._public_key: Optional[Ed25519PublicKey] = None
        self._public_key_hex: Optional[str] = None
        self._initialize_signing_key()

        # Token cache
        self._token_cache: Dict[str, Any] = {}

    def _initialize_signing_key(self):
        """Load or generate signing key."""
        # First, try environment variable (HSM/secure secret)
        env_key = os.environ.get(self.signing_key_env)
        if env_key:
            try:
                key_bytes = bytes.fromhex(env_key)
                self._private_key = Ed25519PrivateKey.from_private_bytes(key_bytes)
                self._public_key = self._private_key.public_key()
                self._public_key_hex = self._public_key.public_bytes(
                    encoding=serialization.Encoding.Raw,
                    format=serialization.PublicFormat.Raw,
                ).hex()
                logger.info("AzureProvider: Loaded signing key from environment")
                return
            except Exception as e:
                logger.warning(f"Failed to load signing key from env: {e}")

        # Fall back to file-based key
        self.keys_dir.mkdir(parents=True, exist_ok=True)
        private_key_path = self.keys_dir / "azure_signing.key"

        if private_key_path.exists():
            private_bytes = private_key_path.read_bytes()
            self._private_key = serialization.load_pem_private_key(
                private_bytes, password=None
            )
            logger.info(f"AzureProvider: Loaded signing key from {private_key_path}")
        else:
            self._private_key = Ed25519PrivateKey.generate()
            private_bytes = self._private_key.private_bytes(
                encoding=serialization.Encoding.PEM,
                format=serialization.PrivateFormat.PKCS8,
                encryption_algorithm=serialization.NoEncryption(),
            )
            private_key_path.write_bytes(private_bytes)
            private_key_path.chmod(0o600)
            logger.info(f"AzureProvider: Generated signing key at {private_key_path}")

        self._public_key = self._private_key.public_key()
        self._public_key_hex = self._public_key.public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        ).hex()

    def authenticate(self, token: Optional[str] = None) -> User:
        """
        Authenticate a user using Azure AD.

        If token is provided, validates it as a JWT.
        Otherwise, initiates Device Code Flow for interactive auth.
        """
        if token:
            return self._validate_token(token)
        return self._device_code_flow()

    def _validate_token(self, token: str) -> User:
        """Validate an Azure AD JWT token."""
        try:
            # Get the signing key from JWKS
            signing_key = self._jwks_client.get_signing_key_from_jwt(token)

            # Decode and validate the token
            claims = jwt.decode(
                token,
                signing_key.key,
                algorithms=["RS256"],
                audience=self.client_id,
                issuer=f"https://login.microsoftonline.com/{self.tenant_id}/v2.0",
            )

            # Extract user info
            user = User(
                id=hash(claims.get("oid", claims.get("sub"))) % (10**9),
                name=claims.get("name", claims.get("preferred_username", "unknown")),
                email=claims.get("email", claims.get("preferred_username")),
                azure_oid=claims.get("oid"),
            )

            # Check group membership if configured
            if self.publisher_group_oid:
                groups = claims.get("groups", [])
                if self.publisher_group_oid not in groups:
                    raise AuthenticationError(
                        f"User not in Casparian_Publishers group. "
                        f"Required group OID: {self.publisher_group_oid}"
                    )

            logger.info(f"Authenticated user: {user.name} (OID: {user.azure_oid})")
            return user

        except jwt.ExpiredSignatureError:
            raise AuthenticationError("Token has expired")
        except jwt.InvalidTokenError as e:
            raise AuthenticationError(f"Invalid token: {e}")

    def _device_code_flow(self) -> User:
        """
        Initiate Device Code Flow for interactive authentication.

        This is suitable for CLI use where browser-based auth isn't possible.
        """
        scopes = [f"{self.client_id}/.default"]

        # Start device code flow
        flow = self._msal_app.initiate_device_flow(scopes=scopes)

        if "user_code" not in flow:
            raise AuthenticationError(
                f"Failed to initiate device flow: {flow.get('error_description')}"
            )

        # Display instructions to user
        print("\n" + "=" * 60)
        print("AZURE AUTHENTICATION REQUIRED")
        print("=" * 60)
        print(f"\nTo sign in, visit: {flow['verification_uri']}")
        print(f"Enter code: {flow['user_code']}")
        print("\nWaiting for authentication...")
        print("=" * 60 + "\n")

        # Wait for user to authenticate
        result = self._msal_app.acquire_token_by_device_flow(flow)

        if "access_token" not in result:
            error_desc = result.get("error_description", "Unknown error")
            raise AuthenticationError(f"Authentication failed: {error_desc}")

        # Cache the token
        self._token_cache = result

        # Extract user info from ID token
        id_token = result.get("id_token_claims", {})
        user = User(
            id=hash(id_token.get("oid", id_token.get("sub", ""))) % (10**9),
            name=id_token.get("name", id_token.get("preferred_username", "unknown")),
            email=id_token.get("email", id_token.get("preferred_username")),
            azure_oid=id_token.get("oid"),
        )

        logger.info(f"Authenticated via Device Code Flow: {user.name}")
        return user

    def sign_artifact(self, artifact_hash: str) -> SignedArtifact:
        """Sign an artifact hash using Ed25519."""
        message = artifact_hash.encode("utf-8")
        signature = self._private_key.sign(message)
        signature_hex = signature.hex()

        return SignedArtifact(
            artifact_hash=artifact_hash,
            signature=signature_hex,
            public_key=self._public_key_hex,
        )

    def verify_signature(self, artifact_hash: str, signature: str) -> bool:
        """Verify an artifact signature."""
        try:
            message = artifact_hash.encode("utf-8")
            signature_bytes = bytes.fromhex(signature)
            self._public_key.verify(signature_bytes, message)
            return True
        except Exception as e:
            logger.warning(f"Signature verification failed: {e}")
            return False

    def get_access_token(self) -> Optional[str]:
        """Get the cached access token (for API calls)."""
        return self._token_cache.get("access_token")

    def get_public_key_hex(self) -> str:
        """Get the public key in hex format."""
        return self._public_key_hex
