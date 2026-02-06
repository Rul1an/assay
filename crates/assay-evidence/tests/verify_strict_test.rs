//! Strict verification tests for Evidence Contract v1.
//!
//! These tests ensure the verifier enforces all contract requirements.

use assay_evidence::bundle::writer::{verify_bundle, BundleWriter};
use assay_evidence::crypto::id::compute_content_hash;
use assay_evidence::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use sha2::Digest;
use std::io::Cursor;

/// Helper to create deterministic test events.
fn create_event(seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.test.event",
        "urn:assay:test",
        "run_deterministic_test",
        seq,
        serde_json::json!({"seq": seq, "data": "test"}),
    );
    // Fixed timestamp for determinism
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event
}

/// Create a valid bundle for testing.
fn create_valid_bundle(event_count: usize) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buffer);
        for seq in 0..event_count {
            writer.add_event(create_event(seq as u64));
        }
        writer.finish().unwrap();
    }
    buffer
}

// ============================================================================
// Positive Tests
// ============================================================================

#[test]
fn test_verify_valid_bundle_single_event() {
    let bundle = create_valid_bundle(1);
    let result = verify_bundle(Cursor::new(&bundle)).unwrap();

    assert_eq!(result.event_count, 1);
    assert_eq!(result.manifest.event_count, 1);
    assert_eq!(result.manifest.run_id, "run_deterministic_test");
}

#[test]
fn test_verify_valid_bundle_multiple_events() {
    let bundle = create_valid_bundle(10);
    let result = verify_bundle(Cursor::new(&bundle)).unwrap();

    assert_eq!(result.event_count, 10);
    assert!(result.computed_run_root.starts_with("sha256:"));
}

#[test]
fn test_verify_run_root_matches() {
    let bundle = create_valid_bundle(5);
    let result = verify_bundle(Cursor::new(&bundle)).unwrap();

    // Computed run_root must match manifest
    assert_eq!(result.computed_run_root, result.manifest.run_root);
}

// ============================================================================
// Contract Violation Tests
// ============================================================================

/// Verifies that BundleWriter normalizes events by setting content_hash
/// even when the caller leaves it as None.
#[test]
fn test_writer_normalizes_missing_content_hash() {
    let mut event = create_event(0);
    event.content_hash = None; // Explicitly remove

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);
    writer.finish().unwrap();

    // Verify succeeds because BundleWriter normalized the event (set content_hash)
    let result = verify_bundle(Cursor::new(&buffer)).unwrap();
    assert_eq!(result.event_count, 1);
}

/// Proves that the verifier itself rejects events without content_hash by crafting
/// a raw tar.gz bundle that bypasses BundleWriter normalization.
#[test]
fn test_verifier_rejects_missing_content_hash_raw_tar() {
    use flate2::{Compression, GzBuilder};
    use tar::{Builder, Header};

    let mut event = create_event(0);
    // Compute the correct content_hash first (to build a valid manifest)
    let correct_hash = compute_content_hash(&event).unwrap();

    // Now serialize the event WITHOUT content_hash (simulating a broken writer)
    event.content_hash = None;
    let events_ndjson = serde_json::to_string(&event).unwrap() + "\n";

    // Build manifest that references the correct hash (but the event in the archive has None)
    let manifest = serde_json::json!({
        "schema_version": 1,
        "bundle_id": "test-missing-hash",
        "producer": {"name": "test", "version": "1.0"},
        "run_id": "run_deterministic_test",
        "event_count": 1,
        "run_root": correct_hash,
        "algorithms": {"canon": "jcs", "hash": "sha256", "root": "chain"},
        "files": {
            "events.ndjson": {
                "path": "events.ndjson",
                "sha256": format!("sha256:{}", hex::encode(sha2::Sha256::digest(events_ndjson.as_bytes()))),
                "bytes": events_ndjson.len()
            }
        }
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();

    let mut buffer = Vec::new();
    {
        let encoder = GzBuilder::new().write(&mut buffer, Compression::default());
        let mut tar = Builder::new(encoder);

        let mut header = Header::new_gnu();
        header.set_path("manifest.json").unwrap();
        header.set_size(manifest_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();
        tar.append(&header, manifest_bytes.as_slice()).unwrap();

        let events_bytes = events_ndjson.as_bytes();
        let mut header = Header::new_gnu();
        header.set_path("events.ndjson").unwrap();
        header.set_size(events_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();
        tar.append(&header, events_bytes).unwrap();

        tar.into_inner().unwrap().finish().unwrap();
    }

    // Verify must reject: content_hash is missing from the event
    let result = verify_bundle(Cursor::new(&buffer));
    assert!(
        result.is_err(),
        "Verifier must reject events without content_hash, got: {:?}",
        result
    );
}

#[test]
fn test_reject_incorrect_content_hash() {
    let mut event = create_event(0);
    // Set WRONG content_hash
    event.content_hash =
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string());

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);

    // BundleWriter should reject inconsistent hash
    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("content_hash"));
}

#[test]
fn test_reject_incorrect_event_id() {
    let mut event = create_event(0);
    event.id = "wrong_id".to_string(); // Should be "run_deterministic_test:0"

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);

    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("id"));
}

#[test]
fn test_reject_inconsistent_run_id() {
    let event1 = create_event(0);
    let mut event2 = create_event(1);
    event2.run_id = "different_run".to_string();
    event2.id = "different_run:1".to_string();

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event1);
    writer.add_event(event2);

    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("run_id"));
}

#[test]
fn test_reject_empty_bundle() {
    let mut buffer = Vec::new();
    let writer = BundleWriter::new(&mut buffer);

    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("empty"));
}

