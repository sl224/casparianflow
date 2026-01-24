//! SHA256 hashing for content identity
//!
//! Used for computing content-based identity of parsers and files.

use sha2::{Digest, Sha256};

/// Compute SHA256 hash of data
pub fn sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute a stable artifact hash from multiple components.
///
/// Components are separated with ASCII Unit Separator (0x1f) to avoid ambiguity.
pub fn compute_artifact_hash(
    source_code: &str,
    lockfile_content: &str,
    manifest_json: &str,
    schema_artifacts_json: &str,
) -> String {
    const SEP: u8 = 0x1f;
    let mut hasher = Sha256::new();
    for part in [
        source_code,
        lockfile_content,
        manifest_json,
        schema_artifacts_json,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([SEP]);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_compute_artifact_hash() {
        let hash1 = compute_artifact_hash("source", "lockfile", "manifest", "schemas");
        let hash2 = compute_artifact_hash("source", "lockfile", "manifest", "schemas");
        assert_eq!(hash1, hash2);

        let hash_ab = compute_artifact_hash("a", "b", "m", "s");
        let hash_ba = compute_artifact_hash("b", "a", "m", "s");
        assert_ne!(hash_ab, hash_ba);

        let hash3 = compute_artifact_hash("source1", "lockfile", "manifest", "schemas");
        let hash4 = compute_artifact_hash("source2", "lockfile", "manifest", "schemas");
        assert_ne!(hash3, hash4);
    }
}
