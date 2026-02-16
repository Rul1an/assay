//! Pack loader with YAML parsing, validation, and digest computation.
//!
//! # YAML Parsing
//! - Rejects unknown fields (`deny_unknown_fields`).
//! - Duplicate keys: rejected when detected by the YAML parser (not guaranteed at all nesting levels).
//! - Anchors/aliases: currently accepted; future versions may reject.
//! - Computes deterministic digest: sha256(JCS(JSON(yaml)))

use super::schema::{PackDefinition, PackValidationError};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[path = "loader_internal/mod.rs"]
mod loader_internal;

/// Source of a loaded pack.
#[derive(Debug, Clone)]
pub enum PackSource {
    /// Built-in pack (embedded at compile time).
    BuiltIn(&'static str),
    /// Pack loaded from file.
    File(PathBuf),
}

impl std::fmt::Display for PackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackSource::BuiltIn(name) => write!(f, "builtin:{}", name),
            PackSource::File(path) => write!(f, "file:{}", path.display()),
        }
    }
}

/// A loaded and validated pack.
#[derive(Debug, Clone)]
pub struct LoadedPack {
    /// Pack definition.
    pub definition: PackDefinition,
    /// Pack digest (sha256 of JCS-canonical JSON).
    pub digest: String,
    /// Source of the pack.
    pub source: PackSource,
}

impl LoadedPack {
    /// Get the canonical ID for a rule.
    pub fn canonical_rule_id(&self, rule_id: &str) -> String {
        format!(
            "{}@{}:{}",
            self.definition.name, self.definition.version, rule_id
        )
    }
}

/// Pack loading error.
#[derive(Debug, Error)]
pub enum PackError {
    #[error("Pack '{reference}' not found. {suggestion}")]
    NotFound {
        reference: String,
        suggestion: String,
    },

    #[error("Failed to read pack file '{path}': {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse pack YAML: {message}")]
    YamlParseError { message: String },

    #[error("Pack validation failed: {0}")]
    ValidationError(#[from] PackValidationError),

    #[error("Pack '{pack}' requires Assay >={required}, but current version is {current}")]
    IncompatibleVersion {
        pack: String,
        required: String,
        current: String,
    },

    #[error(
        "Rule collision in compliance packs: {rule_id} defined in both '{pack_a}' and '{pack_b}'"
    )]
    ComplianceCollision {
        rule_id: String,
        pack_a: String,
        pack_b: String,
    },
}

/// Load a pack from a reference (file path or built-in name).
pub fn load_pack(reference: &str) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_impl(reference)
}

/// Validate pack name grammar per ADR-021.
///
/// Chars: a-z, 0-9, -
/// Must not start or end with hyphen.
#[cfg(test)]
fn is_valid_pack_name(name: &str) -> bool {
    loader_internal::resolve::is_valid_pack_name_impl(name)
}

/// Load multiple packs from references.
pub fn load_packs(references: &[String]) -> Result<Vec<LoadedPack>, PackError> {
    loader_internal::run::load_packs_impl(references)
}

/// Load a pack from a file path.
pub fn load_pack_from_file(path: &Path) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_from_file_impl(path)
}

/// Simple semver comparison (checks if current >= required).
#[cfg(test)]
fn version_satisfies(current: &str, required: &str) -> bool {
    loader_internal::compat::version_satisfies_impl(current, required)
}

