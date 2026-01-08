//! Ed25519 Signature Generation and Verification
//!
//! Used for signing plugin deployments to ensure integrity and authenticity.

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Sign data with Ed25519 private key
pub fn sign(data: &[u8], private_key: &SigningKey) -> Signature {
    private_key.sign(data)
}

/// Verify Ed25519 signature
pub fn verify(data: &[u8], signature: &Signature, public_key: &VerifyingKey) -> Result<()> {
    public_key
        .verify(data, signature)
        .context("Signature verification failed")
}

/// Compute SHA256 hash of data
pub fn sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_sign_and_verify() {
        let signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let data = b"test message";
        let signature = sign(data, &signing_key);

        assert!(verify(data, &signature, &verifying_key).is_ok());
    }

    #[test]
    fn test_verify_wrong_data() {
        let signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let data = b"test message";
        let signature = sign(data, &signing_key);

        let wrong_data = b"wrong message";
        assert!(verify(wrong_data, &signature, &verifying_key).is_err());
    }

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = sha256(data);
        assert_eq!(hash.len(), 64); // SHA256 is 32 bytes = 64 hex chars
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
