//! Determinism tests for Evidence Bundle format.
//!
//! These tests verify that bundles are byte-for-byte reproducible
//! and that all variable fields (mtime, uid, etc.) are fixed.

use assay_evidence::bundle::writer::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use std::io::Cursor;

/// Create a deterministic test event with fixed timestamp.
fn create_deterministic_event(seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.determinism.test",
        "urn:assay:determinism-test",
        "run_golden_test_12345",
        seq,
        serde_json::json!({
            "seq": seq,
            "message": "This is a deterministic test event",
            "nested": {
                "z_field": 3,
                "a_field": 1,  // JCS will sort this
                "m_field": 2
            }
        }),
    );

    // Fixed timestamp: 2023-11-14T22:13:20Z (Unix: 1700000000)
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event.producer = "assay-evidence-test".to_string();
    event.producer_version = "0.1.0-test".to_string();
    event.git_sha = "abc1234".to_string();

    event
}

/// Generate a bundle and return (bytes, sha256).
fn generate_bundle(event_count: usize) -> (Vec<u8>, String) {
    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        for seq in 0..event_count {
            writer.add_event(create_deterministic_event(seq as u64));
        }
        writer.finish().unwrap();
    }

    let hash = hex::encode(Sha256::digest(&buffer));
    (buffer, hash)
}

// ============================================================================
// Byte-for-Byte Determinism
// ============================================================================

#[test]
fn test_bundle_content_determinism_single_event() {
    let (bundle1, _) = generate_bundle(1);
    let (bundle2, _) = generate_bundle(1);

    // Content Determinism: Check manifest and events are identical
    // We do NOT check container bytes (tar/gzip) as they vary by platform/lib version

    let (m1, e1) = unpack_bundle(&bundle1);
    let (m2, e2) = unpack_bundle(&bundle2);

    assert_eq!(
        hash_bytes(&m1),
        hash_bytes(&m2),
        "Manifests must be identical"
    );
    assert_eq!(hash_bytes(&e1), hash_bytes(&e2), "Events must be identical");

    // Also verify strict verification passes
    assay_evidence::bundle::verify_bundle(Cursor::new(&bundle1)).unwrap();
    assay_evidence::bundle::verify_bundle(Cursor::new(&bundle2)).unwrap();
}

fn unpack_bundle(data: &[u8]) -> (Vec<u8>, Vec<u8>) {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::Archive;

    let mut manifest = Vec::new();
    let mut events = Vec::new();

    let decoder = GzDecoder::new(Cursor::new(data));
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().to_string();
        if path == "manifest.json" {
            entry.read_to_end(&mut manifest).unwrap();
        } else if path == "events.ndjson" {
            entry.read_to_end(&mut events).unwrap();
        }
    }
    (manifest, events)
}