/// Simple Levenshtein distance for fuzzy matching.
#[allow(clippy::needless_range_loop)]
#[cfg(test)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    loader_internal::resolve::levenshtein_distance_impl(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_satisfies() {
        assert!(version_satisfies("2.9.0", "2.9.0"));
        assert!(version_satisfies("2.10.0", "2.9.0"));
        assert!(version_satisfies("3.0.0", "2.9.0"));
        assert!(!version_satisfies("2.8.0", "2.9.0"));
        assert!(!version_satisfies("2.9.0", "2.10.0"));
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("eu-ai-act", "eu-ai-act"), 0);
        assert_eq!(levenshtein_distance("eu-ai-act", "eu-ai-act-baseline"), 9);
        assert_eq!(levenshtein_distance("euaiact", "eu-ai-act"), 2);
    }

    #[test]
    fn test_is_valid_pack_name() {
        // This test now uses the imported `is_valid_pack_name`
        assert!(is_valid_pack_name("simple"));
        assert!(is_valid_pack_name("eu-ai-act-baseline"));
        assert!(is_valid_pack_name("pack-v1"));
        assert!(is_valid_pack_name("123-pack"));

        assert!(!is_valid_pack_name(""));
        assert!(!is_valid_pack_name("-start"));
        assert!(!is_valid_pack_name("end-"));
        assert!(!is_valid_pack_name("Caps"));
        assert!(!is_valid_pack_name("dot.name"));
        assert!(!is_valid_pack_name("space name"));
        assert!(!is_valid_pack_name("/slash"));
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
        // Guard acquires lock and sets env; release/restore on drop
        let _env_guard = TestEnvGuard::new(&temp_dir);

        // Setup structure: $CONFIG/assay/packs/
        let config_home = temp_dir.path();
        let packs_dir = config_home.join("assay").join("packs");
        std::fs::create_dir_all(&packs_dir).unwrap();

        // 1. Create a local pack file: packs/local-pack.yaml
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

        // 2. Create a local pack dir: packs/dir-pack/pack.yaml
        let dir_pack_dir = packs_dir.join("dir-pack");
        std::fs::create_dir_all(&dir_pack_dir).unwrap();
        let dir_pack_content = pack_content.replace("local-pack", "dir-pack");
        std::fs::write(dir_pack_dir.join("pack.yaml"), dir_pack_content).unwrap();

        // Test resolution

        // Resolve file-based local pack
        let pack = load_pack("local-pack").expect("Should resolve local-pack");
        assert_eq!(pack.definition.name, "local-pack");
        assert!(matches!(pack.source, PackSource::File(_)));

        // Resolve dir-based local pack
        let pack = load_pack("dir-pack").expect("Should resolve dir-pack");
        assert_eq!(pack.definition.name, "dir-pack");
    }

    #[test]
    fn test_builtin_wins_over_local() {
        use tempfile::tempdir;
        let temp_dir = tempdir().unwrap();
        // Guard acquires lock and sets env; release/restore on drop
        let _env_guard = TestEnvGuard::new(&temp_dir);

        // Create local pack with built-in name
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

        // Load by name
        let pack = load_pack("eu-ai-act-baseline").expect("Should load");

        // MUST be built-in
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

        // Create packs/my-dir-pack/pack.yaml
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

        // Invalid YAML
        std::fs::write(packs_dir.join("broken.yaml"), ":: INVALID YAML ::").unwrap();

        let result = load_pack("broken");
        match result {
            Err(PackError::YamlParseError { .. }) => {} // Correct hard fail
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

        // Create a local pack with same name as built-in (eu-ai-act-baseline)
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

        // Load by name
        let pack = load_pack("eu-ai-act-baseline").expect("Should load");

        // Verify it is the BUILT-IN one (not spoofed)
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

        // Create a local file with a built-in name
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

        // Load by PATH
        let pack = load_pack(pack_path.to_str().unwrap()).expect("Should load by path");
        assert_eq!(pack.definition.description, "OVERRIDE");
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_escape_rejected() {
        use std::os::unix::fs::symlink;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        // Reuse TestEnvGuard for hygiene
        let _env_guard = TestEnvGuard::new(&temp_dir);

        let config_home = temp_dir.path(); // TestEnvGuard sets this to XDG_CONFIG_HOME
        let outside_dir = temp_dir.path().join("outside");

        std::fs::create_dir_all(&outside_dir).unwrap();

        // Create standard packs dir checking logic relies on XDG_CONFIG_HOME/assay/packs
        let packs_dir = config_home.join("assay").join("packs");
        std::fs::create_dir_all(&packs_dir).unwrap();

        // Create malicious pack OUTSIDE config dir
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

        // Create symlink INSIDE config dir -> pointing OUTSIDE
        symlink(
            outside_dir.join("malicious.yaml"),
            packs_dir.join("malicious.yaml"),
        )
        .unwrap();

        // Try to load
        let result = load_pack("malicious");
        match result {
            Err(PackError::ValidationError(PackValidationError::Safety(msg))) => {
                assert!(msg.contains("resolves outside config directory"));
            }
            Err(e) => panic!("Expected Safety error, got: {:?}", e),
            Ok(_) => panic!("Should verified failed loading symlinked pack"),
        }
    }
}
