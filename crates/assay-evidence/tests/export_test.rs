use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use std::io::Cursor;

#[test]
fn test_bundle_determinism() {
    // 1. Create two identical bundles in memory
    let bundle1 = generate_bundle();
    let bundle2 = generate_bundle();

    // 2. Hash them
    let hash1 = sha256_digest(&bundle1);
    let hash2 = sha256_digest(&bundle2);

    // 3. Verify exact byte match (Determinism Contract)
    assert_eq!(hash1, hash2, "Bundles must be byte-for-byte identical");

    // 4. Verify content (sanity check)
    // Decompress and verify manifest is first
    let tar_gz = Cursor::new(bundle1.clone()); // Clone for first use
    let decoder = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(decoder);

    let mut entries = archive.entries().unwrap();

    // First entry MUST be manifest.json
    let manifest_entry = entries.next().expect("Manifest missing").unwrap();
    assert_eq!(
        manifest_entry.path().unwrap().to_str().unwrap(),
        "manifest.json"
    );

    // Verify Bundle Contract (New Self-Check)
    let mut verify_cursor = Cursor::new(bundle1.clone());
    assay_evidence::bundle::verify_bundle(&mut verify_cursor).expect("verify_bundle failed");

    // Second entry MUST be events.ndjson
    let events_entry = entries.next().expect("Events missing").unwrap();
    assert_eq!(
        events_entry.path().unwrap().to_str().unwrap(),
        "events.ndjson"
    );
}

fn generate_bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);

    // Create a dummy event
    let event = EvidenceEvent {
        specversion: "1.0".to_string(),
        type_: "assay.test.event".to_string(),
        // source should be "urn:assay:..." per SOTA
        // Using "urn:assay:test-producer"
        source: "urn:assay:test-producer".to_string(),

        // ID must match run:seq
        // run_id="run_fixed_test", seq=0 -> "run_fixed_test:0"
        id: "run_fixed_test:0".to_string(),
        time: Utc.timestamp_opt(1700000000, 0).unwrap(),
        data_content_type: "application/json".to_string(),
        subject: None,
        trace_parent: None,
        trace_state: None,

        run_id: "run_fixed_test".to_string(),
        seq: 0,
        producer: "assay".to_string(),
        producer_version: "2.5.0".to_string(),
        git_sha: "000000".to_string(),
        policy_id: None,
        contains_pii: false,
        contains_secrets: false,
        content_hash: None, // Optional in v1

        payload: serde_json::json!({"foo": "bar", "baz": 123}),
    };

    writer.add_event(event);
    writer.finish().expect("Failed to write bundle");

    buffer
}

fn sha256_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
