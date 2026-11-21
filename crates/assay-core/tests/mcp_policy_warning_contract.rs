//! Current readers direct legacy input to the converter. Typos in current fields remain errors.
use assay_core::mcp::policy::McpPolicy;

#[test]
fn migration_guidance_is_specific_to_legacy_fields() {
    for (yaml, key, legacy) in [
        ("deny: []", "deny", true),
        (
            "tools: {unknown_field_xyz: true}",
            "tools.unknown_field_xyz",
            false,
        ),
    ] {
        let error = McpPolicy::from_slice(yaml.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains(key), "{error}");
        assert_eq!(error.contains("assay policy migrate"), legacy, "{error}");
    }
}

#[test]
fn v1_file_loader_reports_the_unknown_field_path() {
    let fixture = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        fixture.path(),
        "version: '1.0'\ntools: {unknown_field_xyz: true}",
    )
    .unwrap();
    let error = McpPolicy::from_file(fixture.path()).unwrap_err();
    assert!(
        error.to_string().contains("tools.unknown_field_xyz"),
        "{error:#}"
    );
}
