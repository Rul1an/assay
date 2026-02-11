//! Pack loader with YAML parsing, validation, and digest computation.
//!
//! # YAML Parsing
//! - Rejects unknown fields (`deny_unknown_fields`).
//! - Duplicate keys: rejected when detected by the YAML parser (not guaranteed at all nesting levels).
//! - Anchors/aliases: currently accepted; future versions may reject.
//! - Computes deterministic digest: sha256(JCS(JSON(yaml)))

use super::schema::{PackDefinition, PackValidationError};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

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
    let path = Path::new(reference);

    // 1. If path exists on filesystem → load as file or dir
    if path.exists() {
        if path.is_dir() {
            let pack_yaml = path.join("pack.yaml");
            if pack_yaml.exists() {
                return load_pack_from_file(&pack_yaml);
            }
            // If dir exists but no pack.yaml, we fall through?
            // User requested: "for directories: pack.yaml required".
            // If the user explicitly passed a path, we should probably error if it's a dir without pack.yaml,
            // UNLESS it also happens to match a built-in name?
            // "Path step" accepts any existing path.
            // Let's strict fail if it's a directory and explicit path was likely intended?
            // But wait, if I run `assay lint --pack ./my-pack`, and `./my-pack` is a dir, I expect it to load `pack.yaml`.
            // If `pack.yaml` is missing, should I fail or check built-ins?
            // The requirement: "Precedence: Direct File Path > Built-in > Local".
            // If `reference` resolves to a filesystem path, it wins.
            // If it's a directory without `pack.yaml`, it's an invalid pack path.
            return Err(PackError::ReadError {
                path: pack_yaml,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Directory provided but 'pack.yaml' not found",
                ),
            });
        }
        return load_pack_from_file(path);
    }

    // 2. Check built-in packs by name
    if let Some((builtin_name, content)) = get_builtin_pack_with_name(reference) {
        return load_pack_from_string(content, PackSource::BuiltIn(builtin_name));
    }

    // 3. Local pack directory (valid name only; containment enforced in load)
    if is_valid_pack_name(reference) {
        if let Some(loaded) = try_load_from_config_dir(reference)? {
            return Ok(loaded);
        }
    }

    // 4. Registry / BYOS (future)

    // 5. Not found
    Err(PackError::NotFound {
        reference: reference.to_string(),
        suggestion: suggest_similar_pack(reference),
    })
}

/// Look up a built-in pack by name, returning both name and content.
fn get_builtin_pack_with_name(name: &str) -> Option<(&'static str, &'static str)> {
    super::BUILTIN_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, c)| (*n, *c))
}

/// Try to load a pack from the local config directory.
fn try_load_from_config_dir(name: &str) -> Result<Option<LoadedPack>, PackError> {
    let config_dir = match get_config_pack_dir() {
        Some(d) => d,
        None => return Ok(None),
    };

    if !config_dir.exists() {
        return Ok(None);
    }

    // Harden canonicalization: if dir exists, must canonicalize or error
    let canonical_config =
        std::fs::canonicalize(&config_dir).map_err(|e| PackError::ReadError {
            path: config_dir.clone(),
            source: e,
        })?;

    // Candidates:
    // 1. {config_dir}/{name}.yaml
    // 2. {config_dir}/{name}/pack.yaml
    let candidates = vec![
        config_dir.join(format!("{}.yaml", name)),
        config_dir.join(name).join("pack.yaml"),
    ];

    for path in candidates {
        if path.exists() {
            // SECURITY: Path containment check
            // Use canonicalize() to resolve symlinks and '..'
            let canonical_path =
                std::fs::canonicalize(&path).map_err(|e| PackError::ReadError {
                    path: path.clone(),
                    source: e,
                })?;

            if !canonical_path.starts_with(&canonical_config) {
                // Determine if this is a symlink escape attempt
                return Err(PackValidationError::Safety(format!(
                    "Pack path '{}' resolves outside config directory '{}'",
                    path.display(),
                    config_dir.display()
                ))
                .into());
            }

            // Mitigate TOCTOU: load from the canonical path we just verified
            return Ok(Some(load_pack_from_file(&canonical_path)?));
        }
    }

    Ok(None)
}

/// Determine the config pack directory per ADR-021.
fn get_config_pack_dir() -> Option<PathBuf> {
    // Unix: $XDG_CONFIG_HOME/assay/packs or ~/.config/assay/packs
    // Windows: %APPDATA%\assay\packs

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("assay").join("packs"));
            }
        }

        // Fallback: ~/.config/assay/packs
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("assay")
                    .join("packs"),
            );
        }
    }

    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("assay").join("packs"));
        }
    }

    None
}