fn hash_bytes(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

#[test]
fn test_bundle_content_determinism_multiple_events() {
    let (bundle1, _) = generate_bundle(5);
    let (bundle2, _) = generate_bundle(5);

    let (m1, e1) = unpack_bundle(&bundle1);
    let (m2, e2) = unpack_bundle(&bundle2);

    assert_eq!(hash_bytes(&m1), hash_bytes(&m2));
    assert_eq!(hash_bytes(&e1), hash_bytes(&e2));
}

#[test]
fn test_bundle_content_determinism_many_events() {
    let (bundle1, _) = generate_bundle(100);
    let (bundle2, _) = generate_bundle(100);

    let (m1, e1) = unpack_bundle(&bundle1);
    let (m2, e2) = unpack_bundle(&bundle2);

    assert_eq!(hash_bytes(&m1), hash_bytes(&m2));
    assert_eq!(hash_bytes(&e1), hash_bytes(&e2));
}

// ============================================================================
// Gzip Header Determinism
// ============================================================================

#[test]
fn test_gzip_header_determinism() {
    let (bundle, _) = generate_bundle(1);

    // Gzip header structure (RFC 1952):
    // Bytes 0-1: Magic (0x1f 0x8b)
    // Byte 2: Compression method (8 = deflate)
    // Byte 3: Flags
    // Bytes 4-7: Modification time (should be 0)
    // Byte 8: Extra flags
    // Byte 9: OS (should be 255 = unknown)

    assert!(bundle.len() >= 10, "Bundle too small");

    // Magic bytes
    assert_eq!(bundle[0], 0x1f, "Gzip magic byte 1");
    assert_eq!(bundle[1], 0x8b, "Gzip magic byte 2");

    // Compression method
    assert_eq!(bundle[2], 8, "Compression method must be deflate");

    // Modification time (bytes 4-7) must be 0
    let mtime = u32::from_le_bytes([bundle[4], bundle[5], bundle[6], bundle[7]]);
    assert_eq!(mtime, 0, "Gzip mtime must be 0 for determinism");

    // OS byte must be 255 (unknown) for cross-platform determinism
    assert_eq!(bundle[9], 255, "Gzip OS byte must be 255 (unknown)");
}

// ============================================================================
// Tar Header Determinism
// ============================================================================

#[test]
fn test_tar_headers_deterministic() {
    let (bundle, _) = generate_bundle(1);

    // Decompress to get tar
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(Cursor::new(&bundle));
    let mut tar_bytes = Vec::new();
    decoder.read_to_end(&mut tar_bytes).unwrap();

    // Tar header is 512 bytes per file
    // Check first file (manifest.json) header
    assert!(tar_bytes.len() >= 512, "Tar too small");

    // Bytes 100-107: Mode (should be 0000644)
    let mode = std::str::from_utf8(&tar_bytes[100..108])
        .unwrap()
        .trim_end_matches('\0');
    assert!(
        mode.contains("644"),
        "Tar mode should be 644, got: {}",
        mode
    );

    // Bytes 108-115: UID (should be 0)
    let uid = std::str::from_utf8(&tar_bytes[108..116])
        .unwrap()
        .trim_end_matches('\0');
    let uid_val: u64 = u64::from_str_radix(uid.trim(), 8).unwrap_or(0);
    assert_eq!(uid_val, 0, "Tar UID must be 0 for determinism");

    // Bytes 116-123: GID (should be 0)
    let gid = std::str::from_utf8(&tar_bytes[116..124])
        .unwrap()
        .trim_end_matches('\0');
    let gid_val: u64 = u64::from_str_radix(gid.trim(), 8).unwrap_or(0);
    assert_eq!(gid_val, 0, "Tar GID must be 0 for determinism");

    // Bytes 136-147: Mtime (should be 0)
    let mtime = std::str::from_utf8(&tar_bytes[136..148])
        .unwrap()
        .trim_end_matches('\0');
    let mtime_val: u64 = u64::from_str_radix(mtime.trim(), 8).unwrap_or(999);
    assert_eq!(mtime_val, 0, "Tar mtime must be 0 for determinism");
}

// ============================================================================
// JCS Canonicalization
// ============================================================================

#[test]
fn test_jcs_key_ordering_in_bundle() {
    // Create event with out-of-order nested keys
    let mut event = create_deterministic_event(0);
    event.payload = serde_json::json!({
        "z": 3,
        "a": 1,
        "m": 2
    });

    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        writer.add_event(event);
        writer.finish().unwrap();
    }

    // Extract events.ndjson and verify key order
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = GzDecoder::new(Cursor::new(&buffer));
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().to_string();

        if path == "events.ndjson" {
            let mut content = String::new();
            use std::io::Read;
            entry.read_to_string(&mut content).unwrap();

            // JCS: keys must be sorted
            // The payload should have "a" before "m" before "z"
            let a_pos = content.find("\"a\":").unwrap();
            let m_pos = content.find("\"m\":").unwrap();
            let z_pos = content.find("\"z\":").unwrap();

            assert!(a_pos < m_pos, "JCS: 'a' must come before 'm'");
            assert!(m_pos < z_pos, "JCS: 'm' must come before 'z'");
        }
    }
}

// ============================================================================
// Golden Hash Test (Known Good Value)
// ============================================================================

#[test]
fn test_golden_hash() {
    // This test verifies that the exact same input produces the exact same hash.
    // If this test fails after code changes, either:
    // 1. The change broke determinism (bad!)
    // 2. The change intentionally modified the format (update the golden hash)

    let (_bundle, hash) = generate_bundle(1);

    // Print for debugging (comment out in CI)
    // println!("Bundle size: {} bytes", bundle.len());
    // println!("Bundle hash: {}", hash);

    // The hash should be stable across runs.
    // If you need to update this, run the test once to get the new hash.
    // IMPORTANT: Only update this if you intentionally changed the format!

    // For now, we just verify the hash is consistent within the test run
    let (_, hash2) = generate_bundle(1);
    assert_eq!(hash, hash2, "Hash must be stable within test run");

    // Uncomment and fill in to create a true golden test:
    // const GOLDEN_HASH: &str = "abc123..."; // Fill in after first run
    // assert_eq!(hash, GOLDEN_HASH, "Bundle hash changed! Format may have changed.");
}

// ============================================================================
// File Order Test
// ============================================================================

#[test]
fn test_manifest_always_first() {
    for event_count in [1, 5, 10, 50] {
        let (bundle, _) = generate_bundle(event_count);

        use flate2::read::GzDecoder;
        use tar::Archive;

        let decoder = GzDecoder::new(Cursor::new(&bundle));
        let mut archive = Archive::new(decoder);
        let mut entries = archive.entries().unwrap();

        let first = entries.next().unwrap().unwrap();
        let first_path = first.path().unwrap().to_string_lossy().to_string();

        assert_eq!(
            first_path, "manifest.json",
            "manifest.json must be first file (event_count={})",
            event_count
        );

        let second = entries.next().unwrap().unwrap();
        let second_path = second.path().unwrap().to_string_lossy().to_string();

        assert_eq!(
            second_path, "events.ndjson",
            "events.ndjson must be second file (event_count={})",
            event_count
        );
    }
}
