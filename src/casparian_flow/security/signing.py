"""
Security Module: Signing & Verification.
Designed to be extensible. Currently uses a simple SHA-256 Hash.
Can be upgraded to HMAC or PKI without changing consumer code.
"""

import hashlib
import logging

logger = logging.getLogger(__name__)


class Signer:
    """
    Abstracts the signing strategy.

    Current Strategy: SHA-256 Hash (Integrity Check).
    Future Strategy: HMAC-SHA256 (Authenticity Check) or RSA (Non-repudiation).
    """

    @staticmethod
    def sign(content: str) -> str:
        """
        Generate a signature for the given content.
        """
        # FUTURE: Use os.environ.get("CASPARIAN_SECRET") for HMAC
        # secret = os.environ.get("CASPARIAN_SECRET", "")
        # return hmac.new(secret.encode(), content.encode(), hashlib.sha256).hexdigest()

        # CURRENT: Simple Hash
        return hashlib.sha256(content.encode("utf-8")).hexdigest()

    @staticmethod
    def verify(content: str, signature: str) -> bool:
        """
        Verify the content matches the signature.
        """
        expected = Signer.sign(content)

        # Use compare_digest to prevent timing attacks (good practice even for simple hashes)
        return import_hmac_compare_digest(expected, signature)


def import_hmac_compare_digest(a, b):
    import hmac

    return hmac.compare_digest(a, b)