/// Validate pack name grammar per ADR-021.
///
/// Chars: a-z, 0-9, -
/// Must not start or end with hyphen.
fn is_valid_pack_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Load multiple packs from references.
pub fn load_packs(references: &[String]) -> Result<Vec<LoadedPack>, PackError> {
    let mut packs = Vec::with_capacity(references.len());
    for reference in references {
        packs.push(load_pack(reference)?);
    }
    Ok(packs)
}

/// Load a pack from a file path.
pub fn load_pack_from_file(path: &Path) -> Result<LoadedPack, PackError> {
    let content = std::fs::read_to_string(path).map_err(|e| PackError::ReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    load_pack_from_string(&content, PackSource::File(path.to_path_buf()))
}

/// Load pack from string content.
///
/// Implements strict YAML parsing for the Pack Engine v1 spec:
/// - Rejects duplicate keys (partially via serde map handling)
/// - Rejects unknown fields (`deny_unknown_fields`)
///
/// Note: While the spec discourages anchors/aliases, the current implementation
/// accepts them if they resolve to valid JSON. Future versions may enforce failing on anchors/aliases.
fn load_pack_from_string(content: &str, source: PackSource) -> Result<LoadedPack, PackError> {
    // Parse YAML with strict settings (deny_unknown_fields on schema types)
    let definition: PackDefinition =
        serde_yaml::from_str(content).map_err(|e| PackError::YamlParseError {
            message: format_yaml_error(e),
        })?;

    // Validate the pack
    definition.validate()?;

    // Check version compatibility
    check_version_compatibility(&definition)?;

    // Compute digest
    let digest = compute_pack_digest(&definition)?;

    Ok(LoadedPack {
        definition,
        digest,
        source,
    })
}

/// Compute pack digest: sha256(JCS(JSON(pack)))
fn compute_pack_digest(definition: &PackDefinition) -> Result<String, PackError> {
    // Serialize to canonical JSON using RFC 8785 JCS
    let canonical = serde_jcs::to_string(definition).map_err(|e| PackError::YamlParseError {
        message: format!("Failed to canonicalize pack to JCS JSON: {}", e),
    })?;

    // Compute SHA-256
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hash)))
}

/// Check if current Assay version satisfies pack requirements.
fn check_version_compatibility(definition: &PackDefinition) -> Result<(), PackError> {
    let current_version = env!("CARGO_PKG_VERSION");
    let required = &definition.requires.assay_min_version;

    // Parse the required version constraint
    // Format: ">=X.Y.Z" or just "X.Y.Z"
    let required_version = required.trim_start_matches(">=").trim_start_matches("=");

    // Simple semver comparison (major.minor.patch)
    if !version_satisfies(current_version, required_version) {
        return Err(PackError::IncompatibleVersion {
            pack: definition.name.clone(),
            required: required.clone(),
            current: current_version.to_string(),
        });
    }

    Ok(())
}

/// Simple semver comparison (checks if current >= required).
fn version_satisfies(current: &str, required: &str) -> bool {
    let parse_version = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() >= 3 {
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].split('-').next()?.parse().ok()?,
            ))
        } else if parts.len() == 2 {
            Some((parts[0].parse().ok()?, parts[1].parse().ok()?, 0))
        } else {
            None
        }
    };

    match (parse_version(current), parse_version(required)) {
        (Some((c_major, c_minor, c_patch)), Some((r_major, r_minor, r_patch))) => {
            (c_major, c_minor, c_patch) >= (r_major, r_minor, r_patch)
        }
        _ => false, // If we can't parse, treat as incompatible (fail closed)
    }
}

/// Format YAML parsing error for user-friendly display.
fn format_yaml_error(e: serde_yaml::Error) -> String {
    let msg = e.to_string();

    // Check for duplicate key errors
    if msg.contains("duplicate key") {
        return format!(
            "Duplicate key detected (duplicate keys not allowed for security): {}",
            msg
        );
    }

    // Check for unknown field errors
    if msg.contains("unknown field") {
        return format!(
            "Unknown field detected (prevents digest bypass attacks): {}",
            msg
        );
    }

    msg
}

/// Suggest similar pack names.
fn suggest_similar_pack(reference: &str) -> String {
    use super::BUILTIN_PACKS;

    // Simple prefix matching for suggestions
    let suggestions: Vec<&str> = BUILTIN_PACKS
        .iter()
        .filter(|(name, _)| {
            name.starts_with(reference)
                || reference.starts_with(*name)
                || levenshtein_distance(name, reference) <= 3
        })
        .map(|(name, _)| *name)
        .collect();

    if suggestions.is_empty() {
        format!(
            "Available built-in packs: {}. Or specify a file path: --pack ./my-pack.yaml",
            BUILTIN_PACKS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("Did you mean '{}'?", suggestions.join("' or '"))
    }
}

/// Simple Levenshtein distance for fuzzy matching.
#[allow(clippy::needless_range_loop)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
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
