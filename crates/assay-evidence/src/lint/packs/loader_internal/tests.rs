use super::{compat, resolve};
use crate::lint::packs::loader::{load_pack, PackError, PackSource};
use crate::lint::packs::schema::PackValidationError;

#[test]
fn test_version_satisfies() {
    assert!(compat::version_satisfies_impl("2.9.0", "2.9.0"));
    assert!(compat::version_satisfies_impl("2.10.0", "2.9.0"));
    assert!(compat::version_satisfies_impl("3.0.0", "2.9.0"));
    assert!(!compat::version_satisfies_impl("2.8.0", "2.9.0"));
    assert!(!compat::version_satisfies_impl("2.9.0", "2.10.0"));
}

#[test]
fn test_levenshtein_distance() {
    assert_eq!(
        resolve::levenshtein_distance_impl("eu-ai-act", "eu-ai-act"),
        0
    );
    assert_eq!(
        resolve::levenshtein_distance_impl("eu-ai-act", "eu-ai-act-baseline"),
        9
    );
    assert_eq!(
        resolve::levenshtein_distance_impl("euaiact", "eu-ai-act"),
        2
    );
}

#[test]
fn test_is_valid_pack_name() {
    assert!(resolve::is_valid_pack_name_impl("simple"));
    assert!(resolve::is_valid_pack_name_impl("eu-ai-act-baseline"));
    assert!(resolve::is_valid_pack_name_impl("pack-v1"));
    assert!(resolve::is_valid_pack_name_impl("123-pack"));

    assert!(!resolve::is_valid_pack_name_impl(""));
    assert!(!resolve::is_valid_pack_name_impl("-start"));
    assert!(!resolve::is_valid_pack_name_impl("end-"));
    assert!(!resolve::is_valid_pack_name_impl("Caps"));
    assert!(!resolve::is_valid_pack_name_impl("dot.name"));
    assert!(!resolve::is_valid_pack_name_impl("space name"));
    assert!(!resolve::is_valid_pack_name_impl("/slash"));
}

// Mutex to serialize tests that modify environment variables
static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII Guard for test environment.
/// Sets XDG_CONFIG_HOME/APPDATA on creation, restores/clears on drop.
struct TestEnvGuard {
    _mutex_guard: std::sync::MutexGuard<'static, ()>,
    original_xdg: Option<String>,
    #[cfg(windows)]
    original_appdata: Option<String>,
}

impl TestEnvGuard {
    fn new(temp_dir: &tempfile::TempDir) -> Self {
        let guard = ENV_MUTEX.lock().unwrap();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        #[cfg(windows)]
        let original_appdata = std::env::var("APPDATA").ok();

        let path = temp_dir.path();
        std::env::set_var("XDG_CONFIG_HOME", path);
        #[cfg(windows)]
        std::env::set_var("APPDATA", path);

        Self {
            _mutex_guard: guard,
            original_xdg,
            #[cfg(windows)]
            original_appdata,
        }
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        match &self.original_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        #[cfg(windows)]
        match &self.original_appdata {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}

#[test]
fn test_local_pack_resolution() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let config_home = temp_dir.path();
    let packs_dir = config_home.join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let pack_content = r#"
name: local-pack
version: 1.0.0
kind: compliance
description: Test Local Pack
author: Me
license: MIT
disclaimer: Test disclaimer
requires:
  assay_min_version: "0.0.0"
rules:
  - id: LOC-001
    severity: info
    description: Local rule
    check:
      type: event_count
      min: 1
"#;
    std::fs::write(packs_dir.join("local-pack.yaml"), pack_content).unwrap();

    let dir_pack_dir = packs_dir.join("dir-pack");
    std::fs::create_dir_all(&dir_pack_dir).unwrap();
    let dir_pack_content = pack_content.replace("local-pack", "dir-pack");
    std::fs::write(dir_pack_dir.join("pack.yaml"), dir_pack_content).unwrap();

    let pack = load_pack("local-pack").expect("Should resolve local-pack");
    assert_eq!(pack.definition.name, "local-pack");
    assert!(matches!(pack.source, PackSource::File(_)));

    let pack = load_pack("dir-pack").expect("Should resolve dir-pack");
    assert_eq!(pack.definition.name, "dir-pack");
}

#[test]
fn test_builtin_wins_over_local() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let local_content = r#"
name: eu-ai-act-baseline
version: 9.9.9
kind: compliance
description: LOCAL SPOOF
author: Attacker
license: MIT
disclaimer: Spoof
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(packs_dir.join("eu-ai-act-baseline.yaml"), local_content).unwrap();

