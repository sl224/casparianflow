//! Property tests for output filename collision resistance.
//!
//! Validates that the output_filename function produces unique filenames
//! for distinct job_ids, ensuring no collisions even for sequential numeric IDs.

use casparian_sinks::output_filename;
use proptest::prelude::*;
use std::collections::HashSet;

/// Test that output_filename produces 16 hex characters for the job_id portion.
#[test]
fn test_output_filename_length() {
    let filename = output_filename("test", "12345678-abcd-1234-abcd-123456789abc", "parquet");

    // Format: {output_name}_{16_hex_chars}.{extension}
    // So "test_" + 16 chars + ".parquet"
    assert_eq!(filename.len(), "test_".len() + 16 + ".parquet".len());

    // Extract the hash portion
    let hash_portion = &filename["test_".len()..filename.len() - ".parquet".len()];
    assert_eq!(hash_portion.len(), 16);

    // Verify it's all hex characters
    assert!(
        hash_portion.chars().all(|c| c.is_ascii_hexdigit()),
        "Hash portion should be all hex digits, got: {}",
        hash_portion
    );
}

/// Test that identical job_ids produce identical filenames (determinism).
#[test]
fn test_output_filename_deterministic() {
    let job_id = "test-job-123";
    let filename1 = output_filename("output", job_id, "parquet");
    let filename2 = output_filename("output", job_id, "parquet");
    assert_eq!(filename1, filename2);
}

/// Test that different job_ids produce different filenames.
#[test]
fn test_output_filename_different_jobs() {
    let filename1 = output_filename("output", "job-1", "parquet");
    let filename2 = output_filename("output", "job-2", "parquet");
    assert_ne!(filename1, filename2);
}

/// Property test: Generate 100k sequential numeric IDs and verify no collisions.
///
/// This tests the key invariant: no two distinct job_ids can produce the same filename.
#[test]
fn test_no_collisions_sequential_ids() {
    let mut filenames: HashSet<String> = HashSet::with_capacity(100_000);

    for i in 0..100_000u64 {
        let job_id = format!("{}", i);
        let filename = output_filename("output", &job_id, "parquet");
        let is_new = filenames.insert(filename.clone());
        assert!(
            is_new,
            "Collision detected for job_id {} producing filename {}",
            job_id,
            filename
        );
    }

    assert_eq!(filenames.len(), 100_000);
}

/// Property test: Generate 100k sequential UUIDs and verify no collisions.
///
/// UUIDs are a common format for job_ids in practice.
#[test]
fn test_no_collisions_uuid_like_ids() {
    let mut filenames: HashSet<String> = HashSet::with_capacity(100_000);

    for i in 0..100_000u64 {
        // Generate a UUID-like string with sequential component
        let job_id = format!(
            "{:08x}-0000-0000-0000-{:012x}",
            (i >> 32) as u32,
            i & 0xFFFFFFFFFFFF
        );
        let filename = output_filename("output", &job_id, "parquet");
        let is_new = filenames.insert(filename.clone());
        assert!(
            is_new,
            "Collision detected for job_id {} producing filename {}",
            job_id,
            filename
        );
    }

    assert_eq!(filenames.len(), 100_000);
}

// Property tests using proptest macro
proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn test_no_collisions_random_ids(
        id1 in "[a-zA-Z0-9_-]{1,64}",
        id2 in "[a-zA-Z0-9_-]{1,64}",
    ) {
        // If ids are different, filenames should be different
        if id1 != id2 {
            let filename1 = output_filename("output", &id1, "parquet");
            let filename2 = output_filename("output", &id2, "parquet");
            prop_assert_ne!(
                filename1,
                filename2,
                "Different job_ids {} and {} produced same filename",
                id1,
                id2
            );
        }
    }

    /// Test that the hash portion is always exactly 16 hex characters.
    #[test]
    fn test_hash_portion_length(job_id in ".{1,256}") {
        let filename = output_filename("test", &job_id, "parquet");

        // Extract the hash portion between "test_" and ".parquet"
        let prefix = "test_";
        let suffix = ".parquet";
        prop_assert!(filename.starts_with(prefix));
        prop_assert!(filename.ends_with(suffix));

        let hash_portion = &filename[prefix.len()..filename.len() - suffix.len()];
        prop_assert_eq!(
            hash_portion.len(),
            16,
            "Hash portion should be 16 chars, got {} for job_id {}",
            hash_portion.len(),
            job_id
        );
    }

    /// Test that different output names produce different filenames for same job_id.
    #[test]
    fn test_output_name_affects_filename(
        output1 in "[a-zA-Z][a-zA-Z0-9_]{0,31}",
        output2 in "[a-zA-Z][a-zA-Z0-9_]{0,31}",
        job_id in "[a-zA-Z0-9_-]{1,64}",
    ) {
        if output1 != output2 {
            let filename1 = output_filename(&output1, &job_id, "parquet");
            let filename2 = output_filename(&output2, &job_id, "parquet");
            prop_assert_ne!(
                filename1,
                filename2,
                "Different output names {} and {} should produce different filenames",
                output1,
                output2
            );
        }
    }
}

/// Test specifically that similar job_ids (differing only in trailing digits) don't collide.
/// This was the original bug: truncating to 8 chars caused collisions for IDs with same prefix.
#[test]
fn test_similar_prefix_ids_no_collision() {
    let base = "12345678-abcd-1234-abcd-";

    let mut filenames: HashSet<String> = HashSet::new();

    // Generate 1000 IDs that share the same 24-char prefix
    for i in 0..1000u32 {
        let job_id = format!("{}{:012x}", base, i);
        let filename = output_filename("output", &job_id, "parquet");
        let is_new = filenames.insert(filename.clone());
        assert!(
            is_new,
            "Collision detected for job_id {} producing filename {}",
            job_id,
            filename
        );
    }

    assert_eq!(filenames.len(), 1000);
}

/// Test that the old 8-char truncation would have caused collisions.
/// This documents the bug we fixed.
#[test]
fn test_old_truncation_would_collide() {
    // These two IDs have the same first 8 characters
    let id1 = "12345678-aaaa";
    let id2 = "12345678-bbbb";

    // Old truncation would produce the same prefix
    let old_prefix1 = &id1[..8];
    let old_prefix2 = &id2[..8];
    assert_eq!(old_prefix1, old_prefix2, "Old truncation collides");

    // New blake3 hash produces different prefixes
    let filename1 = output_filename("test", id1, "parquet");
    let filename2 = output_filename("test", id2, "parquet");
    assert_ne!(filename1, filename2, "New hash should not collide");
}
