# src/casparian_flow/security/local_provider.py
"""
v5.0 Bridge Mode: Local Identity Provider.

Mode 1: Local / Free (AUTH_MODE="local")
- Goal: Zero friction "It just works."
- Auth: Implicit trust or Shared Secret
- Signing: Sentinel generates a temporary server.key on startup
- Safety: Binds to 127.0.0.1, key rotated on restart (enforces freshness)

Security Model:
- Ed25519 keypair generated on first use
- Keys stored in ~/.casparian_flow/keys/
- Optional API key for basic authentication
"""

import os
import secrets
import logging
from pathlib import Path
from typing import Optional

from casparian_flow.security.identity import (
    IdentityProvider,
    User,
    SignedArtifact,
    AuthenticationError,
)

logger = logging.getLogger(__name__)

# Try to import cryptography for Ed25519
try:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import (
        Ed25519PrivateKey,
        Ed25519PublicKey,
    )
    from cryptography.hazmat.primitives import serialization
    HAS_CRYPTOGRAPHY = True
except ImportError:
    HAS_CRYPTOGRAPHY = False
    logger.warning("cryptography package not installed. Using fallback HMAC signing.")


class LocalProvider(IdentityProvider):
    """
    Local identity provider for development and single-machine deployments.

    Features:
    - Auto-generates Ed25519 keypair on first use
    - Optional API key authentication
    - Keys persist in ~/.casparian_flow/keys/ (or ephemeral for testing)
    """

    def __init__(
        self,
        keys_dir: Optional[Path] = None,
        api_key: Optional[str] = None,
        ephemeral: bool = False,
    ):
        """
        Initialize the local provider.

        Args:
            keys_dir: Directory for key storage (default: ~/.casparian_flow/keys/)
            api_key: Optional shared secret for authentication
            ephemeral: If True, generate new keys every time (testing mode)
        """
        self.ephemeral = ephemeral
        self.api_key = api_key or os.environ.get("CASPARIAN_API_KEY")

        if keys_dir:
            self.keys_dir = keys_dir
        else:
            self.keys_dir = Path.home() / ".casparian_flow" / "keys"

        self._private_key: Optional[Ed25519PrivateKey] = None
        self._public_key: Optional[Ed25519PublicKey] = None
        self._public_key_hex: Optional[str] = None

        # HMAC fallback secret (used when cryptography is not installed)
        self._hmac_secret: Optional[str] = None

        self._initialize_keys()

    def _initialize_keys(self):
        """Load or generate signing keys."""
        if not HAS_CRYPTOGRAPHY:
            self._initialize_hmac_fallback()
            return

        if self.ephemeral:
            self._generate_new_keypair()
            logger.info("LocalProvider: Generated ephemeral Ed25519 keypair")
            return

        # Persistent mode: load or create keys
        self.keys_dir.mkdir(parents=True, exist_ok=True)
        private_key_path = self.keys_dir / "server.key"
        public_key_path = self.keys_dir / "server.pub"

        if private_key_path.exists():
            self._load_keypair(private_key_path, public_key_path)
            logger.info(f"LocalProvider: Loaded Ed25519 keypair from {self.keys_dir}")
        else:
            self._generate_new_keypair()
            self._save_keypair(private_key_path, public_key_path)
            logger.info(f"LocalProvider: Generated new Ed25519 keypair in {self.keys_dir}")

    def _initialize_hmac_fallback(self):
        """Initialize HMAC-based signing when cryptography is not available."""
        secret_path = self.keys_dir / "hmac.secret" if not self.ephemeral else None

        if secret_path and secret_path.exists():
            self._hmac_secret = secret_path.read_text().strip()
        else:
            self._hmac_secret = secrets.token_hex(32)
            if secret_path:
                self.keys_dir.mkdir(parents=True, exist_ok=True)
                secret_path.write_text(self._hmac_secret)
                secret_path.chmod(0o600)  # Restrict permissions

        logger.info("LocalProvider: Using HMAC-SHA256 fallback signing")

    def _generate_new_keypair(self):
        """Generate a new Ed25519 keypair."""
        self._private_key = Ed25519PrivateKey.generate()
        self._public_key = self._private_key.public_key()
        self._public_key_hex = self._public_key.public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        ).hex()

    def _load_keypair(self, private_path: Path, public_path: Path):
        """Load keypair from disk."""
        private_bytes = private_path.read_bytes()
        self._private_key = serialization.load_pem_private_key(
            private_bytes, password=None
        )
        self._public_key = self._private_key.public_key()
        self._public_key_hex = self._public_key.public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        ).hex()

    def _save_keypair(self, private_path: Path, public_path: Path):
        """Save keypair to disk with restricted permissions."""
        # Save private key
        private_bytes = self._private_key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.PKCS8,
            encryption_algorithm=serialization.NoEncryption(),
        )
        private_path.write_bytes(private_bytes)
        private_path.chmod(0o600)  # Owner read/write only

        # Save public key
        public_bytes = self._private_key.public_key().public_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PublicFormat.SubjectPublicKeyInfo,
        )
        public_path.write_bytes(public_bytes)

    def authenticate(self, token: Optional[str] = None) -> User:
        """
        Authenticate a user in local mode.

        If API key is configured, validates the token.
        Otherwise, returns a default local user (implicit trust).
        """
        if self.api_key:
            if not token:
                raise AuthenticationError("API key required but not provided")
            if not secrets.compare_digest(token, self.api_key):
                raise AuthenticationError("Invalid API key")
            return User(id=1, name="api_user", email="api@localhost")

        # Implicit trust mode - return local user
        import getpass
        username = getpass.getuser()
        return User(id=0, name=username, email=f"{username}@localhost")

    def sign_artifact(self, artifact_hash: str) -> SignedArtifact:
        """Sign an artifact hash using Ed25519 (or HMAC fallback)."""
        if not HAS_CRYPTOGRAPHY:
            return self._sign_hmac(artifact_hash)

        # Ed25519 signing
        message = artifact_hash.encode("utf-8")
        signature = self._private_key.sign(message)
        signature_hex = signature.hex()

        return SignedArtifact(
            artifact_hash=artifact_hash,
            signature=signature_hex,
            public_key=self._public_key_hex,
        )

    def _sign_hmac(self, artifact_hash: str) -> SignedArtifact:
        """HMAC-SHA256 fallback signing."""
        import hmac
        import hashlib

        signature = hmac.new(
            self._hmac_secret.encode("utf-8"),
            artifact_hash.encode("utf-8"),
            hashlib.sha256,
        ).hexdigest()

        return SignedArtifact(
            artifact_hash=artifact_hash,
            signature=signature,
            public_key="hmac",  # Indicates HMAC mode
        )

    def verify_signature(self, artifact_hash: str, signature: str) -> bool:
        """Verify an artifact signature."""
        if not HAS_CRYPTOGRAPHY:
            return self._verify_hmac(artifact_hash, signature)

        try:
            message = artifact_hash.encode("utf-8")
            signature_bytes = bytes.fromhex(signature)
            self._public_key.verify(signature_bytes, message)
            return True
        except Exception as e:
            logger.warning(f"Signature verification failed: {e}")
            return False

    def _verify_hmac(self, artifact_hash: str, signature: str) -> bool:
        """HMAC-SHA256 fallback verification."""
        import hmac
        import hashlib

        expected = hmac.new(
            self._hmac_secret.encode("utf-8"),
            artifact_hash.encode("utf-8"),
            hashlib.sha256,
        ).hexdigest()

        return hmac.compare_digest(expected, signature)

    def get_public_key_hex(self) -> str:
        """Get the public key in hex format."""
        if HAS_CRYPTOGRAPHY:
            return self._public_key_hex
        return "hmac"

    def rotate_keys(self):
        """
        Rotate signing keys.

        Note: This invalidates all previously signed artifacts.
        Only call this when you want to enforce freshness.
        """
        if HAS_CRYPTOGRAPHY:
            self._generate_new_keypair()
            if not self.ephemeral:
                private_path = self.keys_dir / "server.key"
                public_path = self.keys_dir / "server.pub"
                self._save_keypair(private_path, public_path)
            logger.info("LocalProvider: Rotated Ed25519 keypair")
        else:
            self._hmac_secret = secrets.token_hex(32)
            if not self.ephemeral:
                secret_path = self.keys_dir / "hmac.secret"
                secret_path.write_text(self._hmac_secret)
            logger.info("LocalProvider: Rotated HMAC secret")
