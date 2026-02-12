//! Integration tests for `assay evidence verify --eval` (ADR-025 E2 Phase 3).

use assay_evidence::bundle::BundleWriter;
use assay_evidence::evaluation::verify_evaluation;
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::packs::load_packs;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

fn create_bundle_bytes() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for seq in 0..2u64 {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_verify_eval",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
        writer.add_event(event);
    }
    writer.finish().unwrap();
    buffer
}

#[test]
fn test_verify_eval_roundtrip() {
    let bundle_bytes = create_bundle_bytes();
    let packs = load_packs(&["cicd-starter".into()]).unwrap();
    let options = LintOptions {
        packs,
        max_results: Some(500),
        bundle_path: Some("bundle.tar.gz".into()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&bundle_bytes), VerifyLimits::default(), options)
            .unwrap();

    let packs_applied: Vec<_> = result
        .pack_meta
        .as_ref()
        .map(|m| {
            m.packs
                .iter()
                .map(|p| assay_evidence::evaluation::PackApplied {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    kind: format!("{}", p.kind),
                    digest: p.digest.clone(),
                    source: if p.source_url.is_some() {
                        "file"
                    } else {
                        "builtin"
                    }
                    .into(),
                })
                .collect()
        })
        .unwrap_or_default();

    let eval = assay_evidence::evaluation::build_evaluation_from_lint(
        &result.report,
        packs_applied,
        "assay evidence lint",
        "1.0",
        vec!["bundle.tar.gz".into()],
        assay_evidence::lint::Severity::Error,
        "550e8400-e29b-41d4-a716-446655440000".into(),
    )
    .unwrap();

    let verify_result = assay_evidence::bundle::verify_bundle(Cursor::new(&bundle_bytes)).unwrap();
    let manifest = verify_result.manifest;

    let result = verify_evaluation(&eval, &manifest, None, false).unwrap();
    assert!(result.ok, "errors: {:?}", result.errors);
    assert!(result.bundle_digest_match);
    assert!(result.manifest_digest_match);
    assert!(result.results_digest_verified);
}
