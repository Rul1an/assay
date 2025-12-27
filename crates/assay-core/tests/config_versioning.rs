use assay_core::config::load_config;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_config_version_defaults() -> anyhow::Result<()> {
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
suite: test
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
"#
    )?;

    let cfg = load_config(tmp.path(), false)?;
    assert_eq!(cfg.version, 0, "Default version should be 0 (legacy)");
    assert!(cfg.is_legacy());
    Ok(())
}

#[test]
fn test_config_version_explicit_v1() -> anyhow::Result<()> {
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
configVersion: 1
suite: test
model: dummy
tests: []
"#
    )?;
    // Note: Empty tests usually error, but let's see if load_config enforces it.
    // Yes, check source: if cfg.tests.is_empty() { return Err(...) }
    // So we need a dummy test.
    writeln!(
        tmp,
        r#"
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
"#
    )?;

    // Re-write coherently
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
configVersion: 1
suite: test
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
"#
    )?;

    let cfg = load_config(tmp.path(), false)?;
    assert_eq!(cfg.version, 1, "Explicit version 1 should be respected");
    assert!(!cfg.is_legacy());
    Ok(())
}

#[test]
fn test_legacy_mode_override() -> anyhow::Result<()> {
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
configVersion: 1
suite: test
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
"#
    )?;

    // Legacy mode = true should force version to 0
    let cfg = load_config(tmp.path(), true)?;
    assert_eq!(cfg.version, 0, "Legacy mode should force version 0");
    assert!(cfg.is_legacy());
    Ok(())
}

#[test]
fn test_unsupported_version() -> anyhow::Result<()> {
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        r#"
configVersion: 999
suite: test
model: dummy
tests:
  - id: t1
    input: {{ prompt: "hi" }}
    expected: {{ type: must_contain, must_contain: ["hi"] }}
"#
    )?;

    let res = load_config(tmp.path(), false);
    assert!(res.is_err());
    let err = res.err().unwrap().to_string();
    assert!(err.contains("unsupported config version 999"));
    Ok(())
}
