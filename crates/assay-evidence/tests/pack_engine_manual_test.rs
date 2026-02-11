//! Manual test for pack engine with EU AI Act baseline pack.
//!
//! Run with: cargo test -p assay-evidence --test pack_engine_manual_test -- --nocapture

use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::packs::load_packs;
use assay_evidence::lint::sarif::{to_sarif_with_options, SarifOptions};
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

/// Create a bundle that passes all EU AI Act baseline checks.
fn create_compliant_bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);

    // Event 0: run started (for EU12-001 + EU12-002)
    let mut event = EvidenceEvent::new(
        "assay.run.started",
        "urn:assay:test",
        "run_euai_compliant",
        0,
        serde_json::json!({
            "run_id": "run_euai_compliant",
            "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            "version": "1.0.0"
        }),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    writer.add_event(event);

    // Event 1: policy decision (for EU12-004)
    let mut event = EvidenceEvent::new(
        "assay.policy.evaluated",
        "urn:assay:test",
        "run_euai_compliant",
        1,
        serde_json::json!({
            "policy_decision": "allow",
            "policy_hash": "sha256:abc123",
            "denied": false
        }),
    );
    event.time = Utc.timestamp_opt(1700000001, 0).unwrap();
    writer.add_event(event);

    // Event 2: run finished (for EU12-002)
    let mut event = EvidenceEvent::new(
        "assay.run.finished",
        "urn:assay:test",
        "run_euai_compliant",
        2,
        serde_json::json!({
            "run_id": "run_euai_compliant",
            "duration_ms": 1000,
            "exit_code": 0
        }),
    );
    event.time = Utc.timestamp_opt(1700000002, 0).unwrap();
    writer.add_event(event);

    writer.finish().unwrap();
    buffer
}

/// Create a bundle that fails some EU AI Act baseline checks.
fn create_non_compliant_bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);

    // Single event without lifecycle or correlation fields
    let mut event = EvidenceEvent::new(
        "some.random.event",
        "urn:assay:test",
        "run_non_compliant",
        0,
        serde_json::json!({
            "message": "This event lacks required fields"
        }),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    writer.add_event(event);

    writer.finish().unwrap();
    buffer
}

#[test]
fn test_eu_ai_act_baseline_compliant() {
    let bundle = create_compliant_bundle();
    let packs = load_packs(&["eu-ai-act-baseline".to_string()]).expect("Failed to load pack");

    let options = LintOptions {
        packs,
        max_results: Some(500),
        bundle_path: Some("test_compliant.tar.gz".to_string()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&bundle), VerifyLimits::default(), options).unwrap();

    println!("\n=== EU AI Act Baseline - Compliant Bundle ===");
    println!("Verified: {}", result.report.verified);
    println!("Findings: {}", result.report.findings.len());

    for finding in &result.report.findings {
        println!(
            "  [{:?}] {} - {}",
            finding.severity, finding.rule_id, finding.message
        );
    }

    if let Some(meta) = &result.pack_meta {
        println!("\nPacks executed:");
        for pack in &meta.packs {
            println!("  - {}@{} ({})", pack.name, pack.version, pack.digest);
        }
        if let Some(disclaimer) = &meta.disclaimer {
            println!("\nDisclaimer:\n{}", disclaimer);
        }
    }

    // Compliant bundle should have no errors from pack rules
    let pack_errors: Vec<_> = result
        .report
        .findings
        .iter()
        .filter(|f| f.rule_id.starts_with("eu-ai-act-baseline@"))
        .filter(|f| f.severity == assay_evidence::lint::Severity::Error)
        .collect();

    assert!(
        pack_errors.is_empty(),
        "Expected no error-level pack findings, got: {:?}",
        pack_errors
    );
}

#[test]
fn test_eu_ai_act_baseline_non_compliant() {
    let bundle = create_non_compliant_bundle();
    let packs = load_packs(&["eu-ai-act-baseline".to_string()]).expect("Failed to load pack");

    let options = LintOptions {
        packs,
        max_results: Some(500),
        bundle_path: Some("test_non_compliant.tar.gz".to_string()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&bundle), VerifyLimits::default(), options).unwrap();

    println!("\n=== EU AI Act Baseline - Non-Compliant Bundle ===");
    println!("Verified: {}", result.report.verified);
    println!("Findings: {}", result.report.findings.len());

    for finding in &result.report.findings {
        let article_ref = finding
            .tags
            .iter()
            .find(|t| t.starts_with("article_ref:"))
            .map(|t| t.strip_prefix("article_ref:").unwrap_or(""))
            .unwrap_or("");
        println!(
            "  [{:?}] {} - {} [Article {}]",
            finding.severity, finding.rule_id, finding.message, article_ref
        );
    }

    // Non-compliant bundle should have findings
    let pack_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter(|f| f.rule_id.starts_with("eu-ai-act-baseline@"))
        .collect();

    assert!(
        !pack_findings.is_empty(),
        "Expected pack findings for non-compliant bundle"
    );

    // Should have EU12-002 (missing start/finish pairs)
    let eu12_002 = pack_findings
        .iter()
        .find(|f| f.rule_id.contains(":EU12-002"));
    assert!(
        eu12_002.is_some(),
        "Expected EU12-002 finding (missing event pairs)"
    );
}

