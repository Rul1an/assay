use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::lint_bundle;
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

#[test]
fn generate_sarif_sample() {
    // Build a bundle with a secret leaked in the subject field.
    // This triggers ASSAY-W001 (secret detection) and ASSAY-W003 (secret in subject).
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);

    let mut event = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        "sample-run-001",
        0,
        serde_json::json!({"url": "https://api.example.com"}),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event = event.with_subject("https://api.example.com?api_key=sk-1234567890");

    writer.add_event(event);
    writer.finish().unwrap();

    // Lint the bundle
    let report = lint_bundle(Cursor::new(&buffer), VerifyLimits::default()).unwrap();

    // Convert to SARIF and print
    let sarif = to_sarif(&report);
    println!("{}", serde_json::to_string_pretty(&sarif).unwrap());
}