    let pack = load_pack("eu-ai-act-baseline").expect("Should load");
    match pack.source {
        PackSource::BuiltIn(_) => {}
        _ => panic!("Expected BuiltIn source, got {:?}", pack.source),
    }
    assert_ne!(pack.definition.description, "LOCAL SPOOF");
}

#[test]
fn test_local_resolves_name_dir_pack_yaml() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    let pack_dir = packs_dir.join("my-dir-pack");
    std::fs::create_dir_all(&pack_dir).unwrap();

    let content = r#"
name: my-dir-pack
version: 1.0.0
kind: compliance
description: Dir Pack
author: Me
license: MIT
disclaimer: Test
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(pack_dir.join("pack.yaml"), content).unwrap();

    let pack = load_pack("my-dir-pack").expect("Should resolve dir pack");
    assert_eq!(pack.definition.name, "my-dir-pack");
}

#[test]
fn test_local_invalid_yaml_fails() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::write(packs_dir.join("broken.yaml"), ":: INVALID YAML ::").unwrap();

    let result = load_pack("broken");
    match result {
        Err(PackError::YamlParseError { .. }) => {}
        Ok(_) => panic!("Should have failed parsing"),
        Err(e) => panic!("Expected YamlParseError, got {:?}", e),
    }
}

#[test]
fn test_resolution_order_mock() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let spoof_content = r#"
name: eu-ai-act-baseline
version: 9.9.9
kind: compliance
description: SPOOFED PACK
author: Attacker
license: MIT
disclaimer: Spoof disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(packs_dir.join("eu-ai-act-baseline.yaml"), spoof_content).unwrap();

    let pack = load_pack("eu-ai-act-baseline").expect("Should load");
    match pack.source {
        PackSource::BuiltIn(_) => {}
        _ => panic!(
            "Should have loaded built-in pack, but got {:?}",
            pack.source
        ),
    }
    assert_ne!(pack.definition.description, "SPOOFED PACK");
}

#[test]
fn test_path_wins_over_builtin() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();

    let pack_path = temp_dir.path().join("eu-ai-act-baseline.yaml");
    let override_content = r#"
name: eu-ai-act-baseline
version: 0.0.0
kind: compliance
description: OVERRIDE
author: Me
license: MIT
disclaimer: Override disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(&pack_path, override_content).unwrap();

    let pack = load_pack(pack_path.to_str().unwrap()).expect("Should load by path");
    assert_eq!(pack.definition.description, "OVERRIDE");
}

#[test]
#[cfg(unix)]
fn test_symlink_escape_rejected() {
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let config_home = temp_dir.path();
    let outside_dir = temp_dir.path().join("outside");

    std::fs::create_dir_all(&outside_dir).unwrap();
    let packs_dir = config_home.join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let malicious_content = r#"
name: malicious
version: 1.0.0
kind: compliance
description: Evil
author: Hacker
license: MIT
disclaimer: Evil disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(outside_dir.join("malicious.yaml"), malicious_content).unwrap();

    symlink(
        outside_dir.join("malicious.yaml"),
        packs_dir.join("malicious.yaml"),
    )
    .unwrap();

    let result = load_pack("malicious");
    match result {
        Err(PackError::ValidationError(PackValidationError::Safety(msg))) => {
            assert!(msg.contains("resolves outside config directory"));
        }
        Err(e) => panic!("Expected Safety error, got: {:?}", e),
        Ok(_) => panic!("Should verified failed loading symlinked pack"),
    }
}