// ============================================================================
// Sequence Tests
// ============================================================================

#[test]
fn test_sequence_must_start_at_zero() {
    // Create event starting at seq=1 (wrong)
    let mut event = EvidenceEvent::new(
        "assay.test",
        "urn:assay:test",
        "run_test",
        1, // Should be 0
        serde_json::json!({}),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);

    // Writer must strictly reject gap at start (expect seq 0, got 1)
    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("Sequence gap") || err.to_string().contains("expected seq=0"));
}

#[test]
fn test_sequence_must_be_contiguous() {
    let event0 = create_event(0);
    let mut event2 = create_event(2); // Skip seq=1!
    event2.seq = 2;
    event2.id = "run_deterministic_test:2".to_string();

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event0);
    writer.add_event(event2);

    // Writer must catch gap
    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("Sequence gap"));
}

// ============================================================================
// Content Hash Self-Reference Prevention
// ============================================================================

#[test]
fn test_content_hash_excludes_self() {
    let mut event = create_event(0);

    // Compute hash without content_hash set
    let hash1 = compute_content_hash(&event).unwrap();

    // Set content_hash to something
    event.content_hash = Some("sha256:DIFFERENT_VALUE".to_string());

    // Recompute - should be SAME because content_hash is excluded
    let hash2 = compute_content_hash(&event).unwrap();

    assert_eq!(
        hash1, hash2,
        "CRITICAL: content_hash must be excluded from hash computation!"
    );
}

#[test]
fn test_content_hash_includes_payload() {
    let mut event1 = create_event(0);
    let mut event2 = create_event(0);

    event1.payload = serde_json::json!({"value": 1});
    event2.payload = serde_json::json!({"value": 2});

    let hash1 = compute_content_hash(&event1).unwrap();
    let hash2 = compute_content_hash(&event2).unwrap();

    assert_ne!(
        hash1, hash2,
        "Different payloads must produce different hashes"
    );
}

#[test]
fn test_content_hash_includes_type() {
    let mut event1 = create_event(0);
    let mut event2 = create_event(0);

    event1.type_ = "assay.type.one".to_string();
    event2.type_ = "assay.type.two".to_string();

    let hash1 = compute_content_hash(&event1).unwrap();
    let hash2 = compute_content_hash(&event2).unwrap();

    assert_ne!(
        hash1, hash2,
        "Different types must produce different hashes"
    );
}

#[test]
fn test_content_hash_excludes_metadata() {
    let event1 = create_event(0);
    let mut event2 = create_event(0);

    // Change metadata fields that should NOT affect hash
    event2.run_id = "different_run".to_string();
    event2.id = "different_run:0".to_string();
    event2.producer = "different_producer".to_string();
    event2.time = Utc.timestamp_opt(9999999999, 0).unwrap();
    event2.trace_parent = Some("00-trace-id".to_string());

    let hash1 = compute_content_hash(&event1).unwrap();
    let hash2 = compute_content_hash(&event2).unwrap();

    assert_eq!(hash1, hash2, "Metadata should NOT affect content hash");
}

#[test]
fn test_reject_extra_file() {
    let mut buffer = Vec::new();

    // Create a bundle manually with an extra file
    {
        use flate2::{Compression, GzBuilder};
        use tar::{Builder, Header};

        let encoder = GzBuilder::new().write(&mut buffer, Compression::default());
        let mut tar = Builder::new(encoder);

        // 1. Manifest
        let manifest = serde_json::json!({
            "schema_version": 1,
            "bundle_id": "test",
            "producer": {"name": "test", "version": "1.0"},
            "run_id": "run_test",
            "event_count": 0,
            "run_root": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "algorithms": {"canon": "jcs", "hash": "sha256", "root": "chain"},
            "files": {
                "events.ndjson": {
                    "path": "events.ndjson",
                    "sha256": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", // Empty hash
                    "bytes": 0
                }
            }
        });
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut header = Header::new_gnu();
        header.set_path("manifest.json").unwrap();
        header.set_size(manifest_bytes.len() as u64);
        header.set_cksum();
        tar.append(&header, manifest_bytes.as_slice()).unwrap();

        // 2. Events
        let events_bytes = b"";
        let mut header = Header::new_gnu();
        header.set_path("events.ndjson").unwrap();
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, &events_bytes[..]).unwrap();

        // 3. EXTRA FILE (Should trigger rejection)
        let extra = b"malicious content";
        let mut header = Header::new_gnu();
        header.set_path("malicious.sh").unwrap();
        header.set_size(extra.len() as u64);
        header.set_cksum();
        tar.append(&header, &extra[..]).unwrap();

        tar.into_inner().unwrap().finish().unwrap();
    }

    // Verify must fail
    match verify_bundle(Cursor::new(&buffer)) {
        Ok(_) => panic!("Verifier should have rejected extra file!"),
        Err(e) => {
            println!("Got expected error: {}", e);
            assert!(
                e.to_string().contains("Unexpected file"),
                "Error was: {}",
                e
            );
        }
    }
}

#[test]
fn test_reject_invalid_run_id_format() {
    let mut event = create_event(0);
    event.run_id = "run:test".into(); // Has colon
    event.id = "run:test:0".into();

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);

    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("Invalid run_id"));
}

#[test]
fn test_reject_invalid_source_format() {
    let mut event = create_event(0);
    event.source = "not-a-uri".into(); // No colon

    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(event);

    let err = writer.finish().unwrap_err();
    assert!(err.to_string().contains("Invalid source"));
}