#[test]
fn test_sarif_output_with_pack_metadata() {
    let bundle = create_non_compliant_bundle();
    let packs = load_packs(&["eu-ai-act-baseline".to_string()]).expect("Failed to load pack");

    let options = LintOptions {
        packs: packs.clone(),
        max_results: Some(500),
        bundle_path: Some("test_sarif.tar.gz".to_string()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&bundle), VerifyLimits::default(), options).unwrap();

    #[allow(deprecated)]
    let sarif_options = SarifOptions {
        pack_meta: result.pack_meta.clone(),
        bundle_path: Some("test_sarif.tar.gz".to_string()),
        working_directory: None, // Deprecated: no longer included in output
    };

    let sarif = to_sarif_with_options(&result.report, sarif_options);

    println!("\n=== SARIF Output Sample ===");
    println!("{}", serde_json::to_string_pretty(&sarif).unwrap());

    // Verify SARIF structure
    assert_eq!(sarif["version"], "2.1.0");

    let run = &sarif["runs"][0];

    // Check pack metadata in driver properties
    let driver_props = &run["tool"]["driver"]["properties"];
    assert!(
        driver_props["assayPacks"].is_array(),
        "Expected assayPacks in driver properties"
    );

    // Check disclaimer in run properties
    let run_props = &run["properties"];
    assert!(
        run_props["disclaimer"].is_string(),
        "Expected disclaimer in run properties"
    );

    // Check invocations
    assert!(run["invocations"].is_array());
    let invocation = &run["invocations"][0];
    assert!(invocation["executionSuccessful"].as_bool().unwrap_or(false));
    // workingDirectory is intentionally NOT included (privacy: avoid path leakage)
    assert!(
        invocation["workingDirectory"].is_null(),
        "workingDirectory should not be present in SARIF"
    );

    // Check results have locations
    let results = run["results"].as_array().unwrap();
    for result in results {
        assert!(
            result["locations"].is_array(),
            "Expected locations on all results"
        );
        let locations = result["locations"].as_array().unwrap();
        assert!(!locations.is_empty(), "Expected at least one location");

        // Check primaryLocationLineHash
        let fingerprints = &result["partialFingerprints"];
        assert!(
            fingerprints["primaryLocationLineHash"].is_string()
                || fingerprints["assayLintFingerprint/v1"].is_string(),
            "Expected fingerprint"
        );
    }
}

#[test]
fn test_cicd_starter_pack_loads_and_runs() {
    // Bundle with events, profile lifecycle, traceparent, build_id — passes all CICD rules
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);

    let mut event = EvidenceEvent::new(
        "assay.profile.started",
        "urn:assay:test",
        "run_cicd",
        0,
        serde_json::json!({
            "run_id": "run-cicd-123",
            "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            "build_id": "build-456",
            "version": "1.0.0"
        }),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    writer.add_event(event);

    let mut event = EvidenceEvent::new(
        "assay.profile.finished",
        "urn:assay:test",
        "run_cicd",
        1,
        serde_json::json!({ "exit_code": 0 }),
    );
    event.time = Utc.timestamp_opt(1700000001, 0).unwrap();
    writer.add_event(event);

    writer.finish().unwrap();

    let packs = load_packs(&["cicd-starter".to_string()]).expect("cicd-starter must be built-in");
    let options = LintOptions {
        packs,
        max_results: Some(500),
        bundle_path: Some("test_cicd.tar.gz".to_string()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&buffer), VerifyLimits::default(), options).unwrap();

    let cicd_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter(|f| f.rule_id.starts_with("cicd-starter@"))
        .collect();

    assert!(
        cicd_findings.is_empty(),
        "Full CICD-compliant bundle should pass all cicd-starter rules, got: {:?}",
        cicd_findings
    );
}

#[test]
fn test_cicd_starter_minimal_bundle_fails_cicd_002() {
    // Single event without profile started/finished — fails CICD-002
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    let mut event = EvidenceEvent::new(
        "assay.test.event",
        "urn:assay:test",
        "run_minimal",
        0,
        serde_json::json!({ "msg": "no lifecycle" }),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    writer.add_event(event);
    writer.finish().unwrap();

    let packs = load_packs(&["cicd-starter".to_string()]).expect("cicd-starter must be built-in");
    let options = LintOptions {
        packs,
        max_results: Some(500),
        bundle_path: Some("minimal.tar.gz".to_string()),
    };

    let result =
        lint_bundle_with_options(Cursor::new(&buffer), VerifyLimits::default(), options).unwrap();

    let cicd_002 = result
        .report
        .findings
        .iter()
        .find(|f| f.rule_id.contains(":CICD-002"));
    assert!(
        cicd_002.is_some(),
        "Bundle without profile lifecycle must fail CICD-002"
    );
}

/// Generate a bundle file for manual CLI testing
#[test]
#[ignore] // Run manually with: cargo test -p assay-evidence --test pack_engine_manual_test generate_test_bundle -- --ignored --nocapture
fn generate_test_bundle() {
    use std::fs::File;
    use std::io::Write;

    let compliant = create_compliant_bundle();
    let non_compliant = create_non_compliant_bundle();

    let mut f = File::create("/tmp/test_compliant.tar.gz").unwrap();
    f.write_all(&compliant).unwrap();
    println!("Created: /tmp/test_compliant.tar.gz");

    let mut f = File::create("/tmp/test_non_compliant.tar.gz").unwrap();
    f.write_all(&non_compliant).unwrap();
    println!("Created: /tmp/test_non_compliant.tar.gz");

    println!("\nTest with:");
    println!(
        "  ./target/debug/assay evidence lint /tmp/test_compliant.tar.gz --pack eu-ai-act-baseline"
    );
    println!("  ./target/debug/assay evidence lint /tmp/test_non_compliant.tar.gz --pack eu-ai-act-baseline");
    println!(
        "  ./target/debug/assay evidence lint /tmp/test_non_compliant.tar.gz --pack eu-ai-act-baseline --format sarif"
    );
}
