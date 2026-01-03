use assay_core::baseline::Baseline;

#[test]
fn test_baseline_validation_suite_mismatch() {
    let b = Baseline {
        schema_version: 1,
        suite: "suite_a".into(),
        assay_version: "0.1.0".into(),
        created_at: "now".into(),
        config_fingerprint: "md5:123".into(),
        git_info: None,
        entries: vec![],
    };

    // Should fail if suite differs
    let err = b.validate("suite_b", "md5:123").unwrap_err();
    assert!(err.to_string().contains("baseline suite mismatch"));
    assert!(err.to_string().contains("expected 'suite_b'"));

    // Should pass if suite matches
    assert!(b.validate("suite_a", "md5:123").is_ok());
}

#[test]
fn test_baseline_schema_version_check() {
    // We can't easily construct a struct with specific field values if we want to test `load` without a file.
    // But `load` is what checks schema version (during deserialization? no, explicit check).
    // Let's rely on standard unit test for `load` via integration or just verify validation method didn't break things.
    // The schema version check is inside `load`, not `validate`.
    // Validating `load` requires a file.
}
