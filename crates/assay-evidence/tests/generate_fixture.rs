//! Generate a test bundle fixture for CI testing.
//!
//! Run with: cargo test -p assay-evidence --test generate_fixture -- --ignored --nocapture

use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::EvidenceEvent;
use chrono::{TimeZone, Utc};
use std::fs::File;
use std::path::Path;

fn create_event(event_type: &str, subject: &str, seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        event_type,
        "urn:assay:ci-test",
        "ci-test-run-001",
        seq,
        serde_json::json!({
            "test": true,
            "seq": seq,
            "subject": subject
        }),
    );
    event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
    event = event.with_subject(subject);
    event
}

#[test]
#[ignore] // Run manually to generate fixture
fn generate_test_bundle_fixture() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/evidence");

    std::fs::create_dir_all(&fixture_dir).unwrap();

    let bundle_path = fixture_dir.join("test-bundle.tar.gz");
    let mut file = File::create(&bundle_path).unwrap();

    {
        let mut writer = BundleWriter::new(&mut file);

        // Profile started
        writer.add_event(create_event("assay.profile.started", "ci-test", 0));

        // Some filesystem access
        writer.add_event(create_event("assay.fs.access", "/tmp/test-file.txt", 1));

        // Network connection
        writer.add_event(create_event("assay.net.connect", "api.example.com:443", 2));

        // Process execution
        writer.add_event(create_event("assay.process.exec", "echo", 3));

        // Profile finished
        let mut finish_event = EvidenceEvent::new(
            "assay.profile.finished",
            "urn:assay:ci-test",
            "ci-test-run-001",
            4,
            serde_json::json!({
                "event_count": 5,
                "status": "ok"
            }),
        );
        finish_event.time = Utc.timestamp_opt(1700000004, 0).unwrap();
        writer.add_event(finish_event);

        writer.finish().unwrap();
    }

    println!("Generated test bundle at: {}", bundle_path.display());

    // Verify the bundle is valid
    let file = File::open(&bundle_path).unwrap();
    let reader = assay_evidence::bundle::BundleReader::open(file).unwrap();
    println!(
        "Bundle info: {} events, run_id={}",
        reader.event_count(),
        reader.run_id()
    );
}
