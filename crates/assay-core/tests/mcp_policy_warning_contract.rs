use assay_core::mcp::policy::McpPolicy;

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
